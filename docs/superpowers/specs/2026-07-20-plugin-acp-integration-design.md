# Ora 插件管理与 ACP 协议结合设计方案

> 状态：设计规格（待实现）
> 日期：2026-07-20
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

- **ora-plugin-protocol**（`crates/plugin-protocol`）：ora 宿主 ↔ 插件进程的私有线协议。5 字节大端帧 `[type:i8][length:i32 BE][payload]`，payload 为 UTF-8 JSON-RPC 2.0（严格 profile：`id` 必须 `h:<n>`、禁 batch、拒重复键/超深/显式 null）。含 lifecycle（`$/initialize`/`$/activate`/`$/deactivate`/`$/exit`/`$/cancelRequest`/`$/stream`）与 agent contract v1。
- **ACP**（`crates/contracts/src/acp`）：Zed Agent Client Protocol，插件进程（或 ora 宿主）↔ 本地 agent 的开放协议。纯 JSON-RPC 2.0 DTO（无成帧规定），方法 `initialize`/`authenticate`/`session/*` + `session/update` 流式通知。

两层**正交**：ora-plugin-protocol 管"插件进程"的生命周期与能力发现，ACP 管"agent 会话"的对话。design-v3（§13.2）把"插件进程如何对接 agent"留给了插件作者自由实现——这正是"用户实现插件管理时不知通信会用 ACP"的根源。

### 1.3 实现现状（探索核实，含对记忆文件的更正）

> ⚠️ 记忆文件 `ora-plugin-protocol-status.md` 记录"M4 runtime actor 未做、Windows Job Object FFI 未实现"**已过期**。实际状态如下（由 workflow 4 agent 并行核实）。

| 层 | 状态 | 证据 |
|---|---|---|
| `ora-plugin-protocol`（线协议：frame/json_rpc/lifecycle/agent contract/identity/manifest） | ✅ 完整 | `crates/plugin-protocol/src/` |
| `ora-plugin-manager` 数据模型层（enablement/catalog/registry/state/grant/layout/limits/PluginError/results/facade-types） | ✅ 完整 | `crates/plugin-manager/src/*.rs` |
| `ora-plugin-manager` runtime actor 层（M4） | ✅ **已完整实现**（与记忆相反） | `crates/plugin-manager/src/runtime/`：`hub.rs`/`supervisor.rs`/`session_actor.rs`/`transport.rs`/`invocation.rs` |
| `ora-process` Windows Job Object FFI（M3） | ✅ **已完整实现**（与记忆相反） | `crates/process/src/windows_tree.rs`：`CreateProcessW`+`CreateJobObjectW`+`TerminateJobObject`+IOCP，`PROC_THREAD_ATTRIBUTE_JOB_LIST`，fail-closed |
| facade traits | ✅ 齐备 | `PluginManagement`(`service.rs:51`) / `PluginRuntimeControl`(`ports.rs:91`) / `PluginRuntimeInvocation`(`ports.rs:145`，即 design-v3 §15.1 `AgentPluginRuntime`，源码已 rename) / `AgentInvocationHandle`(`invocation.rs:21`：`next_event()->AgentEvent`/`finish()->AgentInvocationResult`) |
| `plugin-sdk` `defineAgentPlugin` ABI + `plugin-runtime`(TS) bootstrap | ✅ 完整非空壳，旧 `getNums`/`returnNums` 已删 | `packages/plugin-sdk/src/agent/index.ts:123` / `packages/plugin-runtime/src/bootstrap/` |
| `ora-contracts` ACP DTO | ✅ 完整 | `crates/contracts/src/acp/`（21 文件）+ `packages/contracts/src/acp/*.ts`（ts-rs 生成） |

**真正的缺口全是"应用层未接线"**（无既成"绕过"代码需兼容——设计自由度高）：

1. `crates/application` **不依赖** `ora_plugin_manager`；`PluginRuntimeHub`/`PluginManagementService` 仅在 `crates/plugin-manager/tests/{plugin_library_e2e,runtime_windows_e2e}.rs` 构造。
2. `packages/chat` 的 `AcpClient` 只有 `createMockAcpClient`（同进程 JS）和 `createUnavailableAcpClient`（抛 "ACP transport is not configured"，`packages/chat/src/client.ts:19`），与插件 runtime **零依赖、零相连**。
3. 缺 ACP↔ora 对话面的桥接。
4. 缺前端↔后端流式通道（`apps/web/server/src/routes.rs` 无 SSE/WebSocket/Tauri IPC）。
5. 缺 UI 选择→enable+invoke 调用链。
6. `chat store` 只消费 `sessionUpdate==='agent_message_chunk' && content.type==='text'`（`packages/chat/src/store.ts:139`），丢弃 `tool_call`/`plan`/`permission`；`ChatMessage` 模型极简（无工具调用/附件/多模态）。
7. Ora 领域 `Session.agent_session_id: Option<String>`（`crates/application/src/session/mapper.rs`）当前装的是 mock uuid。

---

## 2. 目标与非目标

### 2.1 目标

1. 让 agent 插件对话能正确进行：用户选 agent 插件 → 发现本地 agent → 与本地 agent 流式对话 → 取消 → 正确回收。
2. 插件与 agent 之间用 ACP 通信。
3. 复用已实现的 `ora-plugin-manager` runtime/facade、`ora-process` Job Object、`ora-contracts` ACP DTO、`plugin-runtime`/`plugin-sdk`，不重写已就绪部分。
4. 为后续 UI/配置/IM 类插件留出清晰的 manifest 判别联合与 executor 边界（不预实现）。
5. 满足 `AGENTS.md` "No Backward Compatibility"：迁移一次完成，不留兼容层。

### 2.2 非目标（后置）

- 具体 Claude Code / codex / opencode 插件实现（本设计只定契约与装配，不写具体 agent 插件）。
- UI/配置/IM 类插件 executor。
- 多模态 prompt（Image/Audio/Resource）、Plan、Permission 前端交互、Refusal/MaxTurnRequests（v1 后置，见 §10）。
- 插件市场、在线下载、签名、archive。
- 插件间 RPC broker、IM→Agent 调度授权。
- 多 profile / workspace scoped enablement。

---

## 3. ora↔ACP 语义对应分析

下表为 `ora-plugin-protocol` agent contract v1（design-v3 §13.1，已实现于 `crates/plugin-protocol/src/agent/dto.rs`）与 ACP（`crates/contracts/src/acp/`）的精确对应。

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
| `agent.discoverInstallations`（`dto.rs:55`，探测本地 agent 安装与可用性） | 无 | 保留，插件自实现 |
| `agent.getConfigurationSummary`（`dto.rs:64`，脱敏配置项） | 无（ACP 有 `session/set_config_option` 但语义不同） | 保留 |
| `agent.listSkills`（`dto.rs:73`，分页 skill 摘要） | 无（ACP `available_commands`/`slash_command` 近似） | 保留 |
| `agent.listMcpServers`（`dto.rs:74`，展示型 MCP 投影） | 无（ACP `session/new.mcp_servers` 是启动参数） | 保留 |

### 3.3 对话面（语义鸿沟）

| ora 方法 | ACP | 鸿沟 |
|---|---|---|
| `agent.startConversation`（`dto.rs:78`，无 conversationId 入参，首个事件 `ConversationStarted` 回传） | `session/new` + `session/prompt`（2 步，`session_id` 在 new response 直给，早于 prompt） | **2:1 拆分 + 时序不同** |
| `agent.sendMessage`（`dto.rs:90`，既有 conversation 续发） | `session/prompt` | 1:1 |
| `agent.cancelConversation`（`dto.rs:103`，同步 Request→`Accepted`/`AlreadyStopped`，safety_control） | `session/cancel`（`notification.rs:39`，异步 Notification，结果仅从 `stop_reason=Cancelled` 推断） | **语义鸿沟**：冲击 design-v3 §13.1 `CancellationUnconfirmed` safety 模型 |
| `agent.listConversations`（`dto.rs:75`，按 provider+scope 分页） | `session/list`（按 cwd 过滤+cursor） | 近似但作用域模型不同 |
| `AgentEvent.ConversationStarted{conversationId}` | `session/new` 响应的 `session_id` | 时序不同 |
| `AgentEvent.TextDelta{channel=Assistant}`（`dto.rs:403`） | `SessionUpdate.AgentMessageChunk(ContentChunk(Text))`（`session.rs:557`） | 强对应 |
| `AgentEvent.TextDelta{channel=Reasoning}` | `SessionUpdate.AgentThoughtChunk` | 强对应 |
| `AgentEvent.TextDelta{channel=Tool}` | ACP 无直接对应（工具输出在 `ToolCallUpdate`） | 丢失 |
| `AgentEvent.ToolCall`/`ToolResult` | `SessionUpdate.ToolCall`/`ToolCallUpdate` | 近似 |
| `AgentEvent.Usage{input/output tokens,costMicros}` | `SessionUpdate.UsageUpdate{used,size,cost}`（`session.rs:673`） | 概念不同（per-turn vs session 累计） |
| `AgentEvent.Status{phase,message}` | 无 | ora 额外 |
| `AgentTurnResult.finishReason: Completed/Cancelled/Limit`（`dto.rs:464`） | `PromptResponse.stop_reason: EndTurn/MaxTokens/MaxTurnRequests/Refusal/Cancelled`（`prompt.rs:69`） | 多对一；ACP 有 `Refusal`/`MaxTurnRequests`，ora 无 |
| `AgentScope.workingDirectory`（`dto.rs:12`，Host-issued，含 project/worktree handle） | `session/new.cwd` + `additional_directories` | 可对应（cwd）；project/worktree handle 是 ora 私有 |
| `AgentConversationId`（插件生成，opaque） | `SessionId`（agent 生成） | 所有权与时序不同 |
| `StreamParams`（`lifecycle.rs:295`，`$/stream` 通知） | `SessionNotification`（`notification.rs:15`，`session/update` 通知） | 两套流式通道 |

### 3.4 核心问题诊断（5 鸿沟）

1. **多模态丢失**：ora `AgentPrompt` 是单 string（`dto.rs` leaf type，1..=1 MiB）；ACP `session/prompt` 是 `Vec<ContentBlock>`（`Text`/`Image`/`Audio`/`ResourceLink`/`Resource`，`content.rs:22`）。
2. **Plan 丢失**：ACP 有 `SessionUpdate.Plan`（`session.rs:569`）；ora `AgentEvent` 无 Plan 变体（design-v3 把 thought 并入 `TextDelta.channel=Reasoning`，无 Plan）。
3. **Permission 丢失**：ACP 有 `session/request_permission`（`permission.rs:18`，agent 同步阻塞等 client 选 `Allow`/`Reject`）；ora 协议 v1 **无 Plugin→Host 业务 Request**（design-v3 §3 不变量 24 明确为空）。
4. **Refusal 丢失**：ACP `stop_reason=Refusal`；ora `finishReason` 只有 `Completed`/`Cancelled`/`Limit`。
5. **cancel 语义鸿沟**：见上表 `cancelConversation` 行。

---

## 4. 方案选择与决策记录

经探索与权衡，提出三个方案：

- **方案 A**：插件进程内 ACP adapter。ora 宿主只说 ora-plugin-protocol，`plugin-sdk` 提供 `createAcpAgentProvider` 在插件进程内做 `SessionNotification↔AgentEvent` 翻译。复用已实现 runtime/facade，ora DTO 不变。5 鸿沟用 v1 保守映射 + 后置扩展处理。
- **方案 B**：对话面对齐 ACP。废弃 ora 私有 conversation 方法（`startConversation`/`sendMessage`/`cancelConversation`/`listConversations`/`AgentEvent`/`AgentTurnResult`），对话面改用 ACP `session/*` + `SessionUpdate` + `PromptResponse`。无语义损失，但需重写已实现 agent contract DTO、ora 宿主要懂 ACP、safety/cancel 模型重设计。
- **方案 C**：双轨。ora 插件 agent（方案 A 路径）+ 纯 ACP agent（后端直接 spawn）并存。灵活但复杂度最高，偏离"通过插件集成 agent"初衷。

**用户决策（2026-07-20 AskUserQuestion）**：
- 总体架构：**方案 B**（对话面对齐 ACP，废弃 ora 私有 conversation）。
- v1 能力范围：**保守**（v1 限 text prompt + 基础事件，多模态/Plan/Permission/Refusal 后置）。

### 4.1 方案 B 的子形态：B1（agent 由 ora 宿主 spawn）

方案 B 下进一步确定：**agent 进程由 ora 宿主直接 spawn**（`ora-process` ProcessTree/Job Object），ora 宿主持 ACP stdio 直接对话；不走"插件进程 spawn agent + ora-plugin-protocol 隧道 ACP"的 B2 形态。

理由：
- B1 对话面纯 ACP，无隧道、无 `ora↔ACP` DTO 翻译层，复用已实现 `ora-process` `WindowsJobProcessTree`。
- 插件进程退化为"agent 适配器"（发现/配置/启动规格），轻量，符合"插件=适配不同 agent 差异"的定位。
- v1 保守要求最简路径；B2 的隧道复杂度后置。

B2（插件进程 own agent 进程树 + 隧道）作为 v2 演进项保留，若未来需要"插件进程对 agent 有更强控制"再升级。

---

## 5. 架构设计（B1 形态）

### 5.1 进程拓扑

```
WebView (chat UI: packages/chat + packages/app-shell)
   │  Tauri IPC (loopback bearer) / SSE
   ▼
Ora Rust 后端 (BackendRuntime)
 ├─ PluginManagementService  (管理面: scan/identify/install/enable/grant)        [design-v3 §15.1, 已实现]
 ├─ PluginRuntimeHub         (插件进程 runtime, 已实现)                              [hub.rs/supervisor.rs]
 │    └─ RuntimeActor: agent 插件 Bun 进程  [Windows Job Object A]
 │         └─ 仅跑 lifecycle + 管理面
 │            (discoverInstallations/describeLaunch/getConfigurationSummary/
 │             listSkills/listMcpServers)
 └─ AcpSessionManager         (对话面, 新增, §7.D)
      └─ spawn 本地 agent (Claude Code/codex/opencode) via ora-process ProcessTree  [Windows Job Object B]
      └─ ACP stdio: initialize/authenticate/session/{new,prompt,cancel,list}
         + session/update 流式通知
```

**两个独立进程树**：
- 插件进程（Bun，跑适配器代码，Job A）——由 `PluginRuntimeHub`/`RuntimeActor` 管理（已实现）。
- agent 进程（CLI，ora 宿主直接 spawn，Job B）——由新增 `AcpSessionManager` 管理，复用 `ora-process` `WindowsJobProcessTree`（`crates/process/src/windows_tree.rs`）。

### 5.2 分层职责

| 层 | 职责 | 禁止 |
|---|---|---|
| Adapter（`apps/web/server` + Tauri 壳） | HTTP/Tauri I/O、loopback bearer、contract 映射 | 解析 wire、操作文件、持有进程状态 |
| Application（`crates/application`） | 对话编排、agent 选择、session 持久化 | 直接读插件目录或 child stdio |
| PluginManagement（`crates/plugin-manager`） | 扫描/验证/安装/状态/catalog/registry/插件进程 runtime | 执行 agent 业务代码 |
| AcpRuntime（新增 `crates/acp-runtime`） | agent 进程 spawn、ACP 握手、session/prompt 流式、cancel/crash 回收 | 修改安装事实或 enablement |
| Process（`crates/process`） | OS 进程、pipe、Job Object（A+B 两个树） | 理解 JSON-RPC 或 agent 业务 |
| Private bootstrap（`packages/plugin-runtime`） | frame codec、lifecycle dispatch、管理面分派 | 对话面（B1 下插件进程不参与对话） |

---

## 6. 关键架构决策

### 6.1 ora 宿主直接持 ACP 连接（不走插件进程隧道）

ora 宿主经 `AcpSessionManager` 直接 spawn agent 进程并持 ACP stdio。对话面请求/响应/通知在 ora 宿主侧用 `crates/contracts/src/acp` DTO 构造与解析。前端经 Tauri IPC/SSE 收到 `session/update` 透传。

### 6.2 ACP 成帧（ora 宿主侧）

ACP spec 不规定成帧。`AcpSessionManager` 与 agent 之间用 **newline-delimited JSON-RPC**（每条 `JsonRpcMessage` 序列化后加 `\n`）。理由：
- 最简，兼容 `packages/mock-service/src/acp.ts` 现有实现风格。
- 与 ora-plugin-protocol 的 5 字节帧**隔离**（5 字节帧只在 ora 宿主↔插件进程之间用，agent 侧不感知）。
- 若未来 ACP spec 明确成帧，按 §12 变更控制升级。

### 6.3 插件进程角色（B1 退化）

插件进程只跑：
- lifecycle（`$/initialize`/`$/activate`/`$/deactivate`/`$/exit`，管插件进程本身）
- 管理面 5 方法：`discoverInstallations`/`describeLaunch`（新增）/`getConfigurationSummary`/`listSkills`/`listMcpServers`

不再跑 `startConversation`/`sendMessage`/`cancelConversation`/`listConversations`/`$/stream`。

### 6.4 agent 启动规格来源

ora 宿主 spawn agent 需 `program`/`args`/`cwd`/`env`。来源：
- `program`/`args`：插件 `agent.describeLaunch(installationId, scope)` 返回。
- `cwd`：ora 从当前 project/worktree 模型解析（复用 design-v3 §13.1 `AgentScope.workingDirectory` 的 Host-resolved 绝对路径）。
- `env`：`PluginLaunchGrant`（design-v3 §14.3）授权的 `LaunchValueReference`，由 `LaunchValueResolver` 在 spawn 时解析为 `ResolvedLaunchValue`（`Plain`/`Secret`），不持久化、不日志。
- `acpProtocolVersion`：`describeLaunch` 返回，用于 `initialize` 协商。

### 6.5 安全（沿用 design-v3 §14）

- ora 宿主 spawn agent 用 `EnvironmentPolicy::ClearAndAllowlist`（`crates/process/src/spec.rs`，`tokio_process.rs:37`），env_clear 后叠 allowlist。
- agent 进程 Job B 设 `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`，不设 `BREAKAWAY_OK`。
- loopback bearer（design-v3 §15.3）：ora 宿主启动生成 256-bit 内存 bearer，Tauri IPC 交 WebView；`/api/plugins*` 与 `/api/acp/*` 全部 `Authorization: Bearer` + Origin allowlist。
- agent 的 ACP `authenticate` 若需带外认证（如打开浏览器），由 agent 自行处理；ora 宿主只转发 `auth_methods` 到前端展示（v1 自动选 `agent` 类型）。

---

## 7. 组件改动（精确到方法/文件）

### 7.A `ora-plugin-protocol`（`crates/plugin-protocol`）

**保留**：
- `frame.rs`（5 字节帧 codec，`FRAME_HEADER_BYTES=5`，`MAX_FRAME_BYTES=8MB`）
- `json_rpc.rs`（严格 JSON-RPC profile，`HostRequestId` 形如 `h:<n>`）
- `lifecycle.rs`（`$/initialize`/`$/activate`/`$/deactivate`/`$/exit`/`$/cancelRequest`；**移除 `$/stream` 即 `METHOD_STREAM`/`StreamParams`**，`lifecycle.rs:16,295`）
- `identity.rs`（`PluginId`/`AgentProviderId`/`SessionId`/`PluginKindTag`）
- `manifest.rs`（`PluginManifest`，kind 路由 `agent`/`workbench`）
- 管理面方法与 DTO：`discoverInstallations`/`getConfigurationSummary`/`listSkills`/`listMcpServers`（`agent/dto.rs` + `agent/method.rs`）

**废弃**（§22.4 无兼容迁移，一次完成）：
- `agent/dto.rs`：`StartConversationRequest`/`SendMessageRequest`/`CancelConversationRequest`/`ListConversationsRequest` 及其 Response、`AgentEvent`、`AgentTurnResult`、`AgentOutputChannel`、`AgentFinishReason`、`AgentConversationId`、`AgentConversationSummary`、`ClientRequestId`（后三者仅被已废弃的对话面方法引用，管理面 idempotent 方法不用，一并废弃）
- `agent/method.rs`：`START_CONVERSATION`/`SEND_MESSAGE`/`CANCEL_CONVERSATION`/`LIST_CONVERSATIONS` 常量；`AgentResponse` 枚举（`method.rs:204`）收敛为管理面 5 变体
- `lifecycle.rs`：`METHOD_STREAM`、`StreamParams`

**新增**：
- `agent/dto.rs`：`DescribeLaunchRequest{ provider_id: AgentProviderId, installation_id: AgentInstallationId, scope: AgentScope }` → `DescribeLaunchResponse{ program: HostResolvedAbsolutePath, args: Vec<String>, cwd: HostResolvedAbsolutePath, env_refs: Vec<EnvironmentBindingRef>, acp_protocol_version: u16, auth_method_ids: Option<Vec<AuthMethodIdString>> }`
  - `EnvironmentBindingRef` 复用 design-v3 §14.3 `LaunchValueReference` 的引用形式（`HostConfiguration{key}`/`Credential{key}`/`DiscoveredExecutable{provider}`/`AuthorizedPath{path_id}`），只传引用不传值。
  - `AuthMethodIdString` 为 opaque string（与 ACP `AuthMethodId` 对齐，但 ora-plugin-protocol 不直接依赖 `ora-contracts` 的 ACP 类型，用透明 string 传递，避免 crate 耦合）。
- `agent/method.rs`：`DESCRIBE_LAUNCH` 常量；`AgentRequest`/`AgentResponse` 增 `DescribeLaunch` 变体。

**ts-rs 导出刷新**：`packages/plugin-runtime/src/generated/plugin-protocol.ts`（移除 conversation DTO，增 `DescribeLaunch`）、`packages/plugin-sdk/src/types/plugin-protocol.ts`。

### 7.B `ora-contracts`（`crates/contracts/src/acp`）

**不改**（ACP DTO 全集保留）。v1 保守子集由 `AcpSessionManager` 运行时选择（contracts 保留全集供 v2 演进，符合 §22.5 单一规范链）。

### 7.C `ora-plugin-manager`（`crates/plugin-manager`）

- runtime 层（`hub.rs`/`supervisor.rs`/`session_actor.rs`/`transport.rs`/`invocation.rs`）**完整复用**，仅 `AgentRequest` 枚举随 §7.A 收敛为管理面（移除 `StartConversation`/`SendMessage`/`CancelConversation`，增 `DescribeLaunch`）。
- `AgentInvocationHandle`（`invocation.rs:21`）：管理面方法用 `finish()→AgentInvocationResult::Response`；无 `next_event()` 流式（流式只在对话面 ACP 侧，`AcpSessionManager` 另持）。
- facade traits 不变：`PluginManagement`（`service.rs:51`）/`PluginRuntimeControl`（`ports.rs:91`）/`PluginRuntimeInvocation`（`ports.rs:145`）。

### 7.D 新增 `ora-acp-runtime`（`crates/acp-runtime`）

新 crate，依赖 `ora-contracts`（ACP DTO）+ `ora-process`（ProcessTree/Job Object）+ `ora-plugin-protocol`（`PluginLimits` 共享上限语义，若需要）。

核心类型：
- `AcpSessionManager`：持 `(plugin_id, installation_id) → AcpSessionState{ agent_tree: ManagedProcessTree, session_id: Option<SessionId>, agent_capabilities: AgentCapabilities, auth_methods: Vec<AuthMethod> }`。
- `AcpSessionHandle`：对话句柄，`prompt(PromptRequest) -> Stream<SessionNotification, PromptResponse>`、`cancel()`、`close()`。
- 方法：`launch_agent(spec: AgentLaunchSpec) -> Result<AcpSessionHandle, AcpRuntimeError>`（spawn + initialize + authenticate）、`new_session(cwd, mcp_servers) -> SessionId`、`prompt`、`cancel`、`list_sessions`、`close_session`、`shutdown_all`。
- 成帧：newline-delimited JSON-RPC（`\n` 分隔），`JsonRpcMessage`（`crates/contracts/src/acp/rpc.rs:117`）序列化。
- 错误：`AcpRuntimeError`（`SpawnFailed`/`TreeKillUnavailable`/`InitializeFailed`/`AuthFailed`/`ProtocolMismatch`/`SessionNotFound`/`UnknownOutcome`/`CancellationUnconfirmed`/`BackpressureExceeded`）。

### 7.E `plugin-sdk`（`packages/plugin-sdk`）+ `plugin-runtime`（`packages/plugin-runtime`）

- `defineAgentPlugin` ABI 改：`AgentProvider` 接口（`packages/plugin-sdk/src/agent/index.ts:111`）移除 `startConversation`/`sendMessage`/`cancelConversation`，新增 `describeLaunch(call, DescribeLaunchRequest) -> Promise<DescribeLaunchResponse>`；保留 `discoverInstallations`/`getConfigurationSummary`/`listSkills`/`listMcpServers`。
- `plugin-runtime` bootstrap（`packages/plugin-runtime/src/bootstrap/session.ts`）：移除 `$/stream` dispatch（`envelope.ts:66` 的 `encodeStream`）、`startConversation`/`sendMessage` 的 AsyncGenerator 处理；保留 lifecycle + 管理面分派。
- `generated/plugin-protocol.ts` 随 §7.A 刷新。
- 旧 `getNums`/`returnNums`/NDJSON reader-writer 已删（核实无 grep 匹配）。

### 7.F `crates/application`（应用层装配）

- `Cargo.toml` 增 `ora_plugin_manager` + `ora_acp_runtime` 依赖。
- `crates/application/src/plugin/`（新增模块）：`PluginApi`（管理面 service，委托 `PluginManagementService`）。
- `crates/application/src/acp/`（新增模块）：`AcpApi`（对话面 service，委托 `AcpSessionManager`）；`AgentLaunchResolver` 实现 `LaunchValueResolver`（解析 `HostConfiguration`/`Credential`/`DiscoveredExecutable`/`AuthorizedPath`，值源来自 ora 配置/密钥库）。
- `BackendRuntime`（design-v3 §15.2，`apps/desktop/src-tauri` 与 `apps/web/server` 共用）：bootstrap 构造 `PluginManagementService` + `PluginRuntimeHub` + `AcpSessionManager` + `AgentLaunchResolver`，持 `ManagerLease` 到 shutdown。
- `AppState`（`apps/web/server/src/app_state.rs:9`）增 `plugin_api: Arc<PluginApi>` + `acp_api: Arc<AcpApi>`。

### 7.G Adapter/API 路由（`apps/web/server/src/routes.rs` + `crates/contracts`）

新增（design-v3 §15.3 管理面 + 对话面）：
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
POST   /api/plugins/{id}/start                 # 显式启动插件进程（管理面可用）
POST   /api/plugins/{id}/stop
POST   /api/plugins/{id}/discover              # 管理面：发现本地 agent 安装
POST   /api/plugins/{id}/describe-launch       # 管理面：agent 启动规格
GET    /api/plugins/{id}/configuration-summary
GET    /api/plugins/{id}/skills
GET    /api/plugins/{id}/mcp-servers
POST   /api/acp/sessions                       # 对话面：spawn agent + ACP new
GET    /api/acp/sessions                       # session/list（v1 可后置）
POST   /api/acp/sessions/{id}/prompt            # 对话面：SSE 流式 session/prompt
POST   /api/acp/sessions/{id}/cancel           # 对话面：session/cancel
DELETE /api/acp/sessions/{id}                  # 对话面：session/close
```

- `POST /api/acp/sessions` body：`{ installationId, cwd, mcpServers? }`（不含 prompt）；创建 = spawn agent + `initialize` + `authenticate` + `session/new`，不流式；响应 body 为 `{ sessionId, agentInfo, agentCapabilities }`，header 含 opaque `invocation-id`。
- `POST /api/acp/sessions/{id}/prompt` 响应 `application/x-ndjson`（SSE-like），每行 compact 单行 envelope（`event | completed | failed`），payload 直接引用 `crates/contracts/src/acp` 类型（`SessionNotification`/`PromptResponse`）。fetch body abort 与 DELETE 都触发 `session/cancel`。
- DTO 在 `ora-contracts` 定义，ts-rs 导出到 `packages/contracts`，前端 SDK 复用。
- `ora-contracts` 只引用 `ora-plugin-protocol` 的管理面 DTO（`DescribeLaunch` 等），或定义带显式转换测试的 wrapper；**不复制** agent event/result/error shape（对话面已废弃 ora shape，直接用 ACP shape）。

### 7.H 前端（`packages/chat` + `packages/app-shell`）

- `AcpClient` 接口（`packages/chat/src/client.ts:12`）保留 `newSession`/`prompt`/`subscribe`，production 实现改走后端 HTTP/SSE（`fetch` + `ReadableStream`/`EventSource`，Tauri 环境用 Tauri IPC）。删 `createUnavailableAcpClient`。
- `chat store`（`packages/chat/src/store.ts:139`）扩展消费 `SessionUpdate` 的 `agentMessageChunk`/`agentThoughtChunk`/`toolCall`/`toolCallUpdate`/`usageUpdate`/`sessionInfoUpdate`（v1）；`plan`/`configOptionUpdate`/`availableCommandsUpdate`/`currentModeUpdate` 后置忽略并计数。
- `ChatMessage` 模型（`store.ts:22`）扩展 tool call（`id`/`name`/`summary`/`isError`/`args` 展示字段）。
- 会话创建 mutation（`packages/app-shell/src/state/hooks/use-workspace-mutations.ts:143`）改为：先 `POST /api/acp/sessions`（后端 spawn agent + ACP `session/new`，返回 `sessionId`）→ 再 `POST /api/sessions`（Ora 领域 Session 持久化，`agent_session_id = sessionId`）。
- 会话恢复：Ora Session 重载时，前端用持久化的 `agent_session_id` 调 `POST /api/acp/sessions/{id}/resume` 或重新 `new` + `load`（v1 后置，v1 reload 不恢复 ACP 会话，仅显示 Ora Session 元数据；v2 实现 `session/load`）。

---

## 8. 端到端数据流（v1 对话）

### 8.1 agent 发现 + 选择

1. 用户在 settings/plugins 启用 agent 插件 → `POST /api/plugins/{id}/enable` → `PluginManagement.enable`（持久化 user_enablement，close runtime admission）。
2. 前端 `POST /api/plugins/{id}/discover { scope }` → 后端 `PluginRuntimeHub.invoke(plugin_id, AgentRequest::DiscoverInstallations)` → `RuntimeActor` 懒启动插件进程（single-flight，已实现）→ `$/initialize`+`$/activate` → 插件探测本地 agent 安装 → `[AgentInstallation{installationId, displayName, version?, locationDisplay?, availability}]`。
3. 用户选某 installation → 前端 `POST /api/plugins/{id}/describe-launch { installationId, scope }` → `invoke(AgentRequest::DescribeLaunch)` → 插件返回 `DescribeLaunchResponse{program, args, cwd, envRefs, acpProtocolVersion, authMethodIds?}`。
4. （可选）`GET /api/plugins/{id}/configuration-summary`、`/skills`、`/mcp-servers` 展示。

### 8.2 首条对话

5. 用户在 chat composer 输入文本回车 → 前端先 `POST /api/acp/sessions { installationId, cwd, mcpServers:[] }`（创建 session，不流式）→ 后端 `AcpApi` → `AcpSessionManager`：
   - a. `AgentLaunchResolver.resolve(envRefs)` → `ResolvedLaunchValue`（`Plain`/`Secret`）。
   - b. `ora-process` spawn agent（`ProcessSpec{program, args, cwd, envs, environment_policy: ClearAndAllowlist, kill_on_drop: true}`，`windows_tree.rs` 建 Job B，`KILL_ON_JOB_CLOSE`，named pipes for stdio）。
   - c. ACP `initialize`（`InitializeRequest{protocol_version: DescribeLaunch.acpProtocolVersion, client_capabilities, client_info: Implementation{name:"ora",version}}`）→ agent 回 `InitializeResponse{protocol_version, agent_capabilities, auth_methods, agent_info}`；ora 宿主校验 `agent_capabilities` 含 baseline（`session/new`/`session/prompt`/`session/cancel`/`session/update`）。
   - d. ACP `authenticate`（若 `auth_methods` 非空；v1 自动选 `type:agent`，认证带外由 agent 处理，ora 宿主转发 `auth_methods` 到前端展示）。
   - e. ACP `session/new {cwd, mcp_servers}` → `session_id`；后端响应 `{ sessionId, agentInfo, agentCapabilities }` 给前端。
6. 前端 `POST /api/acp/sessions/{id}/prompt { prompt:[Text] }`（SSE 流）→ `AcpSessionManager.prompt` → ACP `session/prompt {session_id, prompt:[Text]}` → 开 SSE。agent 流式发 `session/update`（`SessionNotification{session_id, update: SessionUpdate}`）→ ora 宿主经 SSE 透传前端 → `chat store` 按 `SessionUpdate` 变体渲染。
7. agent 发 `PromptResponse{stop_reason}` → SSE 关 → ora 宿主 `AcpApi` 持久化 Ora Session（`agent_session_id = session_id`）→ 前端 `POST /api/sessions` 落库。

### 8.3 续发

8. 前端 `POST /api/acp/sessions/{id}/prompt { prompt:[Text] }` → `AcpSessionManager.prompt` → ACP `session/prompt` → SSE 流式 → `PromptResponse`。

### 8.4 取消

9. 前端 `POST /api/acp/sessions/{id}/cancel`（或 fetch abort）→ `AcpSessionManager.cancel` → ACP `session/cancel` notification（`CancelNotification{session_id}`）→ 等 `PromptResponse{stop_reason=Cancelled}`（deadline）→ 前端。

### 8.5 列历史（v1 后置）

10. v1 不实现 `session/list`/`load`/`resume`/`close`/`delete` 的前端；后端 `AcpSessionManager` 可预留方法。Ora Session reload 仅显示元数据。

---

## 9. 错误处理与生命周期

### 9.1 agent spawn 失败

- `LaunchGrantUnavailable`（env ref 缺/锁定）→ 结构化错误，前端提示用户设 grant。
- `TreeKillUnavailable`（OS 不支持 `PROC_THREAD_ATTRIBUTE_JOB_LIST`）→ fail-closed，拒绝启动（`windows_tree.rs:57` 注释，无 suspended fallback）。
- `SpawnFailed`（`program` 不存在/无权限）→ 提示用户检查 agent 安装。

### 9.2 ACP 握手失败

- `InitializeFailed`：agent 不支持 ora 要求的 `protocol_version` → 拒绝，提示换 agent/插件版本。
- `AuthFailed`：authenticate 失败 → 提示用户认证。
- `ProtocolMismatch`：agent `agent_capabilities` 不含 baseline → 拒绝。

### 9.3 session/prompt 超时/crash

- ora 宿主发 `$/cancel_request`（ACP 协议级，按 `request_id`，`notification.rs:61`）+ `session/cancel`（按 `session_id`，`notification.rs:39`）。
- agent 进程退出 → Job B 收 `JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO` → `AcpSessionManager` 失败 pending `session/prompt`。
- non-idempotent `session/prompt` 在无 `PromptResponse` 时标记 `UnknownOutcome`（对齐 design-v3 §13.1，不自动重放）。

### 9.4 cancel 语义鸿沟（方案 B 核心难点）

- ACP `session/cancel` 是 notification，ora 宿主发后等 `stop_reason=Cancelled`（`prompt.rs:87`，deadline）。
- 超时则 `TerminateJobObject`（Job B）+ 报 `CancellationUnconfirmed`。
- safety slot 从插件进程侧（design-v3 §13.1）移到 ora 宿主侧（`AcpSessionManager`），逻辑等价：ora 宿主对 pending `session/prompt` 锁存 `FatalSettlementCause`，复用 design-v3 §11.6 drain 矩阵。
- v1 不复用插件进程的 `cancelConversation` safety 机制（已废弃）；`AcpSessionManager` 自建等价 safety slot（容量 ≥ 并发 prompt 上限）。

### 9.5 agent crash 与 crash window

- Job B `JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO` + `direct_exit` watcher → `AcpSessionManager` 标 session failed。
- crash window：复用 design-v3 §11.6 滑动窗口（如 5 分钟 3 次）→ `CrashLoop`，`start`/`prompt` fail-closed，需 `reset_crash_loop` 或 disable→enable。作用在 `(plugin_id, installation_id)` 维度。
- 用户 disable/uninstall/shutdown 不计 crash。

### 9.6 shutdown

- ora 宿主 shutdown：`AcpSessionManager.shutdown_all` → 对每 session 发 `session/close`（若 agent 支持）或直接 `TerminateJobObject`（Job B）→ drain → `PluginRuntimeHub.stop_all`（Job A，已实现）→ release `ManagerLease`。
- `ExitRequested`（Tauri）触发 §10.4 shutdown；hard deadline 保持 lease 并退出进程。

### 9.7 背压

- ACP `session/update` 通知有界缓冲；满时 ora 宿主向 agent 发 `$/cancel_request`（按 request_id）+ `session/cancel`。
- `session/prompt` payload v1 限 `[Text]`（≤1 MiB，`AgentPrompt` 上限沿用）；`SessionUpdate` 单帧 ≤256 KiB（design-v3 §13.1 hard cap 沿用为 `AcpSessionManager` 缓冲上限）。

---

## 10. v1 保守范围与后置

| 能力 | v1 | 后置（v2） |
|---|---|---|
| prompt 类型 | `[Text]` 单块 | `Image`/`Audio`/`ResourceLink`/`Resource` 多模态 |
| session/update 变体 | `agentMessageChunk`/`agentThoughtChunk`/`toolCall`/`toolCallUpdate`/`usageUpdate`/`sessionInfoUpdate` | `plan`/`configOptionUpdate`/`availableCommandsUpdate`/`currentModeUpdate` |
| stop_reason | `EndTurn`/`MaxTokens`/`Cancelled` | `Refusal`/`MaxTurnRequests` |
| session 方法 | `new`/`prompt`/`cancel` | `load`/`resume`/`close`/`delete`/`list`/`set_mode`/`set_config_option` |
| permission | 自动 Allow + 日志 | `session/request_permission` 转前端交互（`PermissionOptionKind` AllowOnce/Always/Reject） |
| additional_directories | 不支持 | 支持 |
| 历史恢复 | Ora Session 元数据 only | `session/load`/`resume` 回放历史 |

### 10.1 v1 降级规则

- 收到 `SessionUpdate.Plan`：v1 降级为 `Status{phase:"plan"}` 或忽略 + 计数（前端不渲染 Plan）。
- 收到 `session/request_permission`：v1 自动回 `outcome: Selected{option_id: <first allow>}` + 日志 warn；后置 v2 转前端。
- 收到 `stop_reason=Refusal`：v1 映射为 ora 侧 `finishReason=Completed` + diagnostic 标记 `refusal`；后置 v2 扩展。
- 收到 `stop_reason=MaxTurnRequests`：v1 映射为 `Limit`。
- 收到多模态 `ContentBlock`（非 Text）在 prompt：v1 拒绝（前端不发送）；agent 若在 `session/update` 发多模态 chunk：v1 忽略 + 计数。

### 10.2 v2 演进（按 design-v3 §22.5 变更控制）

升级 `acpProtocolVersion`（ora 宿主↔agent）或 `pluginApi`（ora 宿主↔插件）或 agent `contractVersion` 之一 + 更新 DTO/golden/正反互操作测试 + ADR + 证明旧 runtime/新 agent 或新 runtime/旧 agent 在执行代码前 fail-closed。

---

## 11. 测试策略

### 11.1 单元

- `ora-plugin-protocol`：废弃 conversation DTO 的删除验证（negative test：旧 method 常量不存在）；`DescribeLaunch` DTO + golden fixture；`lifecycle.rs` 移除 `StreamParams` 的 fixture 刷新。
- `ora-acp-runtime`：ACP newline-delimited 帧解析（复用 `crates/contracts/src/acp/rpc.rs` 的 `JsonRpcMessage`）；`SessionUpdate` 11 变体反序列化；`StopReason` 映射；cancel deadline/`CancellationUnconfirmed` 矩阵；crash window。
- `literals.rs` drift check：ora 侧方法名表与 ACP `AgentMethodNames`/`ClientMethodNames` 对齐校验。

### 11.2 集成

- `crates/application` 装配 `PluginRuntimeHub` + `AcpSessionManager` + `AgentLaunchResolver`；mock agent（改造 `packages/mock-service/src/acp.ts` 为后端 in-process ACP server，模拟 `session/update` 流 + `PromptResponse`）。
- 插件进程 mock（`defineAgentPlugin` 实现 `discoverInstallations` 返回 fake 安装 + `describeLaunch` 返回 mock program）。
- 端到端：`enable` → `discover` → `describe-launch` → `POST /api/acp/sessions` → SSE 流 → `cancel` → 回收。

### 11.3 E2E（Windows）

- ora 宿主 spawn 真实 Bun 插件进程（`discoverInstallations` 探测本地 Claude Code 安装）+ 真实 agent（Claude Code ACP）→ `session/new`/`prompt` 流式 → `session/cancel` → 双 Job（A+B）回收。
- Job Object 回收验证：杀 Bun 不留 agent；杀 ora 宿主 Bun+agent 全回收（`KILL_ON_JOB_CLOSE`）。
- named pipe + 5 字节帧（插件侧）+ newline JSON-RPC（agent 侧）的 Windows 真实 pipe E2E。

### 11.4 v1 门禁

- 多模态 prompt 的 negative test（v1 拒绝）。
- `Plan`/`Permission`/`Refusal`/`MaxTurnRequests` 的降级 test（v1 映射正确 + 日志）。
- 旧 `startConversation`/`$/stream` 的 negative test（v1 协议层拒绝旧 method，fail-closed）。

---

## 12. 迁移与变更控制

### 12.1 当前 SDK 无兼容迁移（design-v3 §22.4）

- 删除 `ora-plugin-protocol` 的 conversation DTO + `$/stream`，一次完成，不留兼容层（`AGENTS.md` "No Backward Compatibility"）。
- `plugin-sdk`/`plugin-runtime` 移除 `startConversation`/`sendMessage`/`cancelConversation` 的 ABI 与 dispatch，增 `describeLaunch`。
- `packages/chat` 删 `createUnavailableAcpClient`，production 改后端中转。
- 旧 `conversations-store`/mock-data 已在 commit 2857064 删除，无需再动。

### 12.2 三个独立版本轴（design-v3 §0.13、§22.5）

- `wireVersion`（ora 宿主↔private bootstrap/runtime，锁定 Ora build）=1，不变。
- `pluginApi`（bootstrap↔插件 module，manifest `engines.pluginApi`=1）：若 `describeLaunch` 算 ABI 变化，升级 `pluginApi=2` 并冻结 typed namespace。**决策**：`describeLaunch` 是新增方法 + 废弃旧方法，属 `pluginApi` 升级（v1→v2），manifest `engines.pluginApi` 声明 2，旧 `pluginApi=1` 插件 fail-closed。
- agent `contractVersion`（manifest contribution `contractVersion`）：agent contract v1 已废弃 conversation，管理面 v1 + `DescribeLaunch` 仍为 `contractVersion=1`（管理面未变 conversation 语义，只是收敛）。**决策**：保持 `contractVersion=1`，因管理面 DTO 不变，只是移除对话面（对话面已不在 ora-plugin-protocol 表达）。
- `acpProtocolVersion`（ora 宿主↔agent）：由 `describeLaunch` 返回，ora 宿主 `initialize` 协商。v1 用 ACP 当前版本（`crates/contracts/src/acp/initialization.rs` 的 `ProtocolVersion`）。

### 12.3 单一规范链

design-v3 → `ora-plugin-protocol`（管理面 DTO + lifecycle）+ `ora-contracts/acp`（对话面 DTO）→ ts-rs 生成 TS → golden fixture → Rust/TS/E2E。

---

## 13. 风险与未决

1. **agent 由 ora 宿主 spawn（B1）vs 插件进程 spawn（B2）**：本设计选 B1。若未来需"插件进程对 agent 有更强控制"（如插件自定义 agent 启动参数、插件进程 own agent 进程树），升级为 B2（隧道 ACP），按 §12.2 升级 `pluginApi`。**未决**：用户若偏好 B2，请在审查时指出。
2. **ACP 真实 stdio 成帧**：ACP spec 不规定成帧，本设计选 newline-delimited JSON-RPC。若目标 agent（Claude Code/codex/opencode）实际用 `Content-Length`（LSP 风格）或其他，`AcpSessionManager` 成帧层需调整。**未决**：需在首个真实 agent E2E 时核实 Claude Code 的 ACP 成帧。
3. **`session/load` 历史回放语义**：ACP contracts 的 `LoadSessionResponse` 只含 `modes`/`config_options`，不含消息（`crates/contracts/src/acp/session.rs:200`）。推测 agent 在 load 后经 `session/update` 回放历史，但 spec 未显式声明。v1 后置 `load`，v2 实现时需查上游 ACP spec 确认。
4. **`session/cancel` vs `$/cancel_request` 易混**：前者取消整个 session 进行中操作（按 `session_id`），后者取消单个 JSON-RPC 请求（按 `request_id`）。`AcpSessionManager` 必须用对 DTO（`notification.rs:39` vs `notification.rs:61`）。
5. **ToolCall/Plan 是 complete-replace 语义**：agent 发更新时发全量 entries/content，`AcpSessionManager` 与前端不得按增量合并。
6. **记忆文件过期**：`ora-plugin-protocol-status.md` 的 M4/Job Object 记录已过期，本设计核实已实现。需更新该 memory（见 §14）。
7. **`crates/application/agent_definition` 与插件 agent 的关系**：`agent_definition` 是 ora 自己的"可配置 agent 类型"概念（`crates/domain/src/agent_definition.rs`），与插件 agent provider 是两套。**未决**：是否让插件 agent 成为 application agent 的一种实现，需后续设计。
8. **`SessionId` vs `Ora Session.agent_session_id`**：v1 `agent_session_id` 存 ACP `session_id`。Ora Session 删除时是否 `session/close` agent，v1 可不做（仅标记），v2 实现。

---

## 14. 记忆更新（本设计副产物）

`ora-plugin-protocol-status.md` 需更正：
- M4 runtime actor **已完整实现**（`crates/plugin-manager/src/runtime/`：hub/supervisor/session_actor/transport/invocation）。
- M3 Windows Job Object FFI **已完整实现**（`crates/process/src/windows_tree.rs`：CreateProcessW+Job Object+IOCP，fail-closed）。
- facade traits 已定义：`PluginManagement`/`PluginRuntimeControl`/`PluginRuntimeInvocation`（§15.1 `AgentPluginRuntime` 已 rename）/`AgentInvocationHandle`。
- `plugin-sdk` `defineAgentPlugin` + `plugin-runtime` bootstrap 已实现，旧 `getNums` 已删。
- 剩余缺口为"应用层未接线"（`crates/application` 不依赖 plugin-manager；`packages/chat` AcpClient 仅 mock；缺 ACP↔对话桥接、前端流式通道、UI 调用链）。

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

### ora-plugin-protocol（`crates/plugin-protocol/`）
- frame codec：`frame.rs:69,122`
- JSON-RPC profile：`json_rpc.rs:56,183`
- lifecycle：`lifecycle.rs:10,11,12,13,14,15,16`；`InitializeParams`：`lifecycle.rs:19`；`StreamParams`：`lifecycle.rs:295`
- agent contract：`agent/dto.rs:12,55,64,73,74,75,78,90,103,357,403,441,464`；`agent/method.rs:13,204,247`
- leaf types：`agent/leaf.rs:95,102,122,161,224,269`
- TS 导出：`lib.rs:58,133`

### ora-plugin-manager（`crates/plugin-manager/`）
- facade traits：`service.rs:51` / `ports.rs:91,145`
- `AgentInvocationHandle`：`runtime/invocation.rs:21,75,90,138`
- runtime：`runtime/hub.rs:132,217,272` / `runtime/supervisor.rs:19,42` / `runtime/session_actor.rs:64` / `runtime/transport.rs:7,187,313`

### ora-process（`crates/process/`）
- `ProcessSpec`/`EnvironmentPolicy`：`spec.rs:19,39,62`
- `TokioProcessSpawner` env_clear：`tokio_process.rs:37`
- Windows Job Object：`windows_tree.rs:54,122,373,491,539,546,576,679`

### 前端（`packages/`）
- chat AcpClient：`packages/chat/src/client.ts:12,19`
- chat store：`packages/chat/src/store.ts:22,139,200`
- mock ACP：`packages/mock-service/src/acp.ts:33,72,79`
- plugin-sdk ABI：`packages/plugin-sdk/src/agent/index.ts:111,123`
- plugin-runtime bootstrap：`packages/plugin-runtime/src/bootstrap/main.ts:4` / `session.ts:80` / `loader.ts:27` / `rpc/envelope.ts:66` / `transport/frame.ts:1`
- generated TS：`packages/plugin-runtime/src/generated/plugin-protocol.ts:149`

### 应用层
- `AppState`：`apps/web/server/src/app_state.rs:9`
- `AgentApi`：`apps/web/server/src/service/agent.rs:15`
- Session CRUD：`apps/web/server/src/handlers/sessions.rs:18` / `crates/application/src/session/handlers.rs:42` / `mapper.rs:6` / `ports.rs:6`
- routes：`apps/web/server/src/routes.rs:13`
- 会话创建 mutation：`packages/app-shell/src/state/hooks/use-workspace-mutations.ts:143`

### design-v3（`origin/codex/plugin-management-backend-v3:docs/plugin-management/design-v3.md`）
- §0 结论先行 / §3 不变量 / §4 架构 / §11 Runtime / §12 Wire Protocol / §13 Agent Contract / §14 安全 / §15 facade+应用集成 / §22 迁移控制
