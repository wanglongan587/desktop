# Ora 插件管理现状 vs ACP 兼容性 —— 一手核实底稿

> 状态：一手代码审计底稿（cited file:line）
> 日期：2026-07-21
> 范围：`crates/plugin-protocol` + `crates/plugin-manager` + `packages/plugin-runtime` + `crates/application` + `apps/web/server`，与 ACP（`crates/contracts/src/acp`）的触达关系
> 设计基线：本文件不重新推导设计，只审计**今日代码状态**。设计意图见 `2026-07-20-plugin-acp-integration-design.md`（B2 spec）与 `2026-07-21-b2-design-errata-and-amendments.md`（勘误）。
> 核实方法：`codegraph_explore` + `Read` + `Grep`，全部 file:line 一手可复现。标 ✅ 为本会话已读源码确认；标 🟦 的为转引勘误文件中 workflow agent 的 cited file:line（本会话未逐一重读，但已确认其上下文成立）。

---

## 1. TL;DR / 兼容性判定

**判定：当前 `crates/plugin-manager` + `crates/plugin-protocol` + `packages/plugin-runtime` 三层代码与 ACP 在 B2 形态下完全兼容，且 plugin-manager 需要零改动。**

证据链（全部本会话一手核实）：

- ✅ ora-plugin-protocol（app↔plugin 进程线协议）**不引用 ACP**：`Grep -i acp` 在 `crates/plugin-protocol/src` 零匹配。
- ✅ ora-plugin-manager（生命周期 + runtime actor）**不引用 ACP**：`Grep -i acp` 在 `crates/plugin-manager/src` 零匹配；`Grep "ora_contracts::acp"` 全 `crates/` 零匹配。
- ✅ plugin-runtime（TS bootstrap）**不引用 ACP**：`Grep -i acp` 在 `packages/plugin-runtime/src` 零匹配。
- ✅ plugin-sdk（TS 作者 SDK）**不引用 ACP**：`Grep -i acp` 在 `packages/plugin-sdk/src` 零匹配；`createAcpAgentProvider` 桥**不存在**（这是 B2 待建核心，不是缺陷）。
- ✅ ACP 被**完全圈禁**在 `crates/contracts/src/acp/`（21 文件，纯 DTO）+ 其 ts-rs 生成输出 `packages/contracts/src/acp/*.ts`；Rust 运行时（plugin-manager / plugin-protocol / application）**从不**引用 `ora_contracts::acp`。
- ✅ ora-plugin-protocol 的对话面 DTO（`StartConversationRequest`/`SendMessageRequest`/`CancelConversationRequest`/`AgentEvent`/`AgentTurnResult`/`StreamParams`）、lifecycle（`$/initialize`…`$/stream`）、版本轴（`wireVersion=1`/`pluginApi=1`/`contractVersion=1`）**全部已实现且 B2 下不改 ABI**。
- ✅ plugin-manager runtime actor（`hub.rs`/`supervisor.rs`/`session_actor.rs`/`transport.rs`/`invocation.rs`）+ `AgentInvocationHandle.next_event()`/`finish()` 流式 + `ora-process` Windows Job Object FFI **均已完整实现**。

**结论**：B2 的"不兼容"只可能出现在**待建的 plugin-sdk ACP 桥**（`createAcpAgentProvider`）+ 应用装配 + 前端通道这三处**尚未存在**的代码里，而不在已提交的任何一行代码中。已提交代码的"看起来不兼容"的点（`handshake.rs` 硬拒非 Agent kind、`session_actor.rs` 把 `agent.startConversation` 编码成 JSON-RPC、`AgentRequest` 枚举硬编码 8 方法）**在 B2 下是正确的 ora-plugin-protocol 表面，不是缺陷**——只有废弃这些的 B1 才会视其为缺陷（见 §6）。

---

## 2. 两层协议定位

### 2.1 ora-plugin-protocol（`crates/plugin-protocol`）—— app↔plugin 进程私有线协议

| 表面 | 实现 | file:line |
|---|---|---|
| 帧编解码 | 5 字节大端头 `[type:i8][length:i32 BE][payload]`，`FRAME_HEADER_BYTES=5`，`MAX_FRAME_BYTES=8 MiB` | `crates/plugin-protocol/src/frame.rs:2,4,69,122` |
| 帧类型 | `FrameType::Json = 1`（唯一） | `frame.rs:12-13` |
| JSON-RPC 严格 profile | `id` 必须 `h:<n>`、禁 batch、拒重复键/超深/显式 null | `crates/plugin-protocol/src/json_rpc.rs`（profile 定义）；TS 侧严格执行见 `packages/plugin-runtime/src/rpc/envelope.ts:20-47`、`packages/plugin-runtime/src/json/strict.ts` |
| lifecycle 方法常量 | `$/initialize`/`$/activate`/`$/deactivate`/`$/exit`/`$/cancelRequest`/`$/stream` | `crates/plugin-protocol/src/lifecycle.rs:11-16` |
| `WIRE_VERSION_V1` | `= 1` | `lifecycle.rs:10` |
| `InitializeParams` | `{wire_version, host_version, runtime_version, session_id, plugin, paths, declared_agents, limits}` | `lifecycle.rs:22-31`；校验 `validate()` 在 `lifecycle.rs:35`（含 `wire_version==1`/`kind==Agent`/`plugin_api==1` 守卫，`lifecycle.rs:36,44-45`） |
| `StreamParams`（`$/stream` 通知） | `{id, seq: JsonSafeU64, value: AgentEvent}` | `lifecycle.rs:298-302` |

### 2.2 ACP（`crates/contracts/src/acp/`）—— plugin 进程↔本地 agent 开放协议

| 表面 | 实现 | file:line |
|---|---|---|
| 模块集合 | 21 文件（authentication/common/content/error/file/initialization/literals/mcp/mod/notification/permission/plan/prompt/rpc/serde_util/session/session_config_options/session_mode/slash_command/terminal/tool_call） | `Glob crates/contracts/src/acp/*.rs`（21 命中） |
| `JsonRpcMessage` envelope | `pub struct JsonRpcMessage<M> { jsonrpc: JsonRpcVersion, message: M }`，**crate 内零成帧/transport 定义** | `crates/contracts/src/acp/rpc.rs:117-121` |
| ts-rs 导出 | `acp::export(config)` 聚合 21 子模块导出 | `crates/contracts/src/acp/mod.rs:26-30` |
| Rust 运行时引用 | **零**——`Grep "ora_contracts::acp"` 全 `crates/` 无匹配；ACP DTO 只被 `acp/mod.rs::export()` 用于 ts-rs 导出，无 Rust 运行时消费 | `Grep` 结果 |

### 2.3 两层关系（B2 决议）

两层**正交**：ora-plugin-protocol 管"插件进程"（② Ora 后端 ↔ ③ 插件进程）；ACP 管"agent 会话"（③ 插件进程 ↔ ④ 本地 agent CLI）。ACP 只在 ③↔④ 之间用，由**待建的 plugin-sdk ACP 工具** `createAcpAgentProvider` 在 ③ 内消费；② Ora 后端**不接触 ACP**，只见 ③ 产出的 `AgentEvent` 流。已提交代码与此一致：③ 侧（plugin-runtime/plugin-sdk）零 ACP 引用，② 侧（plugin-manager/plugin-protocol/application）零 ACP 引用。

---

## 3. 逐 crate 现状

### 3.1 `crates/plugin-protocol`（线协议，完整实现，B2 全保留）

**方法常量 + Agent 方法枚举**（`agent/method.rs`）：

- 8 个 wire 方法常量 `agent.discoverInstallations`/`getConfigurationSummary`/`listSkills`/`listMcpServers`/`listConversations`/`startConversation`/`sendMessage`/`cancelConversation` —— `method.rs:13-20`。
- `AgentMethod` 闭枚举（8 变体）—— `method.rs:25-42`。
- `AgentRequest` 闭枚举（8 变体，对应 8 方法）—— `method.rs:135`。
- `AgentResponse` 闭枚举（6 变体；`StartConversation`/`SendMessage` 不在其中，因它们经 `$/stream` 流回 + 终态 `AgentTurnResult`）—— `method.rs:205-212`。

**对话面 DTO**（`agent/dto.rs`）：

- `StartConversationRequest{provider_id, installation_id, scope, client_request_id, prompt}` —— `dto.rs:81-87`（**无 `mcp_servers` 字段**，本会话读源码确认）。
- `SendMessageRequest{…, conversation_id, …, prompt}` —— `dto.rs:93-100`。
- `CancelConversationRequest{provider_id, installation_id, conversation_id, scope}` —— `dto.rs:106-108`。
- `AgentEvent` 闭枚举 **6 变体**：`ConversationStarted`/`TextDelta{channel}`/`Status{phase,message?}`/`ToolCall`/`ToolResult`/`Usage` —— `dto.rs:357-400`。
- `AgentOutputChannel = Assistant|Reasoning|Tool` —— `dto.rs:406`（TS mirror `plugin-protocol.ts:129`）。
- `AgentTurnResult{conversation_id, turn_id?, finish_reason, usage?}` —— `dto.rs:444-461`（**无 refusal flag 字段**，本会话确认）。
- `AgentFinishReason` 闭枚举 **3 变体**：`Completed`/`Cancelled`/`Limit` —— `dto.rs:467-471`（**无 Refusal/MaxTurnRequests**，本会话确认；TS mirror `plugin-protocol.ts:135` = `"completed" | "cancelled" | "limit"`）。
- `CancelConversationResponse{disposition: Accepted|AlreadyStopped}` —— `dto.rs:477-488`。
- `AgentBusinessFailureKind` 闭枚举 **11 种**：`AgentUnavailable`/`AuthenticationRequired`/`InvalidAgentConfiguration`/`InstallationNotFound`/`ConversationNotFound`/`UnsupportedAgentCapability`/`InvalidState`/`PermissionDenied`/`CursorExpired`/`AgentProcessFailed`/`ProviderFailure` —— `method.rs:232-244`（TS mirror `plugin-protocol.ts:145` 11 种一致）。
- `AgentBusinessErrorData{kind, retryable, details?}` —— `method.rs:250-260`。

**manifest + 版本轴**（`manifest.rs`）：

- `PluginKind` 闭枚举 **2 变体**：`Agent`/`Workbench` —— `manifest.rs:102-105`（TS mirror `plugin-protocol.ts:29` = `"agent" | "workbench"`；**无 config/IM kind**，本会话确认）。
- `PLUGIN_API_VERSION_V1 = 1` —— `manifest.rs:12`。
- `AGENT_CONTRACT_VERSION_V1 = 1` —— `manifest.rs:13`（manifest 校验在 `manifest.rs:279,287` 强制 `plugin_api==1`、`contract_version==1`）。
- `MAX_FRAME_BYTES = 8 MiB` —— `frame.rs:4`；`MAX_AGENT_PROMPT_BYTES = 1 MiB` —— `agent/leaf.rs:14`。

**ACP 触达**：✅ `Grep -i acp` 在 `crates/plugin-protocol/src` **0 匹配**。ora-plugin-protocol 与 ACP 完全无关。

### 3.2 `crates/plugin-manager`（生命周期 + runtime，完整实现，B2 全保留）

**facade traits**：

- `PluginManagement`（管理面：scan/identify/install/enable/disable/uninstall/launch-grant/crash-loop）—— `crates/plugin-manager/src/service.rs:51`。
- `PluginRuntimeControl`（open/close admission、stop_and_reap、reset_crash_loop）—— `ports.rs:91`。
- `PluginRuntimeInvocation`（start/stop/invoke，design-v3 §15.1 的 `AgentPluginRuntime`，源码已 rename）—— `ports.rs:145`（`invoke` 在 `ports.rs:154`）。

**runtime actor 层**（`runtime/`，5 文件已完整实现）：

- `runtime/hub.rs`（PluginRuntimeHub，Job A owner，lifecycle + 对话 dispatch + `$/stream` 收敛）—— 勘误引 `hub.rs:132,217,272`（🟦）。
- `runtime/supervisor.rs`（crash-loop 窗口）—— 勘误引 `supervisor.rs:19,42`（🟦）。
- `runtime/session_actor.rs`（ora-plugin-protocol host 侧：lifecycle round-trip + 对话请求派发 + `$/stream`→`AgentInvocationHandle`）—— 本会话确认 `session_actor.rs:774-783` 的 correlation 逻辑（`(StartConversation, ConversationStarted)` 绑定 conversationId；`(SendMessage, ConversationStarted)` → `correlation_violation=true`）。
- `runtime/transport.rs`（5 字节帧 reader/writer）—— 勘误引 `transport.rs:7,187,313`（🟦）。
- `runtime/invocation.rs`：✅ `AgentInvocationHandle` 结构体 —— `invocation.rs:21-26`（`events: mpsc::Receiver<AgentEvent>`、`completion: oneshot::Receiver<Result<AgentInvocationResult,PluginError>>`）；✅ `next_event()` —— `invocation.rs:75-77`（`self.events.recv().await`）；✅ `finish()` —— `invocation.rs:90-96`；`AgentInvocationResult{Response(AgentResponse), Turn(AgentTurnResult)}` —— `invocation.rs:14-18`；`parse_agent_success` 对 `StartConversation|SendMessage` → `AgentInvocationResult::Turn` —— `invocation.rs:138-140`。**流式已实现，本会话读源码确认。**

**handshake**（`runtime/handshake.rs`，ora-plugin-protocol host 侧 lifecycle）：

- ✅ kind 守卫：`if descriptor.kind != PluginKind::Agent { return Err(PluginError::HandshakeFailed { … reason: HandshakeFailure::IdentityMismatch }); }` —— `handshake.rs:39-44`（**硬拒非 Agent kind，返回 `IdentityMismatch`**，本会话确认）。
- `$/initialize` round-trip（`METHOD_INITIALIZE`）—— `handshake.rs:100-112`；`$/activate`（`METHOD_ACTIVATE`）—— `handshake.rs:136-148`；`activate_result.validate_declared_providers` —— `handshake.rs:157-162`。
- **handshake 硬编码 ora-plugin-protocol 方法名（`$/initialize`/`$/activate`）是 CORRECT 的**——这是 ②↔③ 的 ora-plugin-protocol 表面，B2 保留（见 §6）。

**install/ + 数据模型层**：`install/{pipeline,reconcile,digest,receipt}.rs`、`catalog.rs`/`registry.rs`/`state.rs`/`enablement.rs`/`grant.rs`/`limits.rs`/`scanner.rs`/`validation.rs`/`persistence.rs`/`lease.rs` 均存在（`Glob` 确认）。

**插件状态机**（candidate→installed-disabled→enabled→running）：管理面 `PluginManagement`（`service.rs:51`）持 scan/identify/install/enable/disable；runtime 层 `PluginRuntimeControl`（`ports.rs:91`）管 admission gate + `stop_and_reap`；`enable`→`open_admission`→懒启动（`$/initialize`+`$/activate`）→`running`。

**谁 spawn 插件进程**：② Ora 后端经 `ora-process`（Windows Job Object）spawn ③ 插件进程；`TokioProcessSpawner` 用 `EnvironmentPolicy::ClearAndAllowlist`（`crates/process/src/tokio_process.rs:37`：`command.env_clear()`）。

**ACP 触达**：✅ `Grep -i acp` 在 `crates/plugin-manager/src` **0 匹配**。plugin-manager 与 ACP 完全无关——B2 下对话翻译在 ③ plugin-sdk 工具内，非 plugin-manager。

### 3.3 `packages/plugin-runtime`（TS bootstrap，完整实现，B2 全保留）

**bootstrap dispatch**（`bootstrap/`）：

- `runBootstrap()` 对 stdio 跑 private bootstrap —— `packages/plugin-runtime/src/bootstrap/main.ts:4-10`。
- `BootstrapSession`（`bootstrap/session.ts:80`）持 `#readLoop`（`session.ts:122`）→ `parseInboundEnvelope`（`rpc/envelope.ts:20`）→ `#acceptRequest`（`session.ts:142`）。
- lifecycle dispatch：首帧必须 `$/initialize`（`session.ts:145-150`）；`$/activate`（`session.ts:154-157`）；`$/deactivate`（`session.ts:158-161`）；`$/exit`/`$/cancelRequest` notification（`session.ts:172-195`）。
- 对话面 dispatch：`#acceptAgentRequest`（`session.ts:282`）→ `validateAgentRequest` → `provider.handlers[method](call, params)`（`session.ts:407`）→ 对 `startConversation`/`sendMessage` 走 `#driveGenerator`（`session.ts:413,537`）；`cancelConversation` 走 `#runCancelConversation`（`session.ts:465`）。
- `#driveGenerator`（`session.ts:537-588`）：迭代作者 `AsyncGenerator<AgentEvent, AgentTurnResult>`，每个 `AgentEvent` 经 `encodeStream(id, sequence, event)` 发 `$/stream` notification（`session.ts:586`），终态 `AgentTurnResult` 经 `encodeSuccess` 发回（`session.ts:566`）。
- `encodeStream` dispatch —— `packages/plugin-runtime/src/rpc/envelope.ts:66-68`：`encodeJson({ jsonrpc: "2.0", method: "$/stream", params: { id, seq, value } })`。**`$/stream` framing 已实现，本会话确认。**

**transport**：`transport/frame.ts`（FrameDecoder，TS 镜像 Rust `frame.rs`）、`transport/writer.ts`（ProtocolWriter，帧写出 + lane 背压）。

**generated**：`generated/plugin-protocol.ts`（ts-rs 输出，149+ 行；`AgentEvent`/`AgentTurnResult`/`AgentFinishReason`/`AgentBusinessFailureKind` 等镜像 Rust）。

**ACP 触达**：✅ `Grep -i acp` 在 `packages/plugin-runtime/src` **0 匹配**。plugin-runtime 不含任何 ACP 桥代码——`createAcpAgentProvider` 桥**尚未存在**（B2 待建核心，见 §5）。

### 3.4 `crates/application` + `apps/web/server`（应用装配，**未接线**）

**`crates/application/Cargo.toml`**：✅ 依赖为 `gitlancer`/`ora-contracts`/`ora-domain`/`ora-logging`/`thiserror`/`tokio`/`tracing`/`uuid`（`Cargo.toml:13-21`），**无 `ora-plugin-manager` 依赖**（本会话确认）。这是 B2 的应用装配缺口之一。

**`crates/application/src/`**：✅ 模块为 `project/`/`project_work_context/`/`session/`/`task/`/`agent_definition/`/`skill/`/`worktree/`/`error.rs`/`lib.rs`（`Glob` 确认），**无 `plugin/` 模块**，无 `PluginApi`，无 `AgentLaunchResolver`，无 `BackendRuntime` 装配 `PluginRuntimeHub`。`PluginRuntimeHub`/`PluginManagementService` 仅在 `crates/plugin-manager/tests/{plugin_library_e2e,runtime_windows_e2e}.rs` 构造（🟦 勘误引）。

**`apps/web/server/src/routes.rs`**：✅ `build_router`（`routes.rs:14`）注册的路由为 `/health/*`/`/api/file-system/directory`/`/api/projects*`/`/api/project-work-contexts/*`/`/api/tasks*`/`/api/sessions*`/`/api/skills*`/`/api/agents*`（`routes.rs:16-73`）。**无 `/api/plugins*`，无 `/api/agent-invocations`，无 SSE/WebSocket/NDJSON 流式端点**（本会话读源码确认）。design-v3 §15.3 的对话面 NDJSON 端点未实现。

**`apps/web/server/src/app_state.rs`**：✅ `AppState`（`app_state.rs:9-18`）字段为 `agent_api`/`file_system_api`/`project_api`/`project_work_context_api`/`task_api`/`session_api`/`skill_api`/`ready`。**无 `plugin_api` 字段**（本会话确认）。

> 注：`AppState.agent_api`（`app_state.rs:10,44`）是 ora 自己的"可配置 agent 类型"（`crates/application/src/agent_definition/`），与插件 agent provider 是两套——这是勘误 §13 risk 7 的"未决项"，不影响 ACP 兼容性判定。

---

## 4. ACP 触达面核实

下表为本会话 `Grep` 一手结果，确认 ACP 被**完全圈禁**在 `crates/contracts/src/acp/` + ts-rs 输出，Rust 运行时从不引用 `ora_contracts::acp`：

| 范围 | 命令 | 结果 |
|---|---|---|
| `packages/plugin-runtime/src` | `Grep -i acp` | ✅ **0 匹配** |
| `packages/plugin-sdk/src` | `Grep -i acp` | ✅ **0 匹配** |
| `crates/plugin-manager/src` | `Grep -i acp` | ✅ **0 匹配** |
| `crates/plugin-protocol/src` | `Grep -i acp` | ✅ **0 匹配** |
| 全 `crates/` | `Grep "ora_contracts::acp"` | ✅ **0 匹配** |

ACP 的落点：
- DTO 定义：`crates/contracts/src/acp/`（21 文件，`Glob` 确认）。
- `JsonRpcMessage` envelope：`crates/contracts/src/acp/rpc.rs:117-121`——**crate 内零成帧/transport 定义**（本会话读源码确认），成帧是消费方（待建 plugin-sdk 桥）的事。
- ts-rs 导出：`crates/contracts/src/acp/mod.rs:26-30` `export()` → `packages/contracts/src/acp/*.ts`（ts-rs 生成）。
- Rust 运行时消费：**无**。

**结论**：B2 下 ACP 由 plugin-sdk（TS）侧在 ③ 插件进程内消费，Rust 后端（②）不直接接触 ACP——这与今日代码状态**完全一致**。已提交代码无需为"隔离 ACP"做任何改动。

---

## 5. 已实现 vs 待建

| 项 | 状态 | 证据 file:line |
|---|---|---|
| ora-plugin-protocol 帧编解码（5B BE） | ✅ DONE | `frame.rs:2,4,69,122` |
| ora-plugin-protocol JSON-RPC 严格 profile | ✅ DONE | `json_rpc.rs`；TS `envelope.ts:20-47` |
| ora-plugin-protocol lifecycle（`$/initialize`…`$/stream`） | ✅ DONE | `lifecycle.rs:10-16,22-31,298-302` |
| ora-plugin-protocol 对话面 DTO（8 方法 + `AgentEvent`/`AgentTurnResult`/`AgentFinishReason`） | ✅ DONE | `dto.rs:81,93,106,357,444,467`；`method.rs:13-20,25,135,205` |
| ora-plugin-protocol 业务错误（`AgentBusinessFailureKind` 11 种 + `AgentBusinessErrorData`） | ✅ DONE | `method.rs:232-260` |
| ora-plugin-protocol 版本轴（`wireVersion=1`/`pluginApi=1`/`contractVersion=1`） | ✅ DONE | `lifecycle.rs:10`；`manifest.rs:12-13` |
| ora-plugin-protocol manifest（`PluginKind={Agent,Workbench}`） | ✅ DONE | `manifest.rs:102-105` |
| plugin-manager 管理面 facade（`PluginManagement`） | ✅ DONE | `service.rs:51` |
| plugin-manager runtime facade（`PluginRuntimeControl`/`PluginRuntimeInvocation`） | ✅ DONE | `ports.rs:91,145` |
| plugin-manager runtime actor（hub/supervisor/session_actor/transport/invocation） | ✅ DONE | `runtime/*.rs`；`invocation.rs:21,75,90`；`session_actor.rs:774-783` |
| plugin-manager `AgentInvocationHandle.next_event()`/`finish()` 流式 | ✅ DONE | `invocation.rs:21,75,90` |
| plugin-manager handshake（`$/initialize`+`$/activate` + kind 守卫） | ✅ DONE | `handshake.rs:39-44,100-162` |
| ora-process Windows Job Object FFI | ✅ DONE | `crates/process/src/windows_tree.rs`（见下） |
| plugin-runtime bootstrap dispatch（lifecycle + 对话面 → 作者 AsyncGenerator → `$/stream`） | ✅ DONE | `bootstrap/main.ts:4`；`bootstrap/session.ts:142,282,407,537,586`；`rpc/envelope.ts:66-68` |
| plugin-runtime transport（FrameDecoder/ProtocolWriter） | ✅ DONE | `transport/frame.ts`；`transport/writer.ts` |
| plugin-sdk `defineAgentPlugin` ABI + `AgentProvider` 接口 | ✅ DONE | `packages/plugin-sdk/src/agent/index.ts:88-120,123` |
| plugin-sdk `AuthorBusinessFailureKind`（排除 `providerFailure`，作者不可用） | ✅ DONE | `packages/plugin-sdk/src/agent/index.ts:49-52` |
| ora-contracts ACP DTO（21 文件 + ts-rs 导出） | ✅ DONE | `crates/contracts/src/acp/`；`acp/mod.rs:26-30` |
| **plugin-sdk ACP 桥 `createAcpAgentProvider`** | ❌ **MISSING**（B2 待建核心） | `packages/plugin-sdk/src/acp/` 不存在；`Grep acp` 在 plugin-sdk 0 匹配 |
| **`crates/application` 依赖 `ora_plugin_manager`** | ❌ **MISSING** | `Cargo.toml:13-21` 无该依赖 |
| **`crates/application/src/plugin/` 模块 + `PluginApi` + `AgentLaunchResolver`** | ❌ **MISSING** | `Glob` 确认无 `plugin/` 模块 |
| **`apps/web/server` `/api/plugins*` + `/api/agent-invocations` NDJSON 路由** | ❌ **MISSING** | `routes.rs:14-73` 无此路由 |
| **`AppState.plugin_api`** | ❌ **MISSING** | `app_state.rs:9-18` 无此字段 |
| **前端↔后端流式通道（SSE/NDJSON/Tauri IPC）** | ❌ **MISSING** | `routes.rs` 无流式端点 |
| **UI 选择→enable+invoke 调用链** | ❌ **MISSING** | 前端会话创建 mutation 仍走旧 mock 路径（🟦 勘误引 `packages/app-shell/src/state/hooks/use-workspace-mutations.ts:143`） |
| **chat store 消费完整 `AgentEvent` 6 变体** | ❌ **MISSING**（部分） | 🟦 勘误引 `packages/chat/src/store.ts:139`（只消费 `agent_message_chunk && text`） |
| **`AcpClient` 改造为 `AgentEvent` 流** | ❌ **MISSING** | 🟦 勘误引 `packages/chat/src/client.ts:12,19`（只有 mock/unavailable） |

**ora-process Windows Job Object FFI 细节**（✅ 本会话 `Grep` 确认）：
- `CreateProcessW` —— `windows_tree.rs:373`（带 `PROC_THREAD_ATTRIBUTE_JOB_LIST` 在 `:491`）。
- `CreateJobObjectW` —— `windows_tree.rs:539`。
- `TerminateJobObject` —— `windows_tree.rs:187`。
- IOCP（`CreateIoCompletionPort`/`GetQueuedCompletionStatus`/`PostQueuedCompletionStatus`）—— `windows_tree.rs:24`（imports）+ `:261`（post）。
- `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` —— `windows_tree.rs:543`。
- `PROC_THREAD_ATTRIBUTE_JOB_LIST` —— `windows_tree.rs:37,491`。
- **无 `BREAKAWAY_OK`**（`Grep` 未命中）——agent 子进程不 breakaway，Job A 覆盖 ③+④ 整树，符合 B2 单 Job 设计。

### 5.1 待建 plugin-sdk ACP 桥的 13 个缺口（引勘误）

`createAcpAgentProvider` 桥**完全缺失**，B2 spec 所有对话正确性 claim "内部自洽但运行未证"（勘误 §13）。桥需补的 13 gap（1 blocker + 6 major + 6 minor，**全在待建桥那一层，不在 plugin-manager**）：

| # | 级别 | 缺口 | 勘误条目 |
|---|---|---|---|
| 1 | ✅ 已解 | ACP stdio 成帧 = **newline-delimited**（spec `transports.mdx` 确认，非 Content-Length）—— errata §1 已解 | errata §1 |
| 2 | Major | Refusal/MaxTurnRequests 处理自相矛盾 → 修订为 `Status{phase:refusal}+Completed`，删 "flag" | errata §2 |
| 3 | Major | 非 agent 扩展"零改动"是假的 → handshake.rs kind 守卫需放开（**v1 不做，如实承认**） | errata §3 |
| 4 | Major | ④ crash/退出无 ③ 内检测 → 桥契约补 ④ 健康检测 | errata §4 |
| 5 | Major | ③↔④ stdio 背压死锁未验证 → 桥侧无界缓冲 + drop-oldest + 主动 cancel | errata §5 |
| 6 | Major | 重启后 stale session_id 无错误映射 → v1 重载禁用 send | errata §6 |
| 7 | Major | late/out-of-band session/update 无关联策略 → 桥"丢弃"（绝不 buffer-replay） | errata §7 |
| 8 | Minor | `UserMessageChunk`/`SessionInfoUpdate` 未定义 v1 处理 → drop+计数 | errata §8 |
| 9 | Minor | `mcp_servers` 无 per-conversation 来源 → v1 传空 | errata §9 |
| 10 | Minor | `disable`/`uninstall` 在 ④ 活跃 turn 期间 teardown 排序未定义 → 先 cancel 再 deactivate | errata §10 |
| 11 | Minor | `session/new` 失败路径未定义 → 桥 throw（不 yield ConversationStarted） | errata §11 |
| 12 | Minor | 多 startConversation agent 复用 vs 泄漏未定义 → 复用 ④ + 再 session/new | errata §12 |
| 13 | Minor | 桥代码完全缺失 → 所有对话 claim 未运行验证 | errata §13 |

> 这 13 gap **全部**指向待建的 `packages/plugin-sdk/src/acp/`，**无一**要求改动 `crates/plugin-manager` 或 `crates/plugin-protocol`——这是 B2 兼容性判定的直接证据。

---

## 6. "不兼容"疑点的逐一澄清

下列代码点"看起来"与 ACP 不兼容，但在 B2 下**是正确的 ora-plugin-protocol 表面，不是缺陷**。只有废弃这些的 B1 才会视其为缺陷。

| 疑点 | 代码 | 为何不是缺陷（B2 下） |
|---|---|---|
| `session_actor.rs` 把 `agent.startConversation`/`sendMessage`/`cancelConversation` 编码成 JSON-RPC request | 🟦 `session_actor.rs`（dispatch）；DTO `dto.rs:81,93,106`；方法常量 `method.rs:18-20` | 这是 ②↔③ 的 **ora-plugin-protocol** 表面，B2 保留对话面骨架。ACP 的 `session/new`/`session/prompt`/`session/cancel` 在 ③↔④ 由待建桥翻译，② 不可见。B2 §6.3 明确"ora-plugin-protocol 全保留，不改 ABI"。 |
| `handshake.rs` 硬编码 `$/initialize`/`$/activate` 方法名 | ✅ `handshake.rs:100-112,136-148`；方法常量 `lifecycle.rs:11-12` | 这是 ②↔③ 的 lifecycle（管"插件进程"），与 ACP `initialize`（管"agent 会话"，在首次 `startConversation` 时由桥在 ③ 内发起）是**两层独立**——不同协议、不同管道、不同时机。勘误 §0 已确认"双 initialize 不冲突"。 |
| `AgentRequest` 闭枚举硬编码 8 方法（不开放扩展） | ✅ `method.rs:13-20,25,135` | B2 下 ora `contractVersion=1` 冻结 v1 对话面。扩展需升级 `contractVersion`（1→2）+ DTO/golden/interop + fail-closed 证明（B2 §10.2、§12.2），而非开放枚举。ACP 侧版本由桥与 agent `initialize` 协商，独立于 ora 版本轴。 |
| `handshake.rs` kind 守卫硬拒非 Agent kind | ✅ `handshake.rs:39-44`（`IdentityMismatch`） | B2 v1 **只做 agent 类插件**（spec §1.1、§2.2）。config/IM kind 不存在（`manifest.rs:102-105` 只有 Agent/Workbench），未来加需改 handshake/validation/lifecycle/manifest + 新 method 注册表 + 新 executor face（勘误 §3 给出 6 步实清单），但**这不影响 ACP 兼容性**——非 agent kind 本就不接触 ACP（② 只见 ora-plugin-protocol）。 |
| `AgentFinishReason` 只有 3 变体（无 Refusal/MaxTurnRequests） | ✅ `dto.rs:467-471` | B2 v1 保守范围（spec §10）：Refusal/MaxTurnRequests 后置 v2。v1 降级规则在桥内（勘误 §2 修订为 `Status{phase:refusal}+Completed`，删 "flag"，因 `AgentTurnResult` 无 refusal flag 字段——✅ `dto.rs:444-461` 确认）。升级需 `contractVersion` 1→2。 |
| `StartConversationRequest` 无 `mcp_servers` 字段 | ✅ `dto.rs:81-87` | v1 桥传空 `mcp_servers`（勘误 §9）；v2 经 `contractVersion` 1→2 扩展字段。不影响兼容性——`mcp_servers` 是 ③↔④ 的 ACP `session/new` 参数，② 不感知。 |

**B1 vs B2 的关键差别**：B1（ora 宿主直接 spawn agent + 持 ACP stdio）会**废弃** `dto.rs:81,93,106` 的 `StartConversation`/`SendMessage`/`CancelConversation`、`lifecycle.rs:298` 的 `StreamParams`、`packages/plugin-runtime/src/rpc/envelope.ts:66` 的 `encodeStream`、`packages/plugin-sdk/src/agent/index.ts:108-115` 的 `AsyncGenerator<AgentEvent,AgentTurnResult>` ABI——即废弃整个已实现骨架。B2 **保留**它们，把 ACP 翻译封进 ③ 内的待建桥。今日代码状态强烈倾向 B2（spec §4.2 的 10 条 file:line 证据），故这些"硬编码"是**特性**而非缺陷。

---

## 7. ACP-spec-dependent 未决项（research #1 已于 2026-07-21 回填收口）

以下项依赖 ACP spec 上游行为。research agent #1（`docs/superpowers/specs/2026-07-21-acp-transport-and-real-agent-behavior-research.md`）已于 2026-07-21 完成，**全部收口**（本底稿初稿写作时该文件尚未落盘，现已存在）。结论见 errata §1（framing 解）/§14（ClientCapabilities）/§15（adapter 启动）/§16（session/close/load/baseline/StopReason 确认）。下表为收口后状态：

| 未决项 | 收口结论 | errata 条目 |
|---|---|---|
| ACP stdio 成帧 | spec 确认 = **newline-delimited**（`docs/protocol/v1/transports.mdx`），非 Content-Length；Ora 代码仍零成帧（在待建桥） | errata §1 ✅ 已解 |
| `ClientCapabilities` advertisement + fs/terminal handler | Ora 作为 IDE **应广告** `fs.read/write_text_file=true` + `terminal=true`（否则 coding agent 降级）；桥须实现 fs/terminal handler | errata §14 ✅ |
| `session/load` 历史回放 | spec **已规定** replay（`session/update` 在 response 前回放，response body=`null`；Ora `LoadSessionResponse` 是 spec-compatible 超集）；v1 后置是简化非 spec 缺失；v2 移除 load→resume+`replayFrom` | errata §16 ✅ |
| `session/close` 确认语义 | spec **无 `session/ended`**；唯一终态=`session/close`（opt-in `sessionCapabilities.close`）+ baseline `session/cancel`；v1 不做 close（仅标记） | errata §16 ✅ |

> 关键：Ora 代码**今日**对这些未决项的"假设"是**零代码**——没有任何一行已提交代码实现了 ACP 成帧/Capabilities/load/close。所有选择都推迟到待建桥。因此这些未决项**不构成已提交代码的兼容性风险**，只构成待建桥的设计输入。

---

## Sources

### 本地代码文件（本会话一手读源码 / Grep）

- `crates/plugin-protocol/src/frame.rs:2,4,12-13,69,122`（帧编解码）
- `crates/plugin-protocol/src/json_rpc.rs`（JSON-RPC 严格 profile）
- `crates/plugin-protocol/src/lifecycle.rs:10-16,22-31,35,36,44-45,298-302`（lifecycle + StreamParams + 版本轴）
- `crates/plugin-protocol/src/manifest.rs:12-13,102-105,279,287`（PluginKind + 版本常量）
- `crates/plugin-protocol/src/agent/dto.rs:81-87,93-100,106-108,357-400,406,444-461,467-471,477-488`（对话面 DTO + AgentEvent + AgentTurnResult + AgentFinishReason）
- `crates/plugin-protocol/src/agent/method.rs:13-20,25-42,135,205-212,232-244,250-260`（方法常量 + AgentMethod/AgentRequest/AgentResponse/AgentBusinessFailureKind/AgentBusinessErrorData）
- `crates/plugin-protocol/src/agent/leaf.rs:14`（MAX_AGENT_PROMPT_BYTES）
- `crates/plugin-manager/src/service.rs:51`（PluginManagement facade）
- `crates/plugin-manager/src/ports.rs:91,145,154`（PluginRuntimeControl / PluginRuntimeInvocation）
- `crates/plugin-manager/src/runtime/handshake.rs:39-44,100-112,136-162`（kind 守卫 + lifecycle round-trip）
- `crates/plugin-manager/src/runtime/invocation.rs:14-18,21-26,75-77,90-96,138-140`（AgentInvocationHandle + next_event/finish）
- `crates/plugin-manager/src/runtime/session_actor.rs:774-783`（stream correlation）
- `crates/process/src/windows_tree.rs:24,37,187,261,373,491,539,543`（Windows Job Object FFI）
- `crates/process/src/tokio_process.rs:37`（env_clear）
- `crates/application/Cargo.toml:13-21`（无 ora-plugin-manager 依赖）
- `apps/web/server/src/routes.rs:14-73`（路由表，无 /api/plugins* / /api/agent-invocations）
- `apps/web/server/src/app_state.rs:9-18`（AppState 无 plugin_api）
- `packages/plugin-runtime/src/bootstrap/main.ts:4-10`（runBootstrap）
- `packages/plugin-runtime/src/bootstrap/session.ts:122,142,154,158,172,282,407,413,465,537,566,586`（bootstrap dispatch + driveGenerator）
- `packages/plugin-runtime/src/rpc/envelope.ts:20-47,66-68`（parseInboundEnvelope + encodeStream）
- `packages/plugin-runtime/src/generated/plugin-protocol.ts:29,89,91,93,127,133,135,141,145,147,149,173,175`（ts-rs 镜像）
- `packages/plugin-sdk/src/types/plugin-protocol.ts:29,67,89,127,135,145`（ts-rs 镜像）
- `packages/plugin-sdk/src/agent/index.ts:49-52,88-120,123`（AgentProvider ABI + AuthorBusinessFailureKind 排除 providerFailure）
- `crates/contracts/src/acp/mod.rs:1-30`（21 子模块 + export()）
- `crates/contracts/src/acp/rpc.rs:117-121`（JsonRpcMessage，零成帧）

### Grep 结果（本会话执行）

- `Grep -i acp` in `packages/plugin-runtime/src` → 0 匹配
- `Grep -i acp` in `packages/plugin-sdk/src` → 0 匹配
- `Grep -i acp` in `crates/plugin-manager/src` → 0 匹配
- `Grep -i acp` in `crates/plugin-protocol/src` → 0 匹配
- `Grep "ora_contracts::acp"` in `crates/` → 0 匹配
- `Glob crates/contracts/src/acp/*.rs` → 21 文件
- `Glob crates/application/src/**/*.rs` → 无 `plugin/` 模块
- `Glob docs/superpowers/specs/2026-07-21*.md` → 勘误文件 + 两份 research 文件（#1 #2 均已于 2026-07-21 落盘）

### 设计文档（已存在于 repo）

- `docs/superpowers/specs/2026-07-20-plugin-acp-integration-design.md`（B2 spec，652 行）
- `docs/superpowers/specs/2026-07-21-b2-design-errata-and-amendments.md`（勘误：1 blocker + 6 major + 6 minor gap，全在待建桥）

### 交叉引用研究文件（均已落盘 2026-07-21）

- `docs/superpowers/specs/2026-07-21-acp-transport-and-real-agent-behavior-research.md`（research #1，ACP spec + 真实 agent 行为）—— 已回填：framing=newline-delimited、ClientCapabilities 应广告 fs/terminal、agent=adapter 二进制（`claude-agent-acp`/`codex-acp`，非 bare CLI）、session/load replay spec 已规定、无 session/ended。本底稿 §7 据其收口（见 errata §1/§14/§15/§16）。
- `docs/superpowers/specs/2026-07-21-ora-plugin-current-state-vs-acp-compatibility.md`（research #2，本文件）。
