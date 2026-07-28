import type { Frame, JsonRpcInbound } from "../protocol";

const FRAME_TYPE_JSON = 1;

/// Maximum payload size (16 MB) to guard against corrupt frames.
const MAX_PAYLOAD_SIZE = 16 * 1024 * 1024;

let buffer = Buffer.alloc(0);

/// Yields complete binary frames from stdin, handling partial reads (分包) and
/// multiple frames per read (粘包). Frame: `[type: i8][length: i32 BE][payload: n]`.
async function* frameIterator(): AsyncGenerator<Frame> {
  for await (const chunk of process.stdin) {
    buffer = Buffer.concat([buffer, chunk instanceof Buffer ? chunk : Buffer.from(chunk)]);
    for (;;) {
      if (buffer.length < 5) break; // Header incomplete (分包).
      const type = buffer.readInt8(0);
      const length = buffer.readInt32BE(1);
      if (length < 0 || length > MAX_PAYLOAD_SIZE) {
        buffer = Buffer.alloc(0); // Invalid frame: reset.
        break;
      }
      if (buffer.length < 5 + length) break; // Payload incomplete (分包).
      const payload = buffer.subarray(5, 5 + length);
      yield { type, payload };
      buffer = buffer.subarray(5 + length); // Consume frame, keep remainder (粘包).
    }
  }
}

const iterator = frameIterator();

/// Reads one binary frame from stdin. Returns null on EOF.
export async function readFrame(): Promise<Frame | null> {
  const result = await iterator.next();
  if (result.done) return null;
  return result.value;
}

/// Reads a type=1 (JSON) frame and parses it as JSON-RPC. Returns null on EOF.
/// Skips non-JSON frames (future: dispatch to a registered callback).
export async function readMessage(): Promise<JsonRpcInbound | null> {
  for (;;) {
    const frame = await readFrame();
    if (frame === null) return null;
    if (frame.type !== FRAME_TYPE_JSON) continue; // Skip non-JSON (future dispatch).
    try {
      return JSON.parse(frame.payload.toString()) as JsonRpcInbound;
    } catch {
      // Skip malformed JSON payload.
    }
  }
}
