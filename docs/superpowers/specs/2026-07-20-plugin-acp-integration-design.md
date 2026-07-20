# Ora 插件管理与 ACP 协议结合设计方案

> 状态：设计规格（待实现）
> 日期：2026-07-21（从 B1 形态修订为 B2 形态）
> 范围：让 ora 的插件管理机制与 ACP（Agent Client Protocol）正确结合，保证 agent 插件对话可正确进行。仅实现 agent 类插件。
> 平台：Windows
> 设计基线：`E:\claude_code_project\desktop`（分支 `self-plugin-manager-with-main`）
> ACP 协议基线：`crates/contracts/src/acp/`（21 文件，Zed ACP JSON-RPC DTO 全集）
> 插件管理设计基线：`origin/codex/plugin-management-backend-v3:docs/plugin-management/design-v3.md`（2627 行）

## 0. 关于参考路径

用户在目标中提到"具体应用进程和插件进程如何对话，请参考 `D:\project\desktop` 项目代码"。该路径在本机不存在。经核实，当前工作目录 `E:\claude_code_project\desktop` 既包含 ACP 协议实现（`crates/contracts/src/acp/`），又包含用户已实现的插件管理（`crates/plugin-protocol` + `crates/plugin-manager` + `packages/plugin-runtime` + `packages/plugin-sdk`）以及 ACP chat 前端（`packages/chat`）。因此本设计以 `E:\claude_code_project\desktop` 为唯一事实基线。若用户另有一份参考实现，请在审查时指出路径，再据以修订。

---

## 1. 背景与现状

### 1.1 ora 的产品形态

- ora 是 AI Agent IDE，运行在 Windows。
- 前端为 web 页面（`packages/app-shell` + `packages/chat`），桌面壳为 Tauri（`apps/desktop`），开发后端为 `apps/web/server`。
- 后端 Rust，crate 前缀 `ora-`。
- 插件用 TypeScript 开发，Bun 运行，每个插件一个独立进程。
- ora 通过插件集成不同 agent（Claude Code、codex、opencode 等）。安装对应插件后，可识别本地安装的 agent、agent 配置、agent 相关 skills 和 mcp、agent 运行时对话等；用户在 ora 内选择某 agent 对应的插件，与本地 agent 对话。
- 插件不仅含 agent 类，还会有 UI 插件、配置插件、即时通讯插件（微信/飞书等，可经 IM 控制本地 agent 工作并获取结果）。**本设计仅覆盖 agent 类插件**，其余后置。

### 1.2 通信协议的两层定位

- **ora-plugin-protocol**（`crates/plugin-protocol`）：ora 宿主 ↔ 插件进程的私有线协议。5 字节大端帧 `[type:i8][length:i32 BE][payload]`，payload 为 UTF-8 JSON-RPC 2.0（严格 profile：`id` 必须 `h:<n>`、禁 batch、拒重复键/超深/显式 null）。含 lifecycle（`$/initialize`/`$/activate`/`$/deactivate`/`$/exit`/`$/cancelRequest`/`$/stream`）与 agent contract v1（`discoverInstallations`/`getConfigurationSummary`/`listSkills`/`listMcpServers`/`listConversations`/`startConversation`/`sendMessage`/`cancelConversation`）。
- **ACP**（`crates/contracts/src/acp`）：Zed Agent Client Protocol，插件进程 ↔ 本地 agent 的开放协议。纯 JSON-RPC 2.0 DTO（无成帧规定），方法 `initialize`/`authenticate`/`session/*` + `session/update` 流式通知。

两层**正交**：ora-plugin-protocol 管"插件进程"的生命周期、能力发现与对话隧道（对话请求经它发给插件进程，插件产出 `AgentEvent` 流回宿主）；ACP 管"agent 会话"的对话（插件进程内与 agent 之间的 session）。design-v3（§13.2）把"插件进程如何对接 agent"留给了插件作者自由实现——这正是"用户实现插件管理时不知通信会用 ACP"的根源。**本设计把这部分明确为：插件进程内用 ACP 与本地 agent 通信，由 plugin-sdk 提供 ACP client 工具降低作者负担。**

### 1.3 实现现状（探索核实，含对记忆文件的更正）

> ⚠️ 记忆文件 `ora-plugin-protocol-status.md` 记录"M4 runtime actor 未做、Windows Job Object FFI 未实现"**已过期**。实际状态如下（由 workflow 4 agent 并行核实）。

| 层 | 状态 | 证据 |
|---|---|---|
| `ora-plugin-protocol`（线协议：frame/json_rpc/lifecycle/agent contract/identity/manifest） | ✅ 完整 | `crates/plugin-protocol/src/` |
| `ora-plugin-manager` 数据模型层（enablement/catalog/registry/state/grant/layout/limits/PluginError/results/facade-types） | ✅ 完整 | `crates/plugin-manager/src/*.rs` |
| `ora-plugin-manager` runtime actor 层（M4） | ✅ **已完整实现**（与记忆相反） | `crates/plugin-manager/src/runtime/`：`hub.rs`/`supervisor.rs`/`session_actor.rs`/`transport.rs`/`invocation.rs`；`AgentInvocationHandle.next_event()→AgentEvent`/`finish()→AgentInvocationResult`（`invocation.rs:21,75`）流式已实现 |
| `ora-process` Windows Job Object FFI（M3） | ✅ **已完整实现**（与记忆相反） | `crates/process/src/windows_tree.rs`：`CreateProcessW`+`CreateJobObjectW`+`TerminateJobObject`+IOCP，`PROC_THREAD_ATTRIBUTE_JOB_LIST`，fail-closed |
| facade traits | ✅ 齐备 | `PluginManagement`(`service.rs:51`) / `PluginRuntimeControl`(`ports.rs:91`) / `PluginRuntimeInvocation`(`ports.rs:145`，即 design-v3 §15.1 `AgentPluginRuntime`，源码已 rename) / `AgentInvocationHandle`(`invocation.rs:21`) |
| `plugin-sdk` `defineAgentPlugin` ABI + `plugin-runtime`(TS) bootstrap | ✅ 完整非空壳，旧 `getNums`/`returnNums` 已删 | `packages/plugin-sdk/src/agent/index.ts:123` / `packages/plugin-runtime/src/bootstrap/`；`AgentProvider` 接口（`agent/index.ts:88-120`）已要求作者实现 `startConversation`/`sendMessage` 返回 `AsyncGenerator<AgentEvent,AgentTurnResult>`；`encodeStream`（`envelope.ts:66`）dispatch 已实现 |
| `ora-contracts` ACP DTO | ✅ 完整 | `crates/contracts/src/acp/`（21 文件）+ `packages/contracts/src/acp/*.ts`（ts-rs 生成） |

**真正的缺口**（无既成"绕过"代码需兼容——设计自由度高）：

1. `crates/application` **不依赖** `ora_plugin_manager`；`PluginRuntimeHub`/`PluginManagementService` 仅在 `crates/plugin-manager/tests/{plugin_library_e2e,runtime_windows_e2e}.rs` 构造。
2. `packages/chat` 的 `AcpClient` 只有 `createMockAcpClient`（同进程 JS）和 `createUnavailableAcpClient`（抛 "ACP transport is not configured"，`packages/chat/src/client.ts:19`），与插件 runtime **零依赖、零相连**。
3. **plugin-sdk / plugin-runtime 没有任何 ACP 代码**：grep `acp` 在 `packages/plugin-runtime` 与 `packages/plugin-sdk` 零匹配（`stdio` 仅出现在 `AgentMcpTransport` 类型；`spawn` 仅在 `packages/plugin-sdk/src/pack/index.ts:264` 的打包工具 `node:child_process`，非运行时 spawn agent）。即 B2 的"插件进程内对接 agent 用 ACP"这一层尚未实现——design-v3 §13.2 把它留给作者自由，本设计要补。
4. 缺前端↔后端流式通道（`apps/web/server/src/routes.rs` 无 SSE/WebSocket/Tauri IPC；design-v3 §15.3 的 `/api/agent-invocations` NDJSON 端点未实现）。
5. 缺 UI 选择→enable+invoke 调用链。
6. `chat store` 只消费 `sessionUpdate==='agent_message_chunk' && content.type==='text'`（`packages/chat/src/store.ts:139`），丢弃 `tool_call`/`plan`/`permission`；`ChatMessage` 模型极简（无工具调用/附件/多模态）。
7. Ora 领域 `Session.agent_session_id: Option<String>`（`crates/application/src/session/mapper.rs`）当前装的是 mock uuid。

> 注：Rust 端 `ora_contracts::acp` grep 零匹配——ACP DTO 只被 `acp/mod.rs::export()` 用于 ts-rs 导出，无 Rust 运行时引用。B2 下 ACP 由 plugin-sdk（TS）侧消费，Rust 后端不直接接触 ACP，与此现状一致。

---

## 2. 目标与非目标

### 2.1 目标

1. 让 agent 插件对话能正确进行：用户选 agent 插件 → 发现本地 agent → 与本地 agent 流式对话 → 取消 → 正确回收。
2. 插件与 agent 之间用 ACP 通信。
3. **最大化复用已实现骨架**：ora-plugin-protocol conversation、plugin-sdk `AgentProvider`、plugin-runtime dispatch、`AgentInvocationHandle.next_event()` 流式——全部保留，不改 ABI。
4. 为后续 UI/配置/IM 类插件留出清晰的 manifest 判别联合与 executor 边界（不预实现）。
5. 满足 `AGENTS.md` "No Backward Compatibility"：迁移一次完成，不留兼容层（但本设计的"迁移"是新增 plugin-sdk ACP 工具 + 应用装配，而非废弃已实现骨架）。

### 2.2 非目标（后置）

- 具体 Claude Code / codex / opencode 插件实现（本设计只定契约与装配 + ACP 工具，不写具体 agent 插件）。
- UI/配置/IM 类插件 executor。
- 多模态 prompt（Image/Audio/Resource）、Plan、Permission 前端交互、Refusal/MaxTurnRequests（v1 后置，见 §10）。
- 插件市场、在线下载、签名、archive。
- 插件间 RPC broker、IM→Agent 调度授权。
- 多 profile / workspace scoped enablement。

---

## 3. ora↔ACP 语义对应分析

下表为 `ora-plugin-protocol` agent contract v1（design-v3 §13.1，已实现于 `crates/plugin-protocol/src/agent/dto.rs`）与 ACP（`crates/contracts/src/acp/`）的精确对应。**B2 下这些映射发生在 plugin-sdk 的 ACP client 工具内部（插件进程内），对 ora 宿主透明。**

### 3.1 lifecycle 层（独立，不冲突）

| ora lifecycle | ACP | 关系 |
|---|---|---|
| `$/initialize`（`lifecycle.rs:11`，协商 wireVersion=1/limits/declaredAgents/sessionId，entry import 前由 bootstrap 处理） | `initialize`（`initialization.rs`，协商 `ProtocolVersion`/`AgentCapabilities`/`auth_methods`） | 两层独立：ora lifecycle 管"插件进程"，ACP initialize 管"agent 会话" |
| `$/activate`（`lifecycle.rs:12`，校验 provider 集合与 manifest 精确匹配） | `authenticate`（`authentication.rs`） | 独立 |
| `$/deactivate`/`$/exit`（`lifecycle.rs:13,14`） | `logout`（`authentication.rs`） | 独立 |
| `$/cancelRequest`（`lifecycle.rs:15`，传输层取消 in-flight 请求） | `$/cancel_request`（`notification.rs:61`，JSON-RPC 协议级取消） | 独立 |

### 3.2 管理面（ora 私有，ACP 无对应）

| ora 方法 | ACP | 处理 |
|---|---|---|
| `agent.discoverInstallations`（`dto.rs:55`，探测本地 agent 安装与可用性） | 无 | 保留，插件自实现（plugin-sdk ACP 工具可辅助探测） |
| `agent.getConfigurationSummary`（`dto.rs:64`，脱敏配置项） | 无（ACP 有 `session/set_config_option` 但语义不同） | 保留 |
| `agent.listSkills`（`dto.rs:73`，分页 skill 摘要） | 无（ACP `available_commands`/`slash_command` 近似） | 保留 |
| `agent.listMcpServers`（`dto.rs:74`，展示型 MCP 投影） | 无（ACP `session/new.mcp_servers` 是启动参数） | 保留 |

### 3.3 对话面（语义鸿沟，由 plugin-sdk ACP 工具内部翻译）

| ora 方法 | ACP | 鸿沟（plugin-sdk 工具内处理） |
|---|---|---|
| `agent.startConversation`（`dto.rs:78`，无 conversationId 入参，首个事件 `ConversationStarted` 回传） | `session/new` + `session/prompt`（2 步，`session_id` 在 new response 直给，早于 prompt） | **2:1 拆分 + 时序**：工具先 `session/new` 拿 `session_id`，立即发 `ConversationStarted{conversationId=session_id}` 事件，再 `session/prompt` |
| `agent.sendMessage`（`dto.rs:90`，既有 conversation 续发） | `session/prompt` | 1:1 |
| `agent.cancelConversation`（`dto.rs:103`，同步 Request→`Accepted`/`AlreadyStopped`，safety_control） | `session/cancel`（`notification.rs:39`，异步 Notification，结果仅从 `stop_reason=Cancelled` 推断） | **语义鸿沟**：工具发 `session/cancel` + 等 `stop_reason=Cancelled`（deadline）→ `Accepted`；agent 已停 → `AlreadyStopped`；超时 → `CancellationUnconfirmed`（ora safety 模型，在插件进程侧） |
| `agent.listConversations`（`dto.rs:75`，按 provider+scope 分页） | `session/list`（按 cwd 过滤+cursor） | 工具映射 scope.workingDirectory→cwd |
| `AgentEvent.ConversationStarted{conversationId}` | `session/new` 响应的 `session_id` | 工具用 `session_id` 作 `conversationId` |
| `AgentEvent.TextDelta{channel=Assistant}`（`dto.rs:403`） | `SessionUpdate.AgentMessageChunk(ContentChunk(Text))`（`session.rs:557`） | 翻译 |
| `AgentEvent.TextDelta{channel=Reasoning}` | `SessionUpdate.AgentThoughtChunk` | 翻译 |
| `AgentEvent.TextDelta{channel=Tool}` | ACP 无直接对应（工具输出在 `ToolCallUpdate`） | v1 降级为 `ToolResult` 或忽略 |
| `AgentEvent.ToolCall`/`ToolResult` | `SessionUpdate.ToolCall`/`ToolCallUpdate` | 翻译 |
| `AgentEvent.Usage{input/output tokens,costMicros}` | `SessionUpdate.UsageUpdate{used,size,cost}`（`session.rs:673`） | 近似翻译（per-turn vs session 累计） |
| `AgentEvent.Status{phase,message}` | 无 | ora 额外，工具内部合成 |
| `AgentTurnResult.finishReason: Completed/Cancelled/Limit`（`dto.rs:464`） | `PromptResponse.stop_reason: EndTurn/MaxTokens/MaxTurnRequests/Refusal/Cancelled`（`prompt.rs:69`） | 多对一映射（见 §10.1） |
| `AgentScope.workingDirectory`（`dto.rs:12`，Host-issued，含 project/worktree handle） | `session/new.cwd` + `additional_directories` | 工具映射 cwd |
| `AgentConversationId`（插件生成，opaque） | `SessionId`（agent 生成） | 工具用 agent `session_id` 作 `conversationId` |
| `StreamParams`（`lifecycle.rs:295`，`$/stream` 通知） | `SessionNotification`（`notification.rs:15`，`session/update` 通知） | 两套流式通道：插件进程内 agent→工具（ACP `session/update`）→工具翻译→`$/stream`（ora 协议）→ora 宿主 |

### 3.4 核心问题诊断（5 鸿沟）

1. **多模态丢失**：ora `AgentPrompt` 是单 string；ACP `session/prompt` 是 `Vec<ContentBlock>`。v1 限 `[Text]`（见 §10）。
2. **Plan 丢失**：ACP 有 `SessionUpdate.Plan`；ora `AgentEvent` 无 Plan 变体。
3. **Permission 丢失**：ACP 有 `session/request_permission`（agent 同步阻塞）；ora 协议 v1 无 Plugin→Host 业务 Request（design-v3 §3 不变量 24）。
4. **Refusal 丢失**：ACP `stop_reason=Refusal`；ora `finishReason` 无对应。
5. **cancel 语义鸿沟**：见上表。

这 5 鸿沟全部由 **plugin-sdk ACP client 工具**在插件进程内翻译处理（见 §7.D），对 ora 宿主透明。v1 保守降级规则见 §10。

---

## 4. 方案选择与决策记录

经探索与权衡，提出三个方案：

- **方案 A**：插件进程内 ACP adapter，ora DTO 不变。与本设计 B2 接近，但 A 未明确"plugin-sdk 提供 ACP 工具"。
- **方案 B**：对话面对齐 ACP，废弃 ora 私有 conversation 方法。
- **方案 C**：双轨。

### 4.1 方案 B 的子形态决策（B1 → B2）

方案 B 下有两个子形态：

- **B1**：agent 进程由 ora 宿主直接 spawn（`ora-process` Job B），ora 宿主持 ACP stdio 直接对话；废弃 ora 私有 conversation 方法；ora 宿主新增 `AcpSessionManager`。
- **B2**：agent 进程由**插件进程** spawn（`Bun.spawn`），插件进程内用 ACP 与 agent 通信，把 agent 的 `session/update` 翻译成 ora `AgentEvent` 经 `$/stream` 流回宿主；**保留** ora 私有 conversation 骨架；plugin-sdk 提供 ACP client 工具降低作者负担。

**决策历史**：
- 2026-07-20：用户初选方案 B + B1 + v1 保守。
- 2026-07-21：核实代码后发现**当前已实现骨架强烈倾向 B2**（证据见 §1.3 与 §4.2），B1 需废弃已实现骨架。用户改决策：**从 B1 切到 B2**。

### 4.2 代码倾向 B2 的证据（精确到 file:line）

| 代码点 | 实际内容 | 倾向 |
|---|---|---|
| `crates/plugin-protocol/src/agent/dto.rs:78,90,103` | `StartConversation`/`SendMessage`/`CancelConversation`/`ListConversations` Request/Response 全部存在 | B2（对话面在插件侧产出） |
| `crates/plugin-protocol/src/agent/dto.rs:357,441` | `AgentEvent`（6 变体）+ `AgentTurnResult` 存在 | B2（插件产事件流） |
| `crates/plugin-protocol/src/lifecycle.rs:295` | `StreamParams`（`$/stream`） | B2（插件→宿主流式隧道） |
| `packages/plugin-sdk/src/agent/index.ts:108-119` | `AgentProvider` 接口要求作者实现 `startConversation`/`sendMessage` 返回 `AsyncGenerator<AgentEvent,AgentTurnResult>` | B2 铁证 |
| `packages/plugin-runtime/src/rpc/envelope.ts:66-68` | `encodeStream` 把作者 AsyncGenerator yield 经 `$/stream` 流回宿主 | B2 dispatch |
| `packages/plugin-runtime/src/bootstrap/main.ts:3` | `runBootstrap` 对 stdio 跑 private bootstrap，dispatch `agent.*` 请求 | B2 |
| `crates/plugin-manager/src/runtime/invocation.rs:21,75` | `AgentInvocationHandle.next_event()` 从 `mpsc<AgentEvent>` 取事件 | B2（宿主消费插件事件流） |
| grep `acp` in `packages/plugin-runtime` + `packages/plugin-sdk` | 零 ACP 引用；`spawn` 仅在 `plugin-sdk/src/pack/index.ts:264`（打包工具） | B2 的 ACP 层未实现（待补） |
| grep `ora_contracts::acp` 全 Rust | 零匹配 | B1 的 ACP 后端未实现 |
| `crates/application` Cargo.toml | 不依赖 `ora_plugin_manager` | 应用装配未做（B1/B2 都缺） |

### 4.3 选 B2 的理由

1. **最大化复用已实现骨架**：ora-plugin-protocol conversation（`dto.rs`/`lifecycle.rs`）、plugin-sdk `AgentProvider`（`agent/index.ts:88-120`）、plugin-runtime dispatch（`envelope.ts:66`/`bootstrap/`）、`AgentInvocationHandle.next_event()` 流式（`invocation.rs:21,75`）——全部保留不改 ABI。B1 需废弃这些。
2. **符合 design-v3 原设计**：§11.4 的 Job Object 覆盖 Bun+agent 子树（单 Job），§13.2 `AgentProvider.startConversation` 返回 AsyncGenerator（作者在插件进程内对话），§13.1 safety slot + `CancellationUnconfirmed` 在插件进程侧（已实现）。B2 与 design-v3 完全一致；B1 的"ora 宿主直接 ACP + 两独立 Job"是 design-v3 之外的偏离。
3. **改动最小**：B2 只需补"plugin-sdk ACP client 工具 + 应用装配 + 前端流式通道"，不重写已实现部分。
4. **语义鸿沟封装在 plugin-sdk 工具内**：5 鸿沟（多模态/Plan/Permission/Refusal/cancel）由工具内部翻译处理，ora 宿主透明，v1 保守降级即可。

### 4.4 B2 的代价

- ACP↔`AgentEvent` 翻译层在 plugin-sdk ACP 工具内，有语义损失（v1 降级，见 §10）。
- 每个插件进程内维护 ACP session 状态（工具内）。
- agent 进程树由插件进程 spawn，ora 宿主不直接控制 agent（但 Job A `KILL_ON_JOB_CLOSE` 覆盖 Bun+agent，回收仍保证）。

---

## 5. 架构设计（B2 形态）

### 5.1 进程拓扑

```
┌──────────────────────────────────────────────────────────────────────┐
│  ① WebView 渲染进程（前端）                                            │
│  packages/chat (chatStore + AcpClient) + packages/app-shell           │
│  [composer / 消息列表 / settings·plugins / agent 选择]                 │
└───────────┬───────────────────────────────────────────────────────────┘
            │  HTTP REST（管理面 + 对话面）+ SSE/NDJSON 流式（AgentEvent 透传）
            │  Authorization: Bearer <256-bit loopback bearer>
            │  （Tauri: IPC 传 token；Web: localhost fetch + ReadableStream）
            ▼
┌──────────────────────────────────────────────────────────────────────┐
│  ② Ora Rust 后端进程（BackendRuntime，唯一权威写者）                    │
│                                                                        │
│  ┌──────────────┐   ┌────────────────┐                                │
│  │PluginMgmtSvc │   │ PluginRuntimeHub│  (已实现, 全保留)              │
│  │scan/identify │   │  (Job A owner)  │                                │
│  │install/enable│   │  lifecycle +    │                                │
│  │/grant/crash- │   │  对话 dispatch   │                                │
│  │loop          │   │  (startConv/    │                                │
│  │              │   │   sendMessage/  │                                │
│  │              │   │   cancelConv +  │                                │
│  │              │   │   $/stream)     │                                │
│  └──────┬───────┘   └────────┬────────┘                                │
│         │  SQLite/磁盘        │                                          │
│         │  layout/registry    │  AgentInvocationHandle.next_event()     │
│         │  (后端内部,不跨进程)  │  (消费插件产出的 AgentEvent 流)          │
│         ▼                     ▼                                          │
└──────────────────────────────┼──────────────────────────────────────────┘
                                │
                 ora-plugin-protocol │ 5字节帧 + 严格 JSON-RPC
                 lifecycle:         │ $/initialize/activate/deactivate/exit
                 对话面(保留):       │ agent.startConversation/sendMessage/
                                     │ cancelConversation/listConversations
                 管理面:             │ discoverInstallations/getConfigSummary/
                                     │ listSkills/listMcpServers
                 流式:               │ $/stream(AgentEvent) + terminal AgentTurnResult
                                     ▼
   ┌──────────────────────────────────────────────────────────────────────┐
   │  ③ 插件进程（Bun + private bootstrap + 作者代码 + plugin-sdk ACP 工具）│
   │     Job A (Windows Job Object, KILL_ON_JOB_CLOSE, 覆盖 Bun+agent 子树)│
   │                                                                        │
   │  作者 AgentProvider 实现（用 plugin-sdk createAcpAgentProvider 工具）：   │
   │   • Bun.spawn agent (Claude Code/codex/opencode CLI)                   │
   │   • ACP client: initialize/authenticate/session/{new,prompt,cancel,list}
   │   • 翻译 session/update → AgentEvent                                    │
   │   • 翻译 PromptResponse → AgentTurnResult                              │
   │   • cancel → session/cancel + 等 stop_reason=Cancelled                  │
   │                                                                        │
   │  $/stream(AgentEvent) ──► 流回 ora 宿主 (经 ora-plugin-protocol)       │
   └──────────────────────────────────┬───────────────────────────────────┘
                                       │ ACP (newline-delimited JSON-RPC over stdio)
                                       │ initialize/authenticate/session/{new,prompt,cancel,list}
                                       │ session/update (流式通知, agent→client)
                                       │ session/request_permission (v2)
                                       │ PromptResponse{stop_reason}
                                       ▼
   ┌──────────────────────────────────────────────────────────────────────┐
   │  ④ 本地 agent 进程（Claude Code/codex/opencode CLI）                   │
   │     agent 是 ③ 的子进程，被 Job A 覆盖（不 breakaway）                   │
   │     ACP server 端                                                       │
   └──────────────────────────────────────────────────────────────────────┘
```

**单 Job Object**：ora 宿主为 ③ 插件进程建 Job A（`KILL_ON_JOB_CLOSE`，`windows_tree.rs` 已实现），覆盖 Bun + Bun 的后代（agent CLI，因 agent 不 breakaway）。**不需要 B1 的两个独立 Job**——这符合 design-v3 §11.4 原设计。杀 ora 宿主 → Job A 回收 Bun+agent 整树。

**关键：③ 和 ④ 之间用 ACP**（B2 的核心）；② 和 ③ 之间用 ora-plugin-protocol（保留对话面）；② 不直接接触 ④。

### 5.2 分层职责

| 层 | 职责 | 禁止 |
|---|---|---|
| Adapter（`apps/web/server` + Tauri 壳） | HTTP/Tauri I/O、loopback bearer、contract 映射、NDJSON 流 | 解析 wire、操作文件、持有进程状态 |
| Application（`crates/application`） | 对话编排、agent 选择、session 持久化 | 直接读插件目录或 child stdio |
| PluginManagement（`crates/plugin-manager`） | 扫描/验证/安装/状态/catalog/registry/插件进程 runtime + 对话 dispatch（$/stream→AgentInvocationHandle） | 执行 agent 业务代码（B2 下对话翻译在 plugin-sdk 工具内，非 plugin-manager） |
| Process（`crates/process`） | OS 进程、pipe、Job Object（覆盖 Bun+agent 子树） | 理解 JSON-RPC 或 agent 业务 |
| Private bootstrap（`packages/plugin-runtime`） | frame codec、lifecycle dispatch、对话面 dispatch（startConversation→作者 AsyncGenerator→$/stream） | — |
| plugin-sdk ACP 工具（`packages/plugin-sdk/src/acp`，新增） | spawn agent、ACP 握手、session/prompt 流式、SessionNotification→AgentEvent 翻译、cancel/crash 回收 | 不接触 ora-plugin-protocol wire（在插件进程内） |

---

## 6. 关键架构决策

### 6.1 agent 由插件进程 spawn（B2 核心）

agent 进程（Claude Code/codex/opencode CLI）由 **③ 插件进程** `Bun.spawn`，是 ③ 的子进程。ora 宿主（②）不直接 spawn、不直接接触 agent。ora 宿主的 Job A（为 ③ 建）覆盖 ③+④ 整树（`KILL_ON_JOB_CLOSE`，agent 不 breakaway）。

这与 design-v3 §11.4 一致：Job Object 覆盖 Bun + 后代 agent CLI。

### 6.2 ACP 落点：插件进程 ↔ agent 进程

ACP **只在 ③ ↔ ④ 之间**用。③ 作为 ACP client（plugin-sdk 工具实现）：`initialize`/`authenticate`/`session/new`/`session/prompt`/`session/cancel`/`session/list` + `session/update` 订阅 + `session/request_permission`（v2）。④ 作为 ACP server。

ora 宿主（②）**不接触 ACP**——它只说 ora-plugin-protocol，经 `AgentInvocationHandle.next_event()` 消费 ③ 产出的 `AgentEvent` 流。

### 6.3 ora-plugin-protocol 全保留（不改 ABI）

B2 下 ora-plugin-protocol **完全不改**：
- lifecycle（`$/initialize`/`activate`/`deactivate`/`exit`/`$/cancelRequest`/`$/stream`）不变。
- 对话面（`startConversation`/`sendMessage`/`cancelConversation`/`listConversations` + `AgentEvent`/`AgentTurnResult`/`StreamParams`）不变。
- 管理面（`discoverInstallations`/`getConfigurationSummary`/`listSkills`/`listMcpServers`）不变。
- `wireVersion=1`、`pluginApi=1`、agent `contractVersion=1` 三个版本轴**全部不变**。

这是 B2 相对 B1 的最大优势：不废弃已实现 DTO + 不升级版本轴 + 不改 plugin-sdk `AgentProvider` ABI。

### 6.4 ACP 成帧（plugin-sdk 工具侧）

ACP spec 不规定成帧。plugin-sdk ACP 工具与 agent 之间用 **newline-delimited JSON-RPC**（每条 `JsonRpcMessage` 序列化后加 `\n`）。理由：
- 最简，兼容 `packages/mock-service/src/acp.ts` 现有 mock 风格。
- 与 ora-plugin-protocol 的 5 字节帧隔离（5 字节帧只在 ②↔③ 用，agent 侧不感知）。
- 若未来 ACP spec 明确成帧或目标 agent 用 `Content-Length`（LSP 风格），按 §12 变更控制升级 plugin-sdk 工具。

### 6.5 plugin-sdk 提供 ACP client 工具（降低作者负担）

plugin-sdk 新增 public 子模块 `@ora-space/plugin-sdk/acp`，提供 `createAcpAgentProvider` 工具：作者 `activate` 时用它构造 `AgentProvider`，工具内部 `Bun.spawn` agent + ACP stdio + 翻译。作者无需自写 ACP client。作者仍可不用工具、自实现 `AgentProvider`（对接非 ACP agent，或自定义 ACP 客户端）。

### 6.6 safety slot 在插件进程侧（已实现）

cancel 的 `CancellationUnconfirmed` safety 模型（design-v3 §13.1）在**插件进程侧**——`AgentInvocationHandle`（`invocation.rs:21`）+ `$/cancelRequest`（`lifecycle.rs:15`）+ `session_actor.rs` 已实现。B2 下 plugin-sdk ACP 工具内部用 ACP `session/cancel` + 等 `stop_reason=Cancelled`，超时则让插件进程的 `cancelConversation` 返回 `CancellationUnconfirmed`（经 ora safety 矩阵，已实现）。**ora 宿主侧不新增 safety 逻辑**。

### 6.7 安全（沿用 design-v3 §14）

- ③ 插件进程由 ② 用 `EnvironmentPolicy::ClearAndAllowlist`（`crates/process/src/spec.rs`）spawn，env_clear 后叠 allowlist。
- Job A 设 `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`，不设 `BREAKAWAY_OK`。
- ③ 内 `Bun.spawn` agent（④）：agent 继承 ③ 的环境？**决策**：plugin-sdk ACP 工具 spawn agent 时用**显式 env**（从 `ExtensionContext` + 插件 manifest grant 解析），不盲目继承 ③ 全环境；agent 所需 token/secret 经 `ExtensionContext.storagePath` 或 grant 传递，不日志。
- loopback bearer（design-v3 §15.3）：② 启动生成 256-bit 内存 bearer，Tauri IPC 交 WebView；`/api/plugins*` 与 `/api/agent-invocations` 全部 `Authorization: Bearer` + Origin allowlist。
- agent 的 ACP `authenticate` 若需带外认证（如打开浏览器），由 agent 自行处理；plugin-sdk 工具转发 `auth_methods` 到作者代码/宿主展示（v1 自动选 `agent` 类型）。

---

## 7. 组件改动（精确到方法/文件）

### 7.A `ora-plugin-protocol`（`crates/plugin-protocol`）

**完全不改**。B2 保留全部：
- `frame.rs`/`json_rpc.rs`/`lifecycle.rs`（含 `$/stream`/`StreamParams`，`lifecycle.rs:16,295`）/`identity.rs`/`manifest.rs`
- 对话面 DTO：`StartConversation`/`SendMessage`/`CancelConversation`/`ListConversations` + `AgentEvent`/`AgentTurnResult`/`AgentOutputChannel`/`AgentFinishReason`/`AgentConversationId`/`AgentConversationSummary`/`ClientRequestId`（`agent/dto.rs`）
- 管理面：`discoverInstallations`/`getConfigurationSummary`/`listSkills`/`listMcpServers`（`agent/dto.rs` + `agent/method.rs`）
- `AgentRequest`/`AgentResponse` 枚举（`method.rs:204`）不变
- `wireVersion=1`/`pluginApi=1`/`contractVersion=1` 不变

`ts-rs` 导出（`packages/plugin-runtime/src/generated/plugin-protocol.ts` + `packages/plugin-sdk/src/types/plugin-protocol.ts`）无需刷新。

### 7.B `ora-contracts`（`crates/contracts/src/acp`）

**不改**（ACP DTO 全集保留）。被 plugin-sdk ACP 工具（§7.D）经 `packages/contracts/src/acp/*.ts` 消费。

### 7.C `ora-plugin-manager`（`crates/plugin-manager`）

**完全复用，不改**。runtime 层（`hub.rs`/`supervisor.rs`/`session_actor.rs`/`transport.rs`/`invocation.rs`）已实现 lifecycle + 对话 dispatch + `$/stream`→`AgentInvocationHandle.next_event()` 流式。facade traits（`PluginManagement`/`PluginRuntimeControl`/`PluginRuntimeInvocation`/`AgentInvocationHandle`）不变。`AgentRequest` 枚举保留全部 conversation 方法。

### 7.D 新增 `plugin-sdk` ACP client 工具（`packages/plugin-sdk/src/acp/`）

新子模块，public 导出 `@ora-space/plugin-sdk/acp`。依赖 `@ora/contracts`（ACP DTO，`packages/contracts/src/acp`）+ `packages/plugin-sdk/src/types`（ora DTO）。

核心导出：
- `createAcpAgentProvider(options: AcpAgentProviderOptions): AgentProvider` —— 返回一个实现了 `AgentProvider` 全 8 方法的对象，内部对接 ACP agent。
- `AcpAgentProviderOptions`：
  ```ts
  {
    // agent 启动方式（由插件作者/配置决定）
    spawn: { program: string; args: string[]; env?: Record<string,string>; cwd?: string };
    // ACP 协议版本（与 agent 协商；未指定则用工具默认最新）
    protocolVersion?: number;
    // 认证方法选择（v1 默认 'agent' 类型，自动）
    authenticate?: { methodId: string };
    // MCP servers 传给 session/new
    mcpServers?: acp.McpServer[];
    // 可选：作者自定义 session/update→AgentEvent 翻译覆盖点
    translateUpdate?: (update: acp.SessionUpdate) => AgentEvent | AgentEvent[] | null;
  }
  ```
- 工具内部实现（spawn + ACP + 翻译 + cancel safety）：
  - `Bun.spawn(options.spawn)` 建 agent 进程（④，子进程）；stdin/stdout pipe；newline JSON-RPC codec。
  - `activate` 时不 spawn（lazy）；`startConversation` 首次调用时 single-flight spawn + ACP `initialize`/`authenticate` + `session/new` + `session/prompt`。
  - `session/new` 拿 `session_id` → 立即发 `ConversationStarted{conversationId=session_id}`（经 AsyncGenerator yield）。
  - 订阅 agent `session/update` → 按 §10.1 翻译成 `AgentEvent` → yield。
  - `PromptResponse{stop_reason}` → 翻译成 `AgentTurnResult{finishReason}`（§10.1）→ AsyncGenerator return。
  - `sendMessage` = 已有 session_id → `session/prompt`（不 `session/new`）。
  - `cancelConversation` → ACP `session/cancel` notification → 等 `stop_reason=Cancelled`（deadline）→ `Accepted`；agent 已停 → `AlreadyStopped`；超时 → `CancellationUnconfirmed`（ora safety，`AgentBusinessFailureKind`）。
  - `listConversations` → ACP `session/list`（若 agent 支持 `sessionCapabilities.list`）→ 映射 `SessionInfo`→`AgentConversationSummary`；不支持则返回空 + diagnostic。
  - `discoverInstallations`/`getConfigurationSummary`/`listSkills`/`listMcpServers`：由作者另实现（探测本地 agent 安装、读配置、列 skills/mcp；ACP 无对应）。工具可提供辅助 helper（如 `probeLocalAgent(programName)` 探测 PATH 安装），但非必须。
  - crash：agent 进程退出 → 工具失败 pending `startConversation`/`sendMessage` AsyncGenerator → ora 宿主 `session_actor` 检测 ③ 进程或 `$/stream` EOF → `UnknownOutcome`（non-idempotent）。

- 成帧：newline-delimited JSON-RPC，`JsonRpcMessage`（`crates/contracts/src/acp/rpc.rs:117`）序列化 + `\n`。
- 错误映射：ACP 协议错误/agent crash → `AgentBusinessErrorData`（`-32000`，`AgentBusinessFailureKind`：`AgentProcessFailed`/`AuthenticationRequired`/`AgentUnavailable` 等）。

### 7.E `plugin-runtime`（`packages/plugin-runtime`）

**不改**。bootstrap dispatch（`envelope.ts:66`/`bootstrap/session.ts`/`bootstrap/main.ts`）已实现 lifecycle + 对话面（`startConversation`/`sendMessage`→作者 AsyncGenerator→`$/stream`）。`generated/plugin-protocol.ts` 不刷新。

### 7.F `crates/application`（应用层装配）

- `Cargo.toml` 增 `ora_plugin_manager` 依赖（**不增 `ora_acp_runtime`**——B2 下 ACP 在 plugin-sdk 侧，ora 宿主不接触 ACP）。
- `crates/application/src/plugin/`（新增模块）：`PluginApi`（管理面 + 对话面 service，委托 `PluginManagementService` + `PluginRuntimeHub`）。
- `AgentLaunchResolver` 实现 `LaunchValueResolver`（解析 ③ 插件进程的 grant env refs，供 ② spawn ③ 用；④ agent 的 env 由 plugin-sdk 工具在 ③ 内解析）。
- `BackendRuntime`（design-v3 §15.2）：bootstrap 构造 `PluginManagementService` + `PluginRuntimeHub` + `AgentLaunchResolver`，持 `ManagerLease`。
- `AppState`（`apps/web/server/src/app_state.rs:9`）增 `plugin_api: Arc<PluginApi>`。

### 7.G Adapter/API 路由（`apps/web/server/src/routes.rs` + `crates/contracts`）

新增（design-v3 §15.3，管理面 + 对话面）：
```
GET    /api/plugins
POST   /api/plugins/scan
POST   /api/plugins/identify
POST   /api/plugins/install
POST   /api/plugins/{id}/enable
POST   /api/plugins/{id}/disable
DELETE /api/plugins/{id}
PUT    /api/plugins/{id}/launch-grant
GET    /api/plugins/{id}/launch-grant
DELETE /api/plugins/{id}/launch-grant
POST   /api/plugins/{id}/reset-crash-loop
POST   /api/plugins/{id}/remove-data
POST   /api/plugins/{id}/start                 # 显式启动插件进程（lifecycle $/activate manualStart）
POST   /api/plugins/{id}/stop
POST   /api/plugins/{id}/discover              # 管理面：发现本地 agent 安装
GET    /api/plugins/{id}/configuration-summary
GET    /api/plugins/{id}/skills
GET    /api/plugins/{id}/mcp-servers
POST   /api/agent-invocations                  # 对话面：ora-plugin-protocol startConversation + $/stream→NDJSON
DELETE /api/agent-invocations/{invocation-id} # 对话面：cancelConversation
```

- `POST /api/agent-invocations` body（design-v3 §15.3）：`{ pluginId, installationId, scope, clientRequestId, prompt }`（对应 `StartConversationRequest`）。响应 `application/x-ndjson`，header 含 opaque `invocation-id`，每行 compact 单行 envelope（`event | completed | failed`），`event` payload 直接引用 `ora-plugin-protocol` 的 `AgentEvent` 类型（**不是** ACP `SessionNotification`——ora 宿主只见 `AgentEvent`）。fetch body abort 与 `DELETE` 都触发 `cancelConversation`。
- 续发（`sendMessage`）：`POST /api/agent-invocations` body 带 `conversationId` → 后端 `invoke(SendMessage)`；或 design-v3 §15.3 的 invocation 续发模型。
- DTO 在 `ora-contracts` 定义（引用 `ora-plugin-protocol` 的 `AgentEvent`/`AgentTurnResult`/`AgentScope` 等），ts-rs 导出到 `packages/contracts`，前端 SDK 复用。
- `ora-contracts` 只引用 `ora-plugin-protocol` 类型（**不复制** ACP shape——ACP 不出现在后端契约）。

### 7.H 前端（`packages/chat` + `packages/app-shell`）

- `AcpClient` 接口（`packages/chat/src/client.ts:12`）**改造**：原 `newSession`/`prompt`/`subscribe(SessionNotification)` 改为对齐 ora `AgentEvent` 模型——
  ```ts
  interface OraInvocationClient {
    startConversation(req: { pluginId, installationId, scope, prompt }): Promise<{
      conversationId: string;
      events: AsyncIterable<AgentEvent>;  // ora AgentEvent 流
      result: Promise<AgentTurnResult>;
    }>;
    sendMessage(req: { ...conversationId, prompt }): Promise<{ events: AsyncIterable<AgentEvent>; result: Promise<AgentTurnResult> }>;
    cancel(req: { conversationId }): Promise<CancelConversationResponse>;
  }
  ```
  production 实现走后端 `POST /api/agent-invocations`（fetch + ReadableStream 解析 NDJSON，每行 envelope 含 `AgentEvent`）。删 `createUnavailableAcpClient`。`createMockAcpClient` 改为 mock `AgentEvent` 流（供测试/web mock 模式）。
- `chat store`（`packages/chat/src/store.ts`）：消费 `AgentEvent`（ora 6 变体：`conversationStarted`/`textDelta{channel}`/`status`/`toolCall`/`toolResult`/`usage`），扩展渲染 `toolCall`/`toolResult`/`usage`；`textDelta.channel=Reasoning` 单独渲染（思考）；`status` 渲染为阶段提示。
- `ChatMessage` 模型（`store.ts:22`）扩展 tool call 字段。
- 会话创建 mutation（`packages/app-shell/src/state/hooks/use-workspace-mutations.ts:143`）改为：`POST /api/agent-invocations`（后端 `invoke(StartConversation)` → 插件进程 spawn agent + ACP + `session/new` + 首个 `ConversationStarted{conversationId}` 流回）→ 前端拿到 `conversationId` → `POST /api/sessions` 持久化 Ora Session（`agent_session_id = conversationId`，即 ACP `session_id` 经插件进程透传）。
- 会话恢复：v1 后置（Ora Session reload 仅显示元数据；v2 经插件进程 `session/list`/`load`）。

---

## 8. 端到端数据流（B2 经插件进程隧道）

### 8.1 agent 发现 + 选择

1. 用户启用 agent 插件 → `POST /api/plugins/{id}/enable` → `PluginManagement.enable`（持久化 user_enablement，close runtime admission）。
2. 前端 `POST /api/plugins/{id}/discover { scope }` → 后端 `PluginRuntimeHub.invoke(plugin_id, AgentRequest::DiscoverInstallations)` → `RuntimeActor` 懒启动插件进程（③，single-flight，ora 宿主 spawn Bun，Job A，`EnvironmentPolicy::ClearAndAllowlist`）→ `$/initialize`+`$/activate` → 作者 `activate` 返回 `providers`（若用 plugin-sdk 工具，`createAcpAgentProvider` 在此返回 provider，但 agent 未 spawn——lazy）→ 插件 `discoverInstallations`（探测本地 agent 安装）→ `[AgentInstallation]`。
3. （可选）`GET /api/plugins/{id}/configuration-summary`、`/skills`、`/mcp-servers` 展示。

### 8.2 首条对话（B2 隧道）

```
前端(①)          Ora后端(②)             插件进程(③)            agent(④)
  │                │                       │                       │
  │ ①发消息        │                       │                       │
  ├─POST /api/agent-invocations {pluginId,installationId,scope,prompt}─►│
  │                │ PluginRuntimeHub.invoke(StartConversation)
  │                │ ora-plugin-protocol 5字节帧:
  │                ├─agent.startConversation{...prompt}──────────────►│ (③)
  │                │                       │ plugin-sdk ACP 工具:     │
  │                │                       │ Bun.spawn agent (首次, single-flight, Job A 覆盖)
  │                │                       │ ═══ ACP 在 ③↔④ 开始 ═══│
  │                │                       ├──── initialize ────────────────────►│ (④)
  │                │                       │◄──── InitializeResponse ─────────────┤
  │                │                       ├──── authenticate ───────────────────►│
  │                │                       │◄──── AuthenticateResponse ────────────┤
  │                │                       ├──── session/new {cwd,mcp_servers} ──►│
  │                │                       │◄──── NewSessionResponse {session_id}─┤
  │                │                       │ 工具: conversationId = session_id    │
  │                │                       ├──── session/prompt {session_id,prompt:[Text]}─►│
  │                │                       │       (ACP 流式 session/update)      │
  │                │                       │◄──── session/update(AgentMessageChunk)─┤
  │                │                       │ 工具翻译→TextDelta{Assistant}         │
  │                │◄──$/stream{seq,AgentEvent:TextDelta}─────────────┤           │
  │                │ AgentInvocationHandle.next_event()               │           │
  │◄──NDJSON: event────────────────────────┤                       │           │
  │                │                       │◄──── session/update(ToolCall)──────┤
  │                │                       │ 工具翻译→ToolCall                    │
  │                │◄──$/stream{AgentEvent:ToolCall}─────────────────┤           │
  │◄──NDJSON: event────────────────────────┤                       │           │
  │                │                       │◄──── session/update(UsageUpdate)────┤
  │                │◄──$/stream{AgentEvent:Usage}──────────────────────┤           │
  │◄──NDJSON: event────────────────────────┤                       │           │
  │                │                       │◄──── PromptResponse{stop_reason}─────┤
  │                │                       │ 工具翻译→AgentTurnResult{finishReason}│
  │                │◄──JSON-RPC Response{result:AgentTurnResult}──────┤           │
  │                │ AgentInvocationHandle.finish()→AgentInvocationResult::Turn  │
  │◄──NDJSON: completed────────────────────┤                       │           │
  │                │ 持久化 Ora Session(agent_session_id = conversationId = ACP session_id)
  ├─POST /api/sessions ─────────────────►│  (前端落库)            │           │
```

关键：ora 宿主（②）只见 `AgentEvent`/`AgentTurnResult`（ora-plugin-protocol），**不见 ACP**。ACP ③↔④ 在插件进程内由 plugin-sdk 工具处理。

### 8.3 续发

`POST /api/agent-invocations { conversationId, prompt }` → 后端 `invoke(SendMessage)` → 插件进程 `sendMessage` → 工具 `session/prompt`（不 `session/new`，已有 session_id）→ 流式 → `AgentTurnResult`。

### 8.4 取消

`DELETE /api/agent-invocations/{id}`（或 fetch abort）→ 后端 `invoke(CancelConversation)` → 插件进程 `cancelConversation` → 工具发 ACP `session/cancel` notification → 等 `stop_reason=Cancelled`（deadline）→ `Accepted`；agent 已停 → `AlreadyStopped`；超时 → `CancellationUnconfirmed`（ora safety）→ 前端。

### 8.5 列历史（v1 后置）

v1 不实现 `listConversations` 的前端；后端 `PluginRuntimeHub.invoke(ListConversations)` 可预留。Ora Session reload 仅显示元数据。

---

## 9. 错误处理与生命周期

### 9.1 safety slot（插件进程侧，已实现）

cancel 的 `CancellationUnconfirmed` safety 模型在 **③ 插件进程侧**（design-v3 §13.1，已实现于 `crates/plugin-manager/src/runtime/session_actor.rs` + `invocation.rs:21` 的 `AgentInvocationHandle`）。plugin-sdk ACP 工具内部用 ACP `session/cancel` + 等 `stop_reason=Cancelled`，超时让插件的 `cancelConversation` 返回 `CancellationUnconfirmed`，经 ora 协议 `$/cancelRequest`（`lifecycle.rs:15`，传输层取消）+ `AgentInvocationHandle.cancel()` 收敛。**ora 宿主侧不新增 safety 逻辑**（复用已实现）。

### 9.2 agent spawn 失败（插件进程内）

plugin-sdk ACP 工具 `Bun.spawn` agent 失败：
- `program` 不存在/无权限 → `AgentBusinessErrorData{kind: AgentUnavailable, retryable:false}` → ora 宿主 `invoke(StartConversation)` 返回 `AgentInvocationResult::Error`。
- ACP `initialize` 失败（protocol_version 不支持）→ `AgentBusinessErrorData{kind: UnsupportedAgentCapability}`。
- ACP `authenticate` 失败 → `AgentBusinessErrorData{kind: AuthenticationRequired}` → 前端提示认证。

### 9.3 session/prompt 超时/crash

- agent 进程（④）退出 → plugin-sdk ACP 工具的 stdio EOF → 工具失败 pending AsyncGenerator。
- ③ 插件进程的 ora 宿主侧 `session_actor` 检测 ③ 进程退出或 `$/stream` EOF（`runtime/session_actor.rs`，已实现）→ 失败 pending `AgentInvocationHandle` → `UnknownOutcome`（non-idempotent `startConversation`/`sendMessage`，对齐 design-v3 §13.1，不自动重放）。
- agent crash 经 ③ 透传到 ②：ora 宿主 runtime actor 的 crash window（design-v3 §11.6，已实现，作用在 ③ 插件进程维度）。

### 9.4 进程树回收

- 杀 ③ 插件进程 → Job A `KILL_ON_JOB_CLOSE` 回收 ④ agent（agent 是 ③ 子进程，不 breakaway）。
- 杀 ② ora 宿主 → Job A 回收 ③+④ 整树。
- `disable`/`uninstall`/`shutdown`：ora 宿主 `PluginRuntimeHub.stop`（已实现）→ ③ `$/deactivate`→`$/exit` → ③ 退出 → Job A 回收 ④。

### 9.5 背压

- `AgentEvent` 经 `$/stream` 流式，`StreamParams.seq` 严格单调（`lifecycle.rs:295`，已实现）；ora 宿主 `AgentInvocationHandle` 的 `mpsc<AgentEvent>` 有界（已实现）。
- ACP `session/update` 在 plugin-sdk 工具内有界缓冲；满时工具发 ACP `$/cancel_request`（按 request_id）+ `session/cancel`。
- v1 `prompt` 限 `[Text]`（≤1 MiB，`AgentPrompt` 上限）；`AgentEvent` 单帧 ≤256 KiB，`AgentTurnResult` ≤1 MiB（design-v3 §13.1 hard cap，已实现于 `InitializeLimits`）。

---

## 10. v1 保守范围与后置

| 能力 | v1 | 后置（v2） |
|---|---|---|
| prompt 类型 | `[Text]` 单块（ora `AgentPrompt` string → ACP `[Text]`） | `Image`/`Audio`/`ResourceLink`/`Resource` 多模态（需 ora DTO 扩展 `AgentPrompt` 为 `Vec<ContentBlock>`，升级 `contractVersion`） |
| ACP `SessionUpdate` 变体 | `AgentMessageChunk`/`AgentThoughtChunk`/`ToolCall`/`ToolCallUpdate`/`UsageUpdate`/`SessionInfoUpdate` 翻译为 ora `AgentEvent` | `Plan`/`ConfigOptionUpdate`/`AvailableCommandsUpdate`/`CurrentModeUpdate`（ora 无对应变体，需扩展 `AgentEvent`） |
| `stop_reason` | `EndTurn`/`MaxTokens`/`Cancelled` 翻译为 `finishReason` | `Refusal`/`MaxTurnRequests`（需扩展 `AgentFinishReason`） |
| session 方法 | `new`/`prompt`/`cancel` | `load`/`resume`/`close`/`delete`/`list`/`set_mode`/`set_config_option`（ora `listConversations` 可对接 ACP `session/list`，v1 后置） |
| permission | plugin-sdk 工具自动 Allow+日志（`session/request_permission` → `Selected{AllowOnce}`） | 转前端交互（需扩展 ora 协议支持 Plugin→Host Request，打破 design-v3 §3 不变量 24，升级 `pluginApi`） |
| additional_directories | 不支持 | 支持（`session/new.additional_directories`） |
| 历史恢复 | Ora Session 元数据 only | `session/load`/`resume` 经插件进程回放 |

### 10.1 v1 降级规则（plugin-sdk ACP 工具内）

- 收到 `SessionUpdate.Plan`：v1 降级为 `AgentEvent.Status{phase:"plan"}` 或忽略 + 计数（前端不渲染 Plan）。
- 收到 `session/request_permission`：v1 自动回 `outcome: Selected{option_id: <first allow>}` + 日志 warn；后置 v2 转前端（需 ora 协议扩展）。
- 收到 `stop_reason=Refusal`：v1 映射为 `finishReason=Completed` + `AgentBusinessErrorData.details` 标记 `refusal`；后置 v2 扩展 `AgentFinishReason::Refusal`。
- 收到 `stop_reason=MaxTurnRequests`：v1 映射为 `finishReason=Limit`。
- 收到多模态 `ContentBlock`（非 Text）在 `session/update`：v1 忽略 + 计数（前端不渲染）。

### 10.2 v2 演进（按 design-v3 §22.5 变更控制）

扩展 ora `AgentEvent`/`AgentPrompt`/`AgentFinishReason` 需升级 agent `contractVersion`（1→2）+ 更新 DTO/golden/正反互操作测试 + ADR + 证明旧 runtime/新 agent 或新 runtime/旧 agent 在执行代码前 fail-closed。ACP 侧（`acpProtocolVersion`）由 plugin-sdk 工具与 agent 协商，独立于 ora 版本轴。

---

## 11. 测试策略

### 11.1 单元

- **plugin-sdk ACP 工具**：ACP newline-delimited 帧解析（复用 `crates/contracts/src/acp/rpc.rs` 的 `JsonRpcMessage`）；`SessionUpdate` 11 变体→`AgentEvent` 翻译矩阵；`StopReason`→`finishReason` 映射；cancel deadline/`CancellationUnconfirmed`；crash→`UnknownOutcome`。
- **ora-plugin-protocol**：不变（已实现，55+ lib tests 绿）。
- **literals.rs drift check**：plugin-sdk 工具用的 ACP 方法名与 `crates/contracts/src/acp/literals.rs` 对齐。

### 11.2 集成

- `crates/application` 装配 `PluginRuntimeHub` + `AgentLaunchResolver`；后端 mock 插件进程（`defineAgentPlugin` 用 mock `AgentProvider` 产 `AgentEvent` 流，不经真实 ACP）。
- 前端 mock：`createMockAcpClient` 改造为 mock `AgentEvent` 流（`packages/mock-service`）。
- 端到端：`enable` → `discover` → `POST /api/agent-invocations` → NDJSON 流 → `DELETE` cancel → 回收。

### 11.3 E2E（Windows）

- ora 宿主 spawn 真实 Bun 插件进程（③，Job A）→ 插件进程内 plugin-sdk 工具 `Bun.spawn` 真实 agent（④，Claude Code ACP）→ `session/new`/`prompt` 流式 → `session/cancel` → Job A 回收 ③+④ 整树。
- Job Object 回收验证：杀 ③ 不留 ④；杀 ② 回收 ③+④（`KILL_ON_JOB_CLOSE`）。
- named pipe（②↔③ 的 5 字节帧）+ agent stdio（③↔④ 的 newline JSON-RPC）的 Windows 真实 pipe E2E。

### 11.4 v1 门禁

- 多模态 `session/update` 的 negative test（v1 工具忽略 + 计数）。
- `Plan`/`Permission`/`Refusal`/`MaxTurnRequests` 的降级 test（v1 映射正确 + 日志）。
- `session/request_permission` 自动 Allow 的 test。

---

## 12. 迁移与变更控制

### 12.1 B2 的迁移（不废弃已实现骨架）

- `ora-plugin-protocol`：**不改**（conversation + lifecycle + 管理面全保留）。
- `ora-plugin-manager`：**不改**（runtime/facade 全保留）。
- `plugin-sdk`/`plugin-runtime`：**ABI 不改**（`AgentProvider` 接口不变，`defineAgentPlugin` 不变）；plugin-sdk 新增 `acp` 子模块（public 工具，非 ABI 变化）。
- `packages/chat`：`AcpClient` 改造（从 mock SessionNotification 切到 `AgentEvent` 流经后端中转）；删 `createUnavailableAcpClient`。
- `crates/application`：新增依赖 `ora_plugin_manager` + `plugin` 模块 + `AgentLaunchResolver`。
- 旧 `conversations-store`/mock-data 已在 commit 2857064 删除，无需再动。

### 12.2 三个独立版本轴（design-v3 §0.13、§22.5）——全部不变

- `wireVersion=1`（ora 宿主↔private bootstrap/runtime）：不变。
- `pluginApi=1`（bootstrap↔插件 module，manifest `engines.pluginApi`=1）：**不变**（`AgentProvider` ABI 未改）。
- agent `contractVersion=1`（manifest contribution `contractVersion`）：**不变**（对话面 DTO 未改；v2 扩展 `AgentEvent`/`AgentPrompt` 时升级为 2）。
- `acpProtocolVersion`（plugin-sdk 工具↔agent）：由工具与 agent `initialize` 协商，独立于 ora 版本轴；v1 用 ACP 当前版本（`crates/contracts/src/acp/initialization.rs` 的 `ProtocolVersion`）。

### 12.3 单一规范链

design-v3 → `ora-plugin-protocol`（管理面 + lifecycle + 对话面 DTO）+ `ora-contracts/acp`（ACP DTO，被 plugin-sdk 工具消费）→ ts-rs 生成 TS → golden fixture → Rust/TS/E2E。

---

## 13. 风险与未决

1. **ACP 真实 stdio 成帧**：ACP spec 不规定成帧，本设计 plugin-sdk 工具选 newline-delimited JSON-RPC。若目标 agent（Claude Code/codex/opencode）实际用 `Content-Length`（LSP 风格）或其他，工具成帧层需调整。**未决**：需在首个真实 agent E2E 时核实 Claude Code 的 ACP 成帧。
2. **`session/load` 历史回放语义**：ACP `LoadSessionResponse` 只含 `modes`/`config_options`，不含消息（`crates/contracts/src/acp/session.rs:200`）。v1 后置 `load`，v2 实现时需查上游 ACP spec 确认回放机制。
3. **`session/cancel` vs `$/cancel_request` 易混**：plugin-sdk 工具必须用对 DTO（`notification.rs:39` vs `notification.rs:61`）。前者取消整个 session 进行中操作（按 `session_id`），后者取消单个 JSON-RPC 请求（按 `request_id`）。
4. **ToolCall/Plan 是 complete-replace 语义**：agent 发更新时发全量 entries/content，plugin-sdk 工具不得按增量合并。
5. **plugin-sdk ACP 工具的 agent env 解析**：④ agent 的 env（API key 等）在 ③ 内解析。**未决**：env 来源——`ExtensionContext.storagePath` 读密钥文件？还是经 `LaunchValueReference` 传递？需在工具设计时确定（v1 倾向 `storagePath` 读密钥文件 + 插件 manifest grant 声明引用）。
6. **agent 进程是 ③ 子进程**：ora 宿主 Job A 覆盖 ③+④。但若 agent `Bun.spawn` 时设 `detached` 或 breakaway，Job A 不覆盖。**决策**：plugin-sdk 工具 `Bun.spawn` 不设 detached（继承 ③），保证 Job A 覆盖。
7. **`crates/application/agent_definition` 与插件 agent 的关系**：`agent_definition` 是 ora 自己的"可配置 agent 类型"概念（`crates/domain/src/agent_definition.rs`），与插件 agent provider 是两套。**未决**：是否让插件 agent 成为 application agent 的一种实现，需后续设计。
8. **`conversationId` = ACP `session_id`**：B2 下 plugin-sdk 工具用 agent 返回的 `session_id` 作 ora `conversationId`。Ora Session 删除时是否经插件进程 `session/close`，v1 可不做（仅标记），v2 实现。

---

## 14. 记忆更新（本设计副产物）

`ora-plugin-protocol-status.md` 已于 2026-07-20 更正：M3 Windows Job Object FFI、M4 runtime actor **均已完整实现**；剩余缺口为应用层未接线。本设计（2026-07-21 修订为 B2）补充：剩余缺口具体为 (a) plugin-sdk 新增 ACP client 工具、(b) `crates/application` 装配 `PluginRuntimeHub`、(c) 前端↔后端 NDJSON 流式通道、(d) UI 调用链。

---

## 15. 参考资料（file:line 索引）

### ACP（`crates/contracts/src/acp/`）
- 方法名表：`literals.rs:39,57,111`
- `SessionUpdate` 11 变体：`session.rs:557`
- `ContentChunk`/`ContentBlock`：`session.rs:733` / `content.rs:22`
- `StopReason`：`prompt.rs:69`
- `PromptRequest`/`PromptResponse`：`prompt.rs:16,52`
- `SessionNotification`/`CancelNotification`/`CancelRequestNotification`：`notification.rs:15,39,61`
- `initialize`/`AgentCapabilities`/`ClientCapabilities`：`initialization.rs:20,315,134`
- `authenticate`/`AuthMethod`：`authentication.rs:112,14`
- `session/new`/`load`/`resume`/`close`/`list`/`delete`：`session.rs:46,148,247,348,384,458`
- `session/request_permission`：`permission.rs:18`
- `JsonRpcMessage`/`RequestId`：`rpc.rs:117,25`

### ora-plugin-protocol（`crates/plugin-protocol/`）——B2 全保留
- frame codec：`frame.rs:69,122`
- JSON-RPC profile：`json_rpc.rs:56,183`
- lifecycle：`lifecycle.rs:10,11,12,13,14,15,16`；`InitializeParams`：`lifecycle.rs:19`；`StreamParams`：`lifecycle.rs:295`
- 对话面 DTO（B2 保留）：`agent/dto.rs:12,55,64,73,74,75,78,90,103,357,403,441,464`
- 方法常量：`agent/method.rs:13,18,19,20,204,247`
- leaf types：`agent/leaf.rs:95,102,122,161,224,269`
- TS 导出：`lib.rs:58,133`

### ora-plugin-manager（`crates/plugin-manager/`）——B2 全保留
- facade traits：`service.rs:51` / `ports.rs:91,145`
- `AgentInvocationHandle`（`next_event`/`finish` 流式）：`runtime/invocation.rs:21,75,90,138`
- runtime：`runtime/hub.rs:132,217,272` / `runtime/supervisor.rs:19,42` / `runtime/session_actor.rs:64` / `runtime/transport.rs:7,187,313`

### ora-process（`crates/process/`）
- `ProcessSpec`/`EnvironmentPolicy`：`spec.rs:19,39,62`
- `TokioProcessSpawner` env_clear：`tokio_process.rs:37`
- Windows Job Object：`windows_tree.rs:54,122,373,491,539,546,576,679`

### 前端（`packages/`）
- chat AcpClient（B2 改造为 `AgentEvent` 流）：`packages/chat/src/client.ts:12,19`
- chat store：`packages/chat/src/store.ts:22,139,200`
- mock ACP（B2 改造为 mock `AgentEvent`）：`packages/mock-service/src/acp.ts:33,72,79`
- plugin-sdk `AgentProvider`（B2 不改）：`packages/plugin-sdk/src/agent/index.ts:88,108,123`
- plugin-runtime bootstrap（B2 不改）：`packages/plugin-runtime/src/bootstrap/main.ts:4` / `session.ts:80` / `loader.ts:27` / `rpc/envelope.ts:66` / `transport/frame.ts:1`
- generated TS（B2 不刷新）：`packages/plugin-runtime/src/generated/plugin-protocol.ts:149`
- plugin-sdk 打包工具 spawn（非运行时）：`packages/plugin-sdk/src/pack/index.ts:264`

### 应用层（B2 装配缺口）
- `AppState`：`apps/web/server/src/app_state.rs:9`
- Session CRUD：`apps/web/server/src/handlers/sessions.rs:18` / `crates/application/src/session/handlers.rs:42` / `mapper.rs:6` / `ports.rs:6`
- routes：`apps/web/server/src/routes.rs:13`
- 会话创建 mutation：`packages/app-shell/src/state/hooks/use-workspace-mutations.ts:143`

### design-v3（`origin/codex/plugin-management-backend-v3:docs/plugin-management/design-v3.md`）
- §0 结论先行 / §3 不变量（24: v1 无 Plugin→Host Request）/ §4 架构 / §11.4 Windows 进程树（单 Job 覆盖 Bun+agent）/ §13.1 Agent Contract（safety slot + CancellationUnconfirmed）/ §13.2 插件 entry ABI / §14 安全 / §15.1 facade / §15.3 `/api/agent-invocations` NDJSON / §22.5 变更控制
