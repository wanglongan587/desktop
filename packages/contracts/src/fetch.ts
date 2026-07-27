import { ContractTransportError, type ContractCallOptions, type ContractErrorPayload, type ContractStreamFrame, type ContractTransport, type ContractTransportRequest } from "./transport.js";

const MAX_FRAME_BYTES = 8 * 1024 * 1024;

export type FetchTransportOptions = {
  baseUrl?: string;
  fetch?: typeof globalThis.fetch;
};

export function createFetchTransport(
  options: FetchTransportOptions = {},
): ContractTransport {
  const fetchImplementation = options.fetch ?? globalThis.fetch;

  if (fetchImplementation === undefined) {
    throw new Error("global fetch is not available");
  }

  return {
    async send<TResponse>(request: ContractTransportRequest, callOptions?: ContractCallOptions): Promise<TResponse> {
      const response = await fetchImplementation(resolveUrl(options.baseUrl ?? "", request.path), {
        method: request.method,
        headers: request.headers,
        body: request.body === undefined ? undefined : JSON.stringify(request.body),
        ...(callOptions?.signal === undefined ? {} : { signal: callOptions.signal }),
      });
      const responseBody = await readResponseBody(response);

      if (!response.ok) {
        throw toTransportError(response.status, responseBody);
      }

      return responseBody as TResponse;
    },
    stream<TEvent>(request: ContractTransportRequest, callOptions?: ContractCallOptions): AsyncIterable<TEvent> {
      let consumed = false;
      return {
        [Symbol.asyncIterator](): AsyncIterator<TEvent> {
          if (consumed) {
            throw new ContractTransportError({
              code: "stream_already_consumed",
              message: "contract streams can only be consumed once",
              status: null,
              responseBody: null,
            });
          }
          consumed = true;
          return readNdjsonStream(fetchImplementation, options.baseUrl ?? "", request, callOptions);
        },
      };
    },
  };
}

/** Starts a fetch stream on first iteration and validates every private transport frame. */
async function* readNdjsonStream<TEvent>(
  fetchImplementation: typeof globalThis.fetch,
  baseUrl: string,
  request: ContractTransportRequest,
  callOptions?: ContractCallOptions,
): AsyncGenerator<TEvent> {
  const controller = new AbortController();
  const abort = () => controller.abort(callOptions?.signal?.reason);
  callOptions?.signal?.addEventListener("abort", abort, { once: true });

  try {
    if (callOptions?.signal?.aborted === true) abort();
    const response = await fetchImplementation(resolveUrl(baseUrl, request.path), {
      method: request.method,
      headers: { ...request.headers, accept: "application/x-ndjson" },
      body: request.body === undefined ? undefined : JSON.stringify(request.body),
      signal: controller.signal,
    });
    if (!response.ok) {
      throw toTransportError(response.status, await readResponseBody(response));
    }
    if (response.body === null) {
      throw streamError("stream_interrupted", "stream response body is unavailable");
    }

    const reader = response.body.getReader();
    const decoder = new TextDecoder("utf-8", { fatal: true });
    let buffered = "";
    let ended = false;
    try {
      while (!ended) {
        const chunk = await reader.read();
        if (chunk.done) break;
        buffered += decoder.decode(chunk.value, { stream: true });
        if (new TextEncoder().encode(buffered).byteLength > MAX_FRAME_BYTES && !buffered.includes("\n")) {
          throw streamError("stream_frame_too_large", "contract stream frame exceeds 8 MiB");
        }
        let newline = buffered.indexOf("\n");
        while (newline >= 0) {
          const line = buffered.slice(0, newline);
          buffered = buffered.slice(newline + 1);
          if (new TextEncoder().encode(line).byteLength > MAX_FRAME_BYTES) {
            throw streamError("stream_frame_too_large", "contract stream frame exceeds 8 MiB");
          }
          if (line !== "") {
            const frame = decodeStreamFrame<TEvent>(line);
            if (frame.type === "data") yield frame.data;
            if (frame.type === "error") {
              throw new ContractTransportError({ ...frame.error, status: response.status, responseBody: frame });
            }
            if (frame.type === "end") ended = true;
          }
          newline = buffered.indexOf("\n");
        }
      }
      if (!ended) throw streamError("stream_interrupted", "contract stream closed before its end frame");
    } finally {
      await reader.cancel().catch(() => undefined);
    }
  } finally {
    callOptions?.signal?.removeEventListener("abort", abort);
    controller.abort();
  }
}

/** Decodes one NDJSON frame without accepting raw business events on the transport boundary. */
function decodeStreamFrame<TEvent>(line: string): ContractStreamFrame<TEvent> {
  let value: unknown;
  try {
    value = JSON.parse(line) as unknown;
  } catch {
    throw streamError("invalid_stream_frame", "contract stream emitted invalid JSON");
  }
  if (!isRecord(value)) {
    throw streamError("invalid_stream_frame", "contract stream emitted an invalid frame");
  }
  if (value.type === "data" && "data" in value) {
    return value as ContractStreamFrame<TEvent>;
  }
  if (
    value.type === "error" &&
    isRecord(value.error) &&
    typeof value.error.code === "string" &&
    typeof value.error.message === "string"
  ) {
    return value as ContractStreamFrame<TEvent>;
  }
  if (value.type === "end") {
    return value as ContractStreamFrame<TEvent>;
  }
  throw streamError("invalid_stream_frame", "contract stream emitted an invalid frame");
}

/** Creates transport failures that are independent of an HTTP status response. */
function streamError(code: string, message: string): ContractTransportError {
  return new ContractTransportError({ code, message, status: null, responseBody: null });
}

export function resolveUrl(baseUrl: string, path: string): string {
  if (baseUrl === "") {
    return path;
  }

  return new URL(path, baseUrl).toString();
}

export function decodeErrorEnvelope(body: unknown): ContractErrorPayload | null {
  if (!isRecord(body)) {
    return null;
  }

  const error = body.error;

  if (!isRecord(error) || typeof error.code !== "string" || typeof error.message !== "string") {
    return null;
  }

  return {
    code: error.code,
    message: error.message,
  };
}

async function readResponseBody(response: Response): Promise<unknown> {
  const bodyText = await response.text();

  if (bodyText === "") {
    return null;
  }

  try {
    return JSON.parse(bodyText) as unknown;
  } catch {
    return bodyText;
  }
}

function toTransportError(status: number, responseBody: unknown): ContractTransportError {
  const decodedError = decodeErrorEnvelope(responseBody);

  if (decodedError !== null) {
    return new ContractTransportError({
      code: decodedError.code,
      message: decodedError.message,
      status,
      responseBody,
    });
  }

  return new ContractTransportError({
    code: "http_error",
    message: `HTTP request failed with status ${status}`,
    status,
    responseBody,
  });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}
