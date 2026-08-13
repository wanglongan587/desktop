#![cfg(windows)]

use ora_contracts::{
    AgentInvocationRequest, ApplicationAgentScope, INVOCATION_ID_HEADER, IdentifyPluginRequest,
    IdentifyPluginResponse, InstallPluginRequest, InstallPluginResponse, PluginActionResponse,
};
use ora_plugin_protocol::{AgentMethod, PluginId};
use ora_web_server::config::RuntimeConfig;
use ora_web_server::{BackendRuntime, PluginBackendOptions};
use pretty_assertions::{assert_eq, assert_ne};
use reqwest::StatusCode;
use reqwest::header::{ACCESS_CONTROL_ALLOW_ORIGIN, CONTENT_TYPE, ORIGIN, VARY};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Proves production authentication, browser-route omission, shutdown, and lease release together.
#[tokio::test]
#[ignore = "run after `task prepare-plugin-runtime` to use the verified local runtime cache"]
async fn loopback_authority_rotates_and_browser_mode_omits_plugin_routes() {
    let root = TempDir::new().unwrap_or_else(|error| panic!("expected test root: {error}"));
    let data_dir = root.path().join("data");
    let project_dir = root.path().join("project");
    std::fs::create_dir_all(&project_dir)
        .unwrap_or_else(|error| panic!("expected project directory: {error}"));
    let runtime_config = runtime_config(&data_dir, &project_dir);
    let resources = prepared_runtime_resources();
    let origin = "tauri://localhost";

    let first = BackendRuntime::start(
        &runtime_config,
        PluginBackendOptions::new(&resources, vec![origin.to_owned()]),
    )
    .await
    .unwrap_or_else(|error| panic!("expected first backend start: {error}"));
    assert!(first.endpoint().ip().is_loopback());
    let first_credentials = first
        .credentials()
        .unwrap_or_else(|| panic!("expected authenticated credentials"));
    let first_bearer = first_credentials.bearer().to_owned();
    let first_endpoint = first_credentials.endpoint();
    let client = reqwest::Client::new();
    let catalog = client
        .get(format!("http://{first_endpoint}/api/plugins"))
        .header(ORIGIN, origin)
        .bearer_auth(&first_bearer)
        .send()
        .await
        .unwrap_or_else(|error| panic!("expected catalog response: {error}"));
    assert_eq!(
        (
            catalog.status(),
            catalog
                .headers()
                .get(ACCESS_CONTROL_ALLOW_ORIGIN)
                .and_then(|value| value.to_str().ok()),
            catalog
                .headers()
                .get(VARY)
                .and_then(|value| value.to_str().ok()),
        ),
        (StatusCode::OK, Some(origin), Some("Origin"))
    );

    let missing_bearer = client
        .get(format!("http://{first_endpoint}/api/plugins"))
        .header(ORIGIN, origin)
        .send()
        .await
        .unwrap_or_else(|error| panic!("expected missing-bearer response: {error}"));
    assert_eq!(missing_bearer.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        missing_bearer.headers()[ACCESS_CONTROL_ALLOW_ORIGIN],
        origin
    );
    let wrong_origin = client
        .get(format!("http://{first_endpoint}/api/plugins"))
        .header(ORIGIN, "https://attacker.invalid")
        .bearer_auth(&first_bearer)
        .send()
        .await
        .unwrap_or_else(|error| panic!("expected wrong-origin response: {error}"));
    assert_eq!(
        (
            wrong_origin.status(),
            wrong_origin
                .headers()
                .contains_key(ACCESS_CONTROL_ALLOW_ORIGIN),
        ),
        (StatusCode::FORBIDDEN, false)
    );
    first
        .shutdown()
        .await
        .unwrap_or_else(|error| panic!("expected first shutdown: {error}"));

    let second = BackendRuntime::start(
        &runtime_config,
        PluginBackendOptions::new(&resources, vec![origin.to_owned()]),
    )
    .await
    .unwrap_or_else(|error| panic!("expected lease-releasing restart: {error}"));
    let second_bearer = second
        .credentials()
        .unwrap_or_else(|| panic!("expected second credentials"))
        .bearer()
        .to_owned();
    assert_ne!(second_bearer, first_bearer);
    second
        .shutdown()
        .await
        .unwrap_or_else(|error| panic!("expected second shutdown: {error}"));

    let browser = BackendRuntime::start(
        &runtime_config,
        PluginBackendOptions::new(resources, Vec::new()).without_plugin_routes(),
    )
    .await
    .unwrap_or_else(|error| panic!("expected browser-mode backend: {error}"));
    assert!(browser.credentials().is_none());
    let omitted = client
        .get(format!("http://{}/api/plugins", browser.endpoint()))
        .send()
        .await
        .unwrap_or_else(|error| panic!("expected omitted-route response: {error}"));
    assert_eq!(omitted.status(), StatusCode::NOT_FOUND);
    browser
        .shutdown()
        .await
        .unwrap_or_else(|error| panic!("expected browser shutdown: {error}"));
}

/// Proves the trusted picker capability can drive install, invoke, cancel, and removal over HTTP.
#[tokio::test]
#[ignore = "run after `task prepare-plugin-runtime` to use the verified local runtime cache"]
async fn authenticated_http_flow_uses_opaque_authority_and_real_runtime() {
    let root = TempDir::new().unwrap_or_else(|error| panic!("expected test root: {error}"));
    let data_dir = root.path().join("data");
    let project_dir = root.path().join("project");
    let source_dir = root.path().join("source");
    std::fs::create_dir_all(&project_dir)
        .unwrap_or_else(|error| panic!("expected project directory: {error}"));
    write_agent_fixture(&source_dir, "ora.http-e2e");
    let runtime_config = runtime_config(&data_dir, &project_dir);
    let origin = "tauri://localhost";
    let runtime = BackendRuntime::start(
        &runtime_config,
        PluginBackendOptions::new(prepared_runtime_resources(), vec![origin.to_owned()]),
    )
    .await
    .unwrap_or_else(|error| panic!("expected backend start: {error}"));
    let credentials = runtime
        .credentials()
        .unwrap_or_else(|| panic!("expected authenticated credentials"));
    let authority = HttpAuthority::new(
        credentials.endpoint(),
        origin,
        credentials.bearer().to_owned(),
    );

    let selection = runtime
        .register_native_selection(&source_dir)
        .unwrap_or_else(|error| panic!("expected native selection: {error}"))
        .selection
        .unwrap_or_else(|| panic!("expected selected candidate"));
    let identified: IdentifyPluginResponse = authority
        .post_json(
            "/api/plugins/identify",
            &IdentifyPluginRequest {
                selection_handle: selection.selection_handle,
            },
        )
        .await;
    assert_eq!(identified.plugin_id, plugin_id("ora.http-e2e"));
    let installed: InstallPluginResponse = authority
        .post_json(
            "/api/plugins/install",
            &InstallPluginRequest {
                candidate_handle: identified.candidate_handle,
            },
        )
        .await;
    assert_eq!(
        (installed.plugin_id.clone(), installed.enabled),
        (plugin_id("ora.http-e2e"), false)
    );
    let _: PluginActionResponse = authority
        .post_json(
            &format!("/api/plugins/{}/enable", installed.plugin_id),
            &json!({}),
        )
        .await;

    let discovery = authority
        .invoke(AgentInvocationRequest {
            plugin_id: installed.plugin_id.clone(),
            method: AgentMethod::DiscoverInstallations,
            scope: ApplicationAgentScope::Global {},
            params: json!({ "providerId": "example" }),
        })
        .await;
    assert_eq!(
        parse_ndjson(&discovery),
        vec![json!({
            "type": "completed",
            "result": {
                "installations": [],
                "diagnostics": [{
                    "kind": "notFound",
                    "message": "No installations found"
                }]
            }
        })]
    );

    let started = authority
        .invoke(AgentInvocationRequest {
            plugin_id: installed.plugin_id.clone(),
            method: AgentMethod::StartConversation,
            scope: ApplicationAgentScope::Global {},
            params: json!({
                "providerId": "example",
                "installationId": "installation",
                "clientRequestId": "00000000-0000-4000-8000-000000000001",
                "prompt": "hello"
            }),
        })
        .await;
    assert_eq!(
        parse_ndjson(&started),
        vec![
            json!({
                "type": "event",
                "event": { "kind": "conversationStarted", "conversationId": "conversation" }
            }),
            json!({
                "type": "event",
                "event": { "kind": "textDelta", "channel": "assistant", "text": "hello" }
            }),
            json!({
                "type": "completed",
                "result": {
                    "conversationId": "conversation",
                    "turnId": "turn",
                    "finishReason": "completed"
                }
            }),
        ]
    );

    let cancellation = authority
        .invoke_and_cancel(AgentInvocationRequest {
            plugin_id: installed.plugin_id.clone(),
            method: AgentMethod::SendMessage,
            scope: ApplicationAgentScope::Global {},
            params: json!({
                "providerId": "example",
                "installationId": "installation",
                "conversationId": "conversation",
                "clientRequestId": "00000000-0000-4000-8000-000000000002",
                "prompt": "cancel me"
            }),
        })
        .await;
    assert_eq!(
        parse_ndjson(&cancellation),
        vec![
            json!({
                "type": "event",
                "event": { "kind": "textDelta", "channel": "assistant", "text": "pending" }
            }),
            json!({ "type": "failed", "error": "cancelled" }),
        ]
    );

    let _: PluginActionResponse = authority
        .post_json(
            &format!("/api/plugins/{}/disable", installed.plugin_id),
            &json!({}),
        )
        .await;
    authority
        .delete_ok(&format!("/api/plugins/{}", installed.plugin_id))
        .await;
    runtime
        .shutdown()
        .await
        .unwrap_or_else(|error| panic!("expected backend shutdown: {error}"));
}

struct HttpAuthority {
    client: reqwest::Client,
    endpoint: SocketAddr,
    origin: String,
    bearer: String,
}

impl HttpAuthority {
    fn new(endpoint: SocketAddr, origin: &str, bearer: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            endpoint,
            origin: origin.to_owned(),
            bearer,
        }
    }

    /// Sends one authenticated JSON command and verifies its exact CORS projection.
    async fn post_json<Request, Response>(&self, path: &str, request: &Request) -> Response
    where
        Request: Serialize + ?Sized,
        Response: DeserializeOwned,
    {
        let response = self
            .client
            .post(self.url(path))
            .header(ORIGIN, &self.origin)
            .bearer_auth(&self.bearer)
            .json(request)
            .send()
            .await
            .unwrap_or_else(|error| panic!("expected POST {path}: {error}"));
        self.assert_success_cors(&response);
        response
            .json()
            .await
            .unwrap_or_else(|error| panic!("expected JSON from POST {path}: {error}"))
    }

    /// Collects one complete authenticated invocation stream after checking stream headers.
    async fn invoke(&self, request: AgentInvocationRequest) -> String {
        let response = self
            .client
            .post(self.url("/api/agent-invocations"))
            .header(ORIGIN, &self.origin)
            .bearer_auth(&self.bearer)
            .json(&request)
            .send()
            .await
            .unwrap_or_else(|error| panic!("expected invocation response: {error}"));
        self.assert_stream_response(&response);
        response
            .text()
            .await
            .unwrap_or_else(|error| panic!("expected invocation body: {error}"))
    }

    /// Cancels an active stream through its opaque HTTP invocation id and collects its terminal.
    async fn invoke_and_cancel(&self, request: AgentInvocationRequest) -> String {
        let mut response = self
            .client
            .post(self.url("/api/agent-invocations"))
            .header(ORIGIN, &self.origin)
            .bearer_auth(&self.bearer)
            .json(&request)
            .send()
            .await
            .unwrap_or_else(|error| panic!("expected cancellable invocation: {error}"));
        self.assert_stream_response(&response);
        let invocation_id = response
            .headers()
            .get(INVOCATION_ID_HEADER)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_else(|| panic!("expected invocation id"))
            .to_owned();
        let mut body = Vec::new();
        while !body.contains(&b'\n') {
            let chunk = response
                .chunk()
                .await
                .unwrap_or_else(|error| panic!("expected invocation chunk: {error}"))
                .unwrap_or_else(|| panic!("invocation closed before its first event"));
            body.extend_from_slice(&chunk);
        }
        let cancellation = self
            .client
            .delete(self.url(&format!("/api/agent-invocations/{invocation_id}")))
            .header(ORIGIN, &self.origin)
            .bearer_auth(&self.bearer)
            .send()
            .await
            .unwrap_or_else(|error| panic!("expected cancellation response: {error}"));
        self.assert_success_cors(&cancellation);
        let remaining = response
            .bytes()
            .await
            .unwrap_or_else(|error| panic!("expected cancellation terminal: {error}"));
        body.extend_from_slice(&remaining);
        String::from_utf8(body)
            .unwrap_or_else(|error| panic!("expected UTF-8 invocation stream: {error}"))
    }

    /// Executes an authenticated DELETE whose empty success envelope is irrelevant to the test.
    async fn delete_ok(&self, path: &str) {
        let response = self
            .client
            .delete(self.url(path))
            .header(ORIGIN, &self.origin)
            .bearer_auth(&self.bearer)
            .send()
            .await
            .unwrap_or_else(|error| panic!("expected DELETE {path}: {error}"));
        self.assert_success_cors(&response);
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.endpoint)
    }

    /// Requires the same exact Origin/Vary response on every successful application route.
    fn assert_success_cors(&self, response: &reqwest::Response) {
        assert_eq!(
            (
                response.status(),
                response
                    .headers()
                    .get(ACCESS_CONTROL_ALLOW_ORIGIN)
                    .and_then(|value| value.to_str().ok()),
                response
                    .headers()
                    .get(VARY)
                    .and_then(|value| value.to_str().ok()),
            ),
            (StatusCode::OK, Some(self.origin.as_str()), Some("Origin"))
        );
    }

    /// Verifies successful invocation responses retain both CORS and NDJSON identity.
    fn assert_stream_response(&self, response: &reqwest::Response) {
        self.assert_success_cors(response);
        assert_eq!(
            response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/x-ndjson")
        );
    }
}

/// Parses every compact stream line to compare the complete application-level sequence.
fn parse_ndjson(body: &str) -> Vec<Value> {
    body.lines()
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("expected NDJSON line `{line}`: {error}"))
        })
        .collect()
}

/// Writes a materialized structural Agent fixture without lifecycle scripts or dependencies.
fn write_agent_fixture(root: &Path, plugin_id: &str) {
    std::fs::create_dir_all(root.join("dist"))
        .unwrap_or_else(|error| panic!("expected fixture directory: {error}"));
    std::fs::write(
        root.join("dist").join("index.js"),
        r#"
const provider = {
  id: "example",
  contractVersion: 1,
  async discoverInstallations() {
    return {
      installations: [],
      diagnostics: [{ kind: "notFound", message: "No installations found" }],
    };
  },
  async getConfigurationSummary() { return { items: [] }; },
  async listSkills() { return { items: [] }; },
  async listMcpServers() { return { items: [] }; },
  async listConversations() { return { items: [] }; },
  async *startConversation() {
    yield { kind: "conversationStarted", conversationId: "conversation" };
    yield { kind: "textDelta", channel: "assistant", text: "hello" };
    return { conversationId: "conversation", turnId: "turn", finishReason: "completed" };
  },
  async *sendMessage(call) {
    yield { kind: "textDelta", channel: "assistant", text: "pending" };
    await new Promise((resolve, reject) => {
      if (call.signal.aborted) {
        reject(new Error("cancelled"));
        return;
      }
      call.signal.addEventListener("abort", () => reject(new Error("cancelled")), { once: true });
    });
    return { conversationId: "conversation", turnId: "turn", finishReason: "completed" };
  },
  async cancelConversation() { return { disposition: "accepted" }; }
};

export default {
  kind: "agent",
  pluginApi: 1,
  async activate() { return { providers: [provider] }; }
};
"#,
    )
    .unwrap_or_else(|error| panic!("expected fixture entry: {error}"));
    std::fs::write(
        root.join("package.json"),
        format!(
            r#"{{"name":"@ora/http-e2e","version":"0.1.0","type":"module","ora":{{"manifestVersion":1,"id":"{plugin_id}","displayName":"HTTP E2E","kind":"agent","main":"dist/index.js","engines":{{"ora":">=0.1.0 <0.2.0","pluginApi":1,"bun":">=1.0.0 <2.0.0"}},"contributes":{{"agents":[{{"id":"example","displayName":"Example","contractVersion":1}}]}}}}}}"#
        ),
    )
    .unwrap_or_else(|error| panic!("expected fixture manifest: {error}"));
}

/// Parses a stable plugin id used throughout the authenticated HTTP lifecycle.
fn plugin_id(value: &str) -> PluginId {
    PluginId::parse(value).unwrap_or_else(|error| panic!("expected plugin id: {error}"))
}

/// Builds runtime configuration without mutating process environment.
fn runtime_config(data_dir: &Path, project_dir: &Path) -> RuntimeConfig {
    RuntimeConfig::from_reader(|key| match key {
        "ORA_DATA_DIR" => Some(data_dir.to_string_lossy().into_owned()),
        "ORA_PROJECT_NAME" => Some("Backend E2E".to_owned()),
        "ORA_PROJECT_PATH" => Some(project_dir.to_string_lossy().into_owned()),
        "HOME" => Some(data_dir.to_string_lossy().into_owned()),
        _ => None,
    })
    .unwrap_or_else(|error| panic!("expected runtime configuration: {error}"))
}

/// Resolves the explicit prepared runtime resource root used by backend composition.
fn prepared_runtime_resources() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .unwrap_or_else(|| panic!("expected workspace root"))
        .join("runtime-assets")
        .join("prepared")
}
