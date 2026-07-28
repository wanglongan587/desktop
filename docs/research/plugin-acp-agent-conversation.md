# 插件管理与 ACP Agent 对话能力调研

- 仓库: `D:\project\plugin-manager-20260720`（Ora monorepo，pnpm + Cargo workspace）
- 调研日期: 2026-07-22
- 目标功能: 补齐插件管理能力，使 Ora 应用可以通过 agent 插件、使用 ACP 协议与 agent 进行对话
- 调研方式: 以代码库为主、以 ACP 官方规范为辅（非二手资料）。每条结论附 `path:line` 或 URL。

---

> **⚠️ 部分结论已 superseded（2026-07-23）。** 本文档的**事实盘点仍有效**（`AgentDefinition` 无启动规格、`createUnavailableAcpClient` 是空壳、ACP 类型与 `ora-process` 原语齐备、Web/Desktop 注入差异等）。但本文第「真实 ACP 传输」「`crates/acp-runtime`」「`AgentDefinition` 加 launch 字段」等**方案性结论基于「模型 A」（host 拥有 ACP、直接 spawn agent）**，已被 ADR-0001 的**模型 B**决策取代：插件自带代码、自己持有 ACP；host spawn 的是**插件进程**而非 agent；真实 ACP-to-codex 传输移入 codex 插件（TS），host 侧 crate 为 `plugin-runtime`（管插件进程 + plugin-channel client + agent-runtime 桥接 `AcpClient`），**host 不直接说 ACP**，`AgentDefinition` 也不加 launch 字段（改由 manifest 的 process entrypoint 描述插件进程）。详见 [`CONTEXT.md`](../../CONTEXT.md) 与 [`docs/adr/0001-plugin-system-architecture.md`](../adr/0001-plugin-system-architecture.md)。

---

## 概述

Ora 是一个 monorepo，包含两套对等运行时与一套共享后端：

- **Web 运行时** (`apps/web/server`)：基于 axum 的 HTTP 服务器，仅暴露 CRUD 路由（`apps/web/server/src/routes.rs:14`）。
- **Desktop 运行时** (`apps/desktop/src-tauri`)：基于 Tauri，通过 IPC 命令暴露同一批 CRUD 操作（`apps/desktop/src-tauri/src/commands.rs:39` 起的 `backend_command!` 宏）。
- **共享后端** (`crates/backend`)：一个可 clone 的 `Backend` 结构，组合了 Project / Task / Session / Skill / Agent 五类 CRUD handler，底层是 SQLite（`crates/backend/src/bootstrap.rs:37`）。Web 与 Desktop 都复用它，区别仅在传输层。

与本次功能相关的核心事实：

1. **ACP（Agent Client Protocol）是一套外部开放标准**，Ora 在 `crates/contracts/src/acp/` 中对其数据模型做了**完整且忠实的 Rust 建模**，并通过 `ts_rs` 自动生成 TypeScript 绑定到 `packages/contracts/src/acp/`。但**没有任何 ACP 运行时/传输层**——既没有把 JSON-RPC 消息发到真实 agent 进程的客户端，也没有 agent 侧实现。
2. **聊天状态机 `packages/chat` 已经是一个完整的 ACP 客户端消费者**：它知道如何把 `session/prompt` 的返回与 `session/update` 通知装配成聊天对话流。唯一缺失的是**真实的 `AcpClient` 传输实现**——目前生产环境用的是 `createUnavailableAcpClient()`（每次调用都抛错，`packages/chat/src/client.ts:20`），Desktop 与测试用的是内存 mock（`packages/mock-service/src/acp.ts:77`）。
3. **`packages/plugin-sdk` 只是一个玩具脚手架**（`getNums`/`returnNums` 示例方法），既无插件清单、也无发现/加载/生命周期/沙箱，更未与 ACP 方法对齐。
4. 结论：本功能的缺口集中在「**agent 插件加载 + ACP stdio 传输 + 与 Ora session 的接线**」这一条线上；**契约层与 UI 状态层已就绪**，缺的是中间的运行时与插件管理面。

---

## 各子系统现状

### 1. ACP 协议契约 — `crates/contracts/src/acp/`

模块清单见 `crates/contracts/src/acp/mod.rs:1-20`，覆盖 `authentication / common / content / error / file / initialization / literals / mcp / notification / permission / plan / prompt / rpc / serde_util / session / session_config_options / session_mode / slash_command / terminal / tool_call`。这是一份相当完整的 ACP 数据模型。

- **JSON-RPC 2.0 信封**：`rpc.rs` 定义 `Request<Params>`（`rpc.rs:42`）、`Response<Result,Error>`（`rpc.rs:58`）、`Notification<Params>`（`rpc.rs:99`）、`JsonRpcMessage<M>`（强制 `"jsonrpc":"2.0"`，`rpc.rs:117`）、`JsonRpcBatch`（`rpc.rs:160`）。
- **方法名常量**（`literals.rs`）：
  - Agent 侧方法：`initialize`、`authenticate`、`session/new`、`session/load`、`session/set_mode`、`session/set_config_option`、`session/prompt`、`session/cancel`、`session/list`、`session/delete`、`session/resume`、`session/close`、`logout`、`$/cancel_request`（`literals.rs:39-83`）。
  - Client 侧方法（agent 反向调用 client）：`session/update`、`session/request_permission`、`fs/write_text_file`、`fs/read_text_file`、`terminal/create|output|release|wait_for_exit|kill`（`literals.rs:89-140`）。
- **initialize 握手**（`initialization.rs`）：`ProtocolVersion(u16)`（`initialization.rs:10`）、`InitializeRequest`（`initialization.rs:20`）、`InitializeResponse`（`initialization.rs:69`）、`ClientCapabilities`（fs/terminal/session，`initialization.rs:134`）、`AgentCapabilities`（loadSession/prompt/mcp/session/auth，`initialization.rs:315`）、`SessionCapabilities`（list/delete/additional_directories/resume/close，`initialization.rs:497`）。
- **session 生命周期**（`session.rs`）：`NewSessionRequest{cwd, additional_directories, mcp_servers}`（`session.rs:46`）、`NewSessionResponse{session_id, modes, config_options}`（`session.rs:94`）、`LoadSessionRequest`/`ResumeSessionRequest`/`CloseSessionRequest`/`ListSessionsRequest`/`DeleteSessionRequest`（`session.rs:148` 起）、`SessionInfo`（`session.rs:492`）。
- **session/update 通知**（`session.rs:557`）：`SessionUpdate` 枚举包含 `UserMessageChunk / AgentMessageChunk / AgentThoughtChunk / ToolCall / ToolCallUpdate / Plan / AvailableCommandsUpdate / CurrentModeUpdate / ConfigOptionUpdate / SessionInfoUpdate / UsageUpdate`，每种带 `ContentChunk`（`session.rs:733`）等子结构。
- **session/prompt**（`prompt.rs`）：`PromptRequest{session_id, prompt: Vec<ContentBlock>}`（`prompt.rs:16`）、`PromptResponse{stop_reason}`（`prompt.rs:52`）、`StopReason{EndTurn, MaxTokens, MaxTurnRequests, Refusal, Cancelled}`（`prompt.rs:69`）。
- **ContentBlock**（`content.rs:22`）：`Text / Image / Audio / ResourceLink / Resource`，与 MCP 内容块兼容（`content.rs:17` 注释）。
- **认证**（`authentication.rs`）：`AuthMethod::Agent(AuthMethodAgent)`（`authentication.rs:17`）——目前只建模了 agent 自处理认证这一种；`AuthenticateRequest`/`LogoutRequest`（`authentication.rs:112`/`authentication.rs:146`）。
- **错误**（`error.rs`）：`Error{code, message, data}`（`error.rs:19`）、`ErrorCode` 含标准 JSON-RPC 码与 ACP 专用码 `-32000 AuthRequired`、`-32002 ResourceNotFound`、`-32800 RequestCancelled`（`error.rs:127`）。
- **规范出处佐证**：`session.rs:142` 的文档注释直接引用 `https://agentclientprotocol.com/protocol/session-setup#loading-sessions`，证明这是 Agent Client Protocol 标准而非自研协议。
- **Rust→TS 同步**：`crates/contracts/src/lib.rs:58` 的 `export_typescript_bindings_to` 调用 `acp::export`（`crates/contracts/src/acp/mod.rs:26`），每个结构体上的 `#[ts(export_to = "acp/...")]` 注解（如 `initialization.rs:9`）决定输出路径。生成文件头部标注 `generated by ts-rs`（如 `packages/contracts/src/agent.ts:1`）。

### 2. Agent 定义模型 — `crates/application/src/agent_definition/`

- **`AgentDefinitionRepository` trait**（`ports.rs:4`）：仅 `create / find / list / update / soft_delete` 五个存储操作。
- **handler**（`handlers.rs`）：`CreateAgentDefinitionHandler` 构造 `AgentDefinition::new(id, name, description, AuditFields)`（`handlers.rs:43`）。也就是说，一个「agent」在 Ora 领域里**只是一个可配置的 agent 类型元数据**（id + name + description + 审计字段），**没有任何启动规格**（无 command/args/env/transport 字段）。
- **后端组合**（`crates/backend/src/agent.rs:15`）：`AgentApi` 把上述 handler 接到 `SqliteAgentDefinitionRepository` 上，纯 CRUD。
- **与 session 的关联**（`crates/domain/src/session.rs`）：
  - `AgentId(String)`，内置 `TERMINAL="terminal"` 与 `AgentId::terminal()`（`session.rs:11-22`）。
  - `Session{id, task_id, agent_id, agent_session_id: Option<String>, status, audit_fields}`（`session.rs:71`）。这里 `agent_id` 指向 Ora 的 agent_definition，`agent_session_id` 就是 ACP agent 返回的 session id——**这正是 Ora session 与 ACP session 的接线点**，但目前只是个被持久化的字符串，生产环境下从未由真实 ACP `session/new` 产生（仅 mock 产生）。
- **「agent」≠「agent 插件」**：Rust 领域里没有「插件」概念，agent_definition 也不是插件。两者目前是完全分离的概念（详见第 3、4 节）。

### 3. 插件系统 — `packages/plugin-sdk/`

- **包元数据**（`package.json:1`）：`@ora-space/plugin-sdk`，依赖 `bun-types`，仅导出 `./host`。
- **协议形状**（`src/host/protocol.ts:1`）：`HostRequest{id, method, params}`、`SuccessResponse{jsonrpc:"2.0", id, result}`、`ErrorResponse{jsonrpc:"2.0", id, error:{code, message}}`——形态上类 JSON-RPC，但**方法名是任意的**，未与 ACP 方法对齐。
- **入口**（`src/host/index.ts:1`）：仅导出 `getNums`（从 stdin 读一行 JSON-RPC 请求，`getNums.ts:16`）与 `returnNums`（向 stdout 写响应，`returnNums.ts:10`）。底层是 `internal/reader.ts`（stdin 逐行迭代，`reader.ts:7`）与 `internal/writer.ts`（stdout 写行，`writer.ts:6`）。
- **判定**：这是一个**玩具脚手架**，用 `getNums/returnNums` 占位来验证「host 经 stdin/stdout 与子进程交换 JSON-RPC 行」这条管道。它**没有**插件清单（manifest）、发现机制、加载器、注册表、生命周期管理、沙箱，也没有路由到 ACP 的 `initialize/session/new/session/prompt` 等方法。可以视为后续插件 host 协议的雏形，但距离「插件管理」还很远。

### 4. Session 与对话 — `crates/application/src/session/` + `packages/chat/`

- **Ora session 是纯 CRUD 记录**（`crates/application/src/session/handlers.rs:16` 的 `CreateSessionHandler`）：接收 `CreateSessionRequest{taskId, agentId, agentSessionId, status}`，存成 `Session` 行（`handlers.rs:47`）。不驱动任何对话。
- **`AcpClient` 接口**（`packages/chat/src/client.ts:12`）：`newSession / prompt / cancel / subscribe(listener)` 四个方法，是与传输无关的 ACP 客户端抽象。
- **`createUnavailableAcpClient()`**（`client.ts:20`）：每个方法都 `throw new Error("ACP transport is not configured")`。**这是 Web 生产环境注入的实现**（`apps/web/client/src/contracts-runtime.ts:7`）。
- **`createChatStore(client)`**（`packages/chat/src/store.ts:54`）：完整的 ACP 消费者——
  - `sendMessage` 构造用户消息后调用 `client.prompt({sessionId, prompt:[{type:"text",text}]})`，据 `response.stopReason` 置 `completed/cancelled`（`store.ts:103-111`）。
  - `client.subscribe` 收到 `SessionNotification` 后经 `applySessionUpdate`（`store.ts:179`）把 `agent_message_chunk / agent_thought_chunk / plan / tool_call / tool_call_update` 装配成 `ChatTurn` 里的消息/思考/计划/工具调用条目。
  - `cancelMessage` 调 `client.cancel`（`store.ts:134`）。
- **mock ACP agent**（`packages/mock-service/src/acp.ts:77` 的 `createMockAcpClient`）：一个完整的**内存 ACP agent 仿真**——实现 `newSession/prompt/cancel/subscribe`，按场景流式吐出 `agent_message_chunk`、`tool_call`、`plan` 等通知。它是真实 ACP 传输+agent 行为的参考实现。Desktop 即用它（`apps/desktop/web/App.tsx:8`）。
- **一致性测试**（`packages/chat/src/conformance.ts:22`）：`exerciseAcpClientConformance` 校验任意 `AcpClient` 实现的 session 身份、update 投递、退订、取消等基线行为，可用于将来验证真实传输。
- **判定**：**没有 Rust 侧的 agent 运行时/执行器**。全仓 grep 确认 ACP 类型（`NewSessionRequest`/`InitializeRequest`/`session_prompt` 等）仅在 `crates/contracts` 与 `packages/contracts`、`packages/chat`、`packages/mock-service` 中出现；没有任何 Rust 代码调用 `initialize` 或 `session/prompt` 去驱动一个真实 agent 进程。

### 5. 服务端 API — `apps/web/server/`

- **路由表**（`routes.rs:14` 的 `build_router`）：`/health/live|ready`、`/api/file-system/directory`、`/api/projects`、`/api/project-work-contexts/{open,renew}`、`/api/tasks`、`/api/sessions`、`/api/skills`、`/api/agents`，全部是 REST CRUD（`routes.rs:15-73`）。
- **agent handler**（`handlers/agents.rs:28`）：`create_agent/get_agent/list_agents/update_agent/delete_agent`，全部委托 `app_state.backend().*`（`handlers/agents.rs:32`）。
- **session handler**（`handlers/sessions.rs:30`）：同上，CRUD 直通后端（`handlers/sessions.rs:34`）。
- **启动**（`main.rs:17`）：`#[tokio::main]` 的 axum serve，无 WebSocket/SSE/流式端点。
- **判定**：**没有任何 ACP 端点、agent spawn 端点或流式通道**。`docs/web-server-runtime.md:62` 的 HTTP API 清单也确认仅 CRUD。

### 6. 客户端 / UI

- **运行时注入**：
  - Web：`createContractsClient(createFetchTransport())` + `createChatStore(createUnavailableAcpClient())`（`apps/web/client/src/contracts-runtime.ts:6-7`）。即 Web 生产环境一发起对话就抛错。
  - Desktop：`createContractsClient(createTauriTransport())` + `createChatStore(createMockAcpClient())`（`apps/desktop/web/App.tsx:8-9`）。Desktop 走 mock agent。
- **typed HTTP 客户端**：`packages/contracts/src/client.ts:46` 的 `createContractsClient` 据 `endpoints.ts` 清单生成 `project/task/session/skill/agent/fileSystem` 命名空间，编译期与路由锁步（`client.ts:38` 注释）。
- **传输**：`packages/contracts/src/fetch.ts:8` 的 `createFetchTransport`（Web）；Desktop 用 `createTauriTransport`（`docs/application-contracts.md:26`）。
- **对话落点**：`packages/app-shell/src/features/workspace/workspace-view.tsx:30` 的 `WorkspaceView`——选中 session 后渲染 `ChatView`（`workspace-view.tsx:105`），对话状态取自 `chatStore.conversations[selection.sessionId]`（`workspace-view.tsx:46`）。
- **session↔agent 接线**（`packages/app-shell/src/state/hooks/use-workspace-mutations.ts:138` 的 `useCreateSession`）：
  ```ts
  const agentSession = await chatStore.getState().newSession({ cwd: project.rootPath, mcpServers: [] });
  return client.session.create({ taskId, agentId, agentSessionId: agentSession.sessionId, status });
  ```
  即「先调 ACP `newSession` 拿到 agent session id，再持久化 Ora session 行」（`use-workspace-mutations.ts:155-161`）。这条链路就是未来要替换成真实 ACP 传输的地方。
- **默认 agent 硬编码**：`DEFAULT_AGENT_ID = "codex"`（`use-workspace-mutations.ts:17`）——当前隐式只有一个 agent，并未从 `useAgents()`（`use-agents.ts:6`）的 agent_definition 列表里解析启动规格。
- **状态回写**：`use-session-status-sync.ts:18` 把聊天 store 的 in-flight 信号翻译成 `session.update({status})`，是 chat 与 REST session 之间的唯一缝。

### 7. 契约边界与 Rust↔TS 同步

- **单一来源**：Rust 结构体 + `#[derive(TS)]` + `#[ts(export_to=...]` → `ts_rs` 生成 `packages/contracts/src/*.ts`（`crates/contracts/src/lib.rs:58`）。
- **再生成命令**：`cargo xtask export-contracts`（`docs/application-contracts.md:21`，`packages/contracts/src/endpoints.ts:1` 头注）。
- **生成物**：ACP DTO（`packages/contracts/src/acp/`）、CRUD DTO（`agent.ts/project.ts/session.ts/...`）、端点清单 `endpoints.ts`。
- **手写部分**：`client.ts`（typed CRUD 客户端）、`fetch.ts`（浏览器传输）、`transport.ts`（传输抽象与错误）、Desktop 的 `tauri-transport`。
- **ACP 与 CRUD 共用同一套同步机制**，因此将来给 AgentDefinition 增加启动规格字段时，TS 侧会自动跟上。

### 8. 既有设计文档 — `docs/`

- `docs/application-contracts.md`：CRUD 契约边界与前端 SDK 导出流程，**未提插件/ACP/agent 运行时**。
- `docs/web-server-runtime.md`：Web HTTP 运行时（CRUD + 健康检查 + 文件浏览）。
- `docs/desktop-runtime.md`：Tauri 运行时（25 个 CRUD 命令 + 配置/日志）。
- `docs/gitlancer-architecture.md`、`docs/database-migrations.md`、`docs/runtime-logging.md`：Git 操作库、DB 迁移、日志。
- **判定**：`docs/` 下没有任何关于插件、ACP、agent 运行时的设计文档或 ADR。本调研文件所在 `docs/research/` 是新建目录。

---

## ACP 协议说明

**ACP（Agent Client Protocol）是一套开放标准**，官网 https://agentclientprotocol.com 。它标准化了「AI 编码 agent 与其客户端/IDE」之间的通信，定位类似 LSP 之于语言智能。

- **传输**：JSON-RPC 2.0 over stdio（子进程的 stdin/stdout），与 `crates/contracts/src/acp/rpc.rs` 的 `JsonRpcMessage{jsonrpc:"2.0"}` 一致。
- **角色**：ACP 里 Ora 是 **client**（客户端），agent 插件是 **server/agent**（被 spawn 的子进程）。Ora 发请求/通知，agent 返回响应并通过 `session/update` 通知流式吐回对话进度。
- **典型消息生命周期**（结合 Ora 契约）：
  1. **`initialize`**（`initialization.rs:20`）：client 协商 `protocolVersion`、声明 `ClientCapabilities`（fs/terminal/session）；agent 回 `AgentCapabilities` + `authMethods` + `agentInfo`。
  2. **`authenticate`**（`authentication.rs:112`）：若 agent 声明了认证方法，client 选一个 methodId 认证。
  3. **`session/new`**（`session.rs:46`）：client 传 `cwd`/`additionalDirectories`/`mcpServers`，agent 回 `sessionId` + 初始 `modes`/`configOptions`。
  4. **`session/prompt`**（`prompt.rs:16`）：client 发 `Vec<ContentBlock>` 用户消息；agent 在处理期间持续发 **`session/update` 通知**（`session.rs:557` 的 `AgentMessageChunk/AgentThoughtChunk/ToolCall/Plan/...`），最后以 `PromptResponse{stopReason}` 结束本轮（`prompt.rs:52`）。
  5. **`session/cancel`**（`literals.rs:73`）：client 通知取消，agent 应返回 `stopReason: Cancelled`（`prompt.rs:87`）。
  6. 可选 **`session/load|resume|close|list|delete`**（`session.rs:148` 起，均由 `SessionCapabilities` 门控）。
  7. agent 反向请求 client：`session/request_permission`（用户授权）、`fs/read_text_file|write_text_file`、`terminal/*`（`literals.rs:111-140`）——这些是 client 侧能力，Ora 契约里已建模但**尚无实现**。
- **Ora 实现与规范的契合**：数据模型忠实于规范（方法名、`SessionUpdate` 变体、`StopReason`、错误码都与规范一致；`session.rs:142` 直接引用规范 URL）。**唯一明显的收窄**是 `AuthMethod` 仅建模了 `Agent` 变体（`authentication.rs:17`），尚未覆盖 OAuth 等其它认证类型——这是 Ora 的有意取舍，不影响 stdio agent 插件场景。
- **关键**：Ora **只实现了 client 侧契约**，没有任何代码把 `InitializeRequest`/`NewSessionRequest`/`PromptRequest` 真正序列化后写进某个子进程的 stdin，也没有任何代码读取子进程 stdout 上的 `session/update`。这条「wire」是整条链路唯一断掉的地方。

---

## 缺口分析

### (a) 已就绪

| 能力 | 位置 |
|---|---|
| ACP 全量数据模型（Rust + TS） | `crates/contracts/src/acp/`（`mod.rs:1`）、`packages/contracts/src/acp/` |
| Rust↔TS 契约同步管道 | `crates/contracts/src/lib.rs:58` + `cargo xtask export-contracts` |
| Ora `agent_definition` / `session` CRUD（SQLite） | `crates/application/src/agent_definition/`、`crates/application/src/session/`、`crates/backend/src/{agent,session}.rs` |
| 聊天状态机（完整 ACP 消费者） | `packages/chat/src/store.ts:54` |
| `AcpClient` 抽象 + 一致性测试 | `packages/chat/src/client.ts:12`、`packages/chat/src/conformance.ts:22` |
| 内存 mock ACP agent（参考实现） | `packages/mock-service/src/acp.ts:77` |
| HTTP / Tauri CRUD 传输 + typed 客户端 | `packages/contracts/src/{client,fetch}.ts`、`apps/desktop/web/tauri-transport` |
| 工作区 / 聊天 UI + session↔agent 接线 | `packages/app-shell/src/features/workspace/workspace-view.tsx`、`use-workspace-mutations.ts:155` |
| 进程 / PTY 传输原语（可复用） | `crates/process/src/lib.rs:1`（`ProcessSpec/ProcessStdio/ManagedProcess`）、`crates/pty/src/lib.rs:1`（`PtyRuntimeManager`） |

### (b) 桩 / 部分

- **`packages/plugin-sdk`**：仅有 JSON-RPC 行收发骨架与 `getNums/returnNums` 占位（`src/host/getNums.ts:16`、`returnNums.ts:10`），**未承载任何 ACP 方法路由**，无插件清单/发现/加载/生命周期/沙箱。
- **`createUnavailableAcpClient`**：显式「未配置」桩，生产环境一对话就抛错（`packages/chat/src/client.ts:20`、`apps/web/client/src/contracts-runtime.ts:7`）。
- **`AgentDefinition` 模型**：只有 `name/description`（`crates/application/src/agent_definition/handlers.rs:43`），**缺启动规格**（command、args、env、transport=stdio 等），无法据此 spawn 一个真实 agent。
- **`Session.agent_session_id`**：字段已存在并被持久化（`crates/domain/src/session.rs:76`），UI 也按它路由（`workspace-view.tsx:45`），但生产环境下它**从未由真实 ACP `session/new` 产生**——`useCreateSession` 调的 `chatStore.newSession` 在 Web 上直接抛错、在 Desktop 上走 mock。

### (c) 完全缺失

1. **真实 ACP 传输（stdio JSON-RPC 客户端）**——核心缺口。需要 spawn agent 子进程、完成 `initialize`/`authenticate` 握手、发 `session/new|prompt|cancel`、反序列化 stdout 上的 `session/update` 通知。全仓无此实现。
2. **Rust 侧 agent 运行时/执行器**：一个新 crate（建议 `crates/acp-runtime`），复用 `crates/contracts/src/acp` 的类型与 `crates/process` 的进程管理，暴露异步 `AcpClient` trait。
3. **插件清单 + 发现 + 加载 + 注册表 + 生命周期**：当前 `plugin-sdk` 与 agent 运行时未打通；需要决定「agent 插件」是走 AgentDefinition 扩展（声明式 command）还是独立 plugin-sdk 清单（外部贡献插件）。
4. **agent 插件 → ACP 运行时的绑定**：注册表按 agent_id 缓存/复用 `AcpClient` 实例（进程池或懒加载）。
5. **`initialize` / `authenticate` 流程**：契约已建模（`initialization.rs`、`authentication.rs`），运行时未实现。
6. **`session/load|list|resume|close` 接线**：契约齐全（`session.rs:148` 起），无任何调用方。
7. **暴露 ACP 给 TS 侧的通道**：要么 (a) Tauri 命令对（`newSession/prompt/cancel`）+ 事件通道投递 `session/update`，要么 (b) HTTP 流式端点（SSE/WS，如 `POST /api/sessions/{id}/prompt` + `GET /api/sessions/{id}/stream`）。当前 `routes.rs` 与 `commands.rs` 均无此类端点。
8. **client 侧 ACP 反向方法处理**：`session/request_permission`、`fs/read_text_file|write_text_file`、`terminal/*`（`literals.rs:111-140`）——agent 会反向请求，Ora 需实现这些 handler 才能支持合规 agent。
9. **AgentDefinition 启动规格字段**：否则无法据 agent 记录 spawn 子进程。
10. **UI 侧**：agent 选择器（`useAgents` 目前只列名，`use-agents.ts:6`）、session 的 resume/list/close、权限弹窗。

---

## 建议的补齐顺序

下列顺序按「先打通最小闭环、再补规格与合规性」排列。每步标注会变动的关键文件/crate。

1. **扩展 AgentDefinition 启动规格（声明式 agent）**
   - 给 `crates/domain/src/agent_definition.rs` 增加 launch 字段（command、args、env、transport: Stdio 等）。
   - 同步 `crates/contracts/src/agent.rs` 的 `CreateAgentRequest/UpdateAgentRequest/Agent`、`crates/application/src/agent_definition/{ports,handlers,mapper}.rs`、`crates/db` 的 SQLite 仓储 + 一条 migration（见 `docs/database-migrations.md`）。
   - `cargo xtask export-contracts` 自动更新 `packages/contracts/src/agent.ts`。
   - 这样「agent 记录」即可描述一个可启动的 agent 插件。

2. **新建 Rust ACP 运行时 crate `crates/acp-runtime`**
   - 复用 `crates/contracts/src/acp` 的类型与 `crates/process`（`ProcessSpec/ProcessStdio`，`crates/process/src/lib.rs:6`）spawn 子进程。
   - 实现 stdio JSON-RPC：`initialize`/`authenticate` → `session/new` → `session/prompt`（流式收 `session/update`）→ `session/cancel`。
   - 暴露异步 trait（对齐 TS 侧 `AcpClient` 四方法 + subscribe）。
   - 先用本地 mock agent 二进制（或复用 `packages/mock-service` 的逻辑）做端到端验证。

3. **agent 插件 → ACP 运行时绑定（注册表）**
   - 在 `crates/backend`（或新 crate）里加一个 `AgentRuntimeRegistry`：按 `agent_id` 懒加载/复用 `AcpClient` 实例，据 AgentDefinition 的 launch 规格 spawn。
   - 处理进程生命周期（崩溃重启、空闲回收、`session/close`）。

4. **把 Ora session 接到真实运行时**
   - 替换 `use-workspace-mutations.ts:155` 中 `chatStore.newSession` 的 mock/Unavailable 实现：让 `newSession/prompt/cancel/subscribe` 经新通道落到 Rust 运行时。
   - 保留现有「`newSession` 返回 sessionId → `client.session.create({agentSessionId})` 持久化」的接线（`use-workspace-mutations.ts:155-161`），仅替换传输后端。
   - `use-session-status-sync.ts:18` 的状态回写逻辑无需改动，天然兼容。

5. **选型并实现 TS↔Rust ACP 通道**（替换 `createUnavailableAcpClient`，`packages/chat/src/client.ts:20`）
   - 方案 A（Desktop 优先）：新增 Tauri 命令 `acp_new_session/acp_prompt/acp_cancel` + 一个 `acp_update` 事件流（仿 `apps/desktop/src-tauri/src/commands.rs` 的宏 + `tauri::Manager` 事件）。写一个 `createTauriAcpClient` 适配 `AcpClient` 接口。
   - 方案 B（Web 优先）：在 `apps/web/server/src/routes.rs` 加流式端点（SSE/WS），`apps/web/server/src/handlers/` 新增 `acp` handler；写 `createHttpAcpClient`。
   - 建议 Desktop 先行（进程 spawn 更自然），Web 走「后端代理 agent 进程 + SSE」。

6. **补全插件管理面**
   - 决策：外部贡献插件走 `packages/plugin-sdk` 清单格式，还是统一收编进 AgentDefinition 声明式。
   - 无论哪种，都需要：manifest schema、发现（扫目录）、加载、注册表、生命周期、（可选）沙箱。当前 `plugin-sdk` 的 `HostRequest` 形状（`src/host/protocol.ts:1`）可演进成 ACP 方法路由，但需重写方法集为 ACP 方法名（`literals.rs:39`）。

7. **补 client 侧 ACP 反向方法**（`session/request_permission`、`fs/*`、`terminal/*`，`literals.rs:111-140`）
   - 至少实现 `fs/read_text_file|write_text_file` 与 `session/request_permission`，否则主流 ACP agent（需读文件、需授权）无法正常工作。
   - `terminal/*` 可复用 `crates/pty`（`crates/pty/src/lib.rs:6` 的 `PtyRuntimeManager`）。

8. **UI 补齐**
   - agent 选择器接 `useAgents()`（`use-agents.ts:6`）真实列表并显示启动规格。
   - session 的 resume/list/close 入口（契约 `session.rs:148` 起）。
   - 权限弹窗（`session/request_permission`）与工具调用/计划/思考的渲染（`ChatView` 已能消费 `ChatToolCall/ChatPlan`，见 `packages/chat/src/types.ts`）。

> 最小可见闭环建议：完成第 1、2、5(方案 A)、4 步后，Desktop 即可用真实 stdio agent 插件完成一次「initialize → session/new → session/prompt → 流式 session/update → PromptResponse」的对话；Web 与 mock 仍可用作开发兜底。
