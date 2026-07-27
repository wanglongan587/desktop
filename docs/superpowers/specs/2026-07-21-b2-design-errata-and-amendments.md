# B2 设计勘误与修订（Errata & Amendments）

> 针对 `docs/superpowers/specs/2026-07-20-plugin-acp-integration-design.md`（B2 形态，652 行）
> 日期：2026-07-21
> 来源：9-agent workflow 对抗验证（completeness critic + adversarial ACP-correctness）+ 一手代码复核 + ACP spec 调研（framing/ClientCapabilities/session-load 部分由 `2026-07-21-acp-transport-and-real-agent-behavior-research.md` 回填）
> 状态：勘误草案，待 review 后并入 B2 spec。本文件**不修改原 spec**，只列修订点。

## 0. 结论

对抗验证确认 **B2 架构核心成立**：两层协议正交、`crates/plugin-manager` 零改动、双 initialize 不冲突（plugin-protocol `$/initialize` 在 spawn 时 vs ACP `initialize` 在首次 `startConversation`，不同协议/管道/时机）、单 Job Object 覆盖 ③+④、`conversationId===ACP session_id`、happy-path 流关联正确（`session_actor.rs:774-795`）。

但发现 **1 blocker + 6 major + 6 minor gap**，全在**待建的 plugin-sdk ACP 桥那一层**（不在已实现的 plugin-manager）。本文件给出每条 gap 的修订决议。

标注约定：
- ✅ 本会话一手复核（file:line 已亲自读）
- 🟦 workflow 核实（agent 读码 cited file:line，可信但未本会话复核）
- ⏳ 待 research 回填（framing / ClientCapabilities / session-load）

---

## 1. ✅ [BLOCKER 已解] ACP stdio 成帧 —— newline-delimited 已由 spec 确认

**结论**：B2 spec "newline-delimited JSON-RPC over stdio" 选择**正确，不改**。权威 ACP spec（`agentclientprotocol/agent-client-protocol` `docs/protocol/v1/transports.mdx`，v2 相同）明确定义 stdio transport："Messages are delimited by newlines (`\n`), and MUST NOT contain embedded newlines"——**不是** LSP 风格 `Content-Length`。三方印证：参考 Rust runtime（`agentclientprotocol/rust-sdk` `stdio.rs` `BufReader::lines()` + `jsonrpc/transport_actor.rs` `parse_incoming_line`）+ Claude adapter（`zed-industries/claude-agent-acp` `src/index.ts`：stdout 专供 ACP，console 重定向 stderr）。"Claude Code 源自 LSP 可能用 Content-Length"的担忧**不成立**——ACP 是与 LSP 成帧的干净决裂。

**依据**：🟦 `crates/contracts/src/acp/rpc.rs:106-121`（`JsonRpcMessage` envelope，crate 不规定 transport——正确，transport 由 spec 规定）；✅ research file `2026-07-21-acp-transport-and-real-agent-behavior-research.md` §[BLOCKER 1]（spec 原文 + gh api 路径）。

**实现要点**（plugin-sdk ACP 工具，补 §7.D）：
1. `Bun.spawn` adapter（见 §15）为子进程，stdin/stdout pipe。
2. 写：每条 `JsonRpcMessage` 序列化为 **compact JSON**（不可 pretty-print，spec 禁 embedded newlines）+ `\n`。Ora 侧 `serde_json` 用 compact。
3. 读：stdout 按行读（`\n` 分隔），每非空行解析为一条 JSON-RPC message（v2 可能 batch array）。
4. stderr 只作日志，**绝不**当 ACP 解析。
5. 修正 B2 spec 内部矛盾：§13.1 与 data_flow 对齐为"成帧 = newline-delimited（spec 已定）"。

---

## 2. ✅🟦 [Major] Refusal/MaxTurnRequests 处理自相矛盾 —— 修订为"Status 事件 + Completed"，删除"flag"

**问题**：ADR-5 说 Refusal/MaxTurnRequests **deferred to v2**，但 §10.1 + `target_design(b)` 说 v1 映射 `Refusal→Completed+refusal flag`。代码核实：
- ✅ `crates/plugin-protocol/src/agent/dto.rs:467-471`：`enum AgentFinishReason { Completed, Cancelled, Limit }` —— 只有 3 变体，**无 Refusal/MaxTurnRequests**。
- ✅ `dto.rs:444-461`：`AgentTurnResult` 字段 `{conversation_id, turn_id?, finish_reason, usage?}` —— **无 refusal flag 字段**。
- ✅ `packages/plugin-runtime/src/generated/plugin-protocol.ts:145`：`AgentBusinessFailureKind` 11 变体，**无 refusal kind**（`agentUnavailable`/`agentProcessFailed`/`unsupportedAgentCapability` 等都不贴 refusal 语义）。

所以 spec §10.1"Refusal→Completed+`AgentBusinessErrorData.details` 标记 refusal"**不自洽**：一个 turn 不能既是成功 `AgentTurnResult::Turn`（finishReason=Completed）又携带 `AgentBusinessErrorData`（那是 `AgentInvocationResult::Error` 路径）。"flag" 在 v1 DTO 里无处安放。

**修订决议**（v1）：
- `stop_reason=Refusal` → 桥在流中 `yield AgentEvent.Status{phase:"refusal", message:<agent refusal 文案若可得>}`，然后 `return AgentTurnResult{finishReason: Completed, ...}`。
  - 理由：refusal 是 agent 的**合法 turn 结局**（它告诉你它不做 X），不是 failure，不应走 `AgentBusinessError`。`finishReason=Completed` 语义="agent 结束了它的 turn"——refusal 确实是 turn 结束的一种。refusal 信号经**流内 `Status` 事件**携带，**不需要新 DTO 字段**——这消解了"flag 无处放"的矛盾。
  - 前置条件：`packages/chat` store 必须渲染 `AgentEvent.Status` 的 phase（B2 §7.H 已要求 chat store 扩展消费 `status`，此处与之对齐）。
- `stop_reason=MaxTurnRequests` → `AgentTurnResult{finishReason: Limit}`（1:1，可接受，无矛盾）。
- v2：扩展 `AgentFinishReason::Refusal`（`contractVersion` 1→2 + DTO/golden/interop 测试 + fail-closed 证明，per §10.2）。

**要改的 spec 处**：§10.1 删"refusal flag"表述；`target_design(b)` 翻译表 Refusal 行改为"Status{phase:refusal}+Completed（v1）"；ADR-5 表述与之一致。

---

## 3. ✅ [Major] 非 agent 扩展"零改动"是假的 —— 修订 ADR-6 如实承认

**问题**：ADR-6 / `seam(g)` 称"非 agent kind 需 ZERO change to plugin-manager runtime，SHARE lifecycle+transport"。代码核实不符：
- ✅ `crates/plugin-manager/src/runtime/handshake.rs:39-44`：`if descriptor.kind != PluginKind::Agent { return Err(HandshakeFailed{IdentityMismatch}); }` —— **硬拒非 Agent kind**。
- ✅ `crates/plugin-protocol/src/manifest.rs:102-105`：`enum PluginKind { Agent, Workbench }` —— 只有 2 kind，无 config/IM。
- 🟦 `crates/plugin-manager/src/validation.rs:91-93`：`Workbench → RuntimeSupport::UnsupportedKind`；`crates/plugin-protocol/src/lifecycle.rs:44`：`self.plugin.kind != PluginKind::Agent` 拒绝。

所以"share lifecycle+transport"的 seam **今天不存在**；要加一个**可执行**的 config/IM kind 共享 5 字节帧 transport，**必须改** handshake.rs（kind 守卫）+ validation.rs（RuntimeSupport）+ lifecycle.rs（validate）+ manifest.rs（新 kind 变体）。

**修订决议**（ADR-6）：
- 如实承认：v1 只做 agent。未来加 config/IM kind 需要：
  1. `manifest.rs` `PluginManifest` 新增 kind 变体 + 对应 `*Contribution`；
  2. 放开 `handshake.rs:39` 的 kind 守卫（按 kind 分派，而非拒非 Agent）；
  3. `validation.rs` `RuntimeSupport` 扩展新 kind；
  4. 新 method 注册表（`config.*`/`im.*`，**不复用** `agent.*`，`method.rs:13-20` 的 AgentMethod 保持 closed）；
  5. 新 executor face（`ConfigProvider`/`ImProvider`，**不复用** `AgentProvider`）；
  6. 新 `contractVersion` 轴，独立于 agent `contractVersion=1`。
- 不变的部分：ACP 仍是 agent-plugin 实现细节，非 agent kind **不接触 ACP**——因为 ② 只见 ora-plugin-protocol `AgentEvent`。一个"经 IM 控制本地 agent"的 IM 插件是**独立 kind**（自己说 IM 协议，不说 ACP），经 app 编排一个 agent 插件，不需要自己说 ACP。
- **要改的 spec 处**：ADR-6 与 `seam(g)` 删除"零改动/SHARE transport"表述，替换为上述 6 步实清单；明确"v1 仅 agent，扩展性需 runtime/protocol 改动"。

---

## 4. 🟦 [Major] ④ crash/退出无 ③ 内检测 —— 桥契约补

**问题**：④（agent CLI）mid-turn crash/exit 而 ③（Bun）还活 → ② 只能靠 `$/stream` EOF 或下次 invoke 发现 → 对话挂到 deadline（`UnknownOutcome::DeadlineExceeded`）。CLI agent 常一 turn 后 `exit 0`，下次 `sendMessage` 才暴露。

**桥契约**（§7.D 补 "③ 内 ④ 健康契约"）：
- `createAcpAgentProvider` 必须监听 ④ 子进程 `exit`/`close` 事件 + stdout/stderr EOF。
- 任何 pending `startConversation`/`sendMessage` AsyncGenerator：检测到 ④ exit → 立即 `throw` → `AgentBusinessError{kind: agentProcessFailed, retryable:false, details:{agentExitCode, ...}}`（✅ `agentProcessFailed` 是 author-usable kind，见 `plugin-protocol.ts:145`）。
- 经 `$/stream` EOF 被 ② `session_actor` 收敛为 `AgentInvocationResult::Error`（UnknownOutcome，non-idempotent，不自动重放，per §9.3）。
- ④ 生命周期状态机（桥内）：`unspawned` → `alive+turn-in-flight` → `exited`（终态；下次 `startConversation` 重新 single-flight spawn 新 ④，见 §13 gap 13）。
- **要改的 spec 处**：§7.D 增"④ 健康检测"小节；§9.3 增"④ exit 透传到 ② 的路径"。

---

## 5. 🟦 [Major] ③↔④ stdio 背压死锁未验证 —— 桥契约补

**问题**：流式链 ④ stdout → 桥读 → yield `AgentEvent` → `$/stream` → ② `session_actor` → bounded `mpsc(INVOCATION_EVENT_CAPACITY=64)` → `AgentInvocationHandle.next_event()` → NDJSON → ①。① 慢读 / mpsc(64) 满 → session_actor 背压 `$/stream` → 桥停止读 ④ stdout → Windows 匿名 pipe buffer(~64KB)满 → ④ 阻塞写 → **死锁**。

**桥契约**（§7.D + §9.5 补）：
- 桥侧 ④ stdout 读取用**独立无界/大缓冲队列**（不阻塞 ④ 写）；溢出策略 = **drop-oldest**（仅丢弃中间 `TextDelta`，**保留** `ConversationStarted` 与 `PromptResponse` 边界）+ 计数 + `logger.warn`。
- 溢出阈值触发时，桥**主动**发 ACP `session/cancel`（不等 ora 背压回传）。
- ② 侧 `mpsc(64)` 有界保持（已有，`session_actor.rs`）。
- **E2E 必须**在 Windows 真实 named pipe + 慢消费者（① 不读）下验证不死锁（§11.3 增此 case）。
- **要改的 spec 处**：§9.5 背压段补"桥侧无界缓冲 + drop-oldest + 主动 cancel"；§11.3 增死锁 E2E。

---

## 6. 🟦 [Major] 重启后 stale session_id 无错误映射 —— 修订为"重载禁用 send"

**问题**：关 Ora → ④ 被 Job A 回收 → 重开 → Ora Session 的 `agent_session_id`（`crates/domain/src/session.rs:76`）是 stale ACP session_id → `sendMessage` → 新 ③/④ 收到 `session/prompt{stale session_id}` → ACP 返回 "session not found" 错误，**该错误不在 5 个 `stop_reason` 里**（`prompt.rs:69-88`），也**无映射到 ora outcome** → 挂起或不透明错误。且 v1 spec §8.5"reload 仅显示元数据"未显式**禁用 send**，故该失败路径可达。

**修订决议**（§7.H + §10）：
- v1：重载的 Ora Session **禁用 send**（状态=`ReadOnly`，UI 显示"会话已结束，无法续发，请新建"）。不尝试 `session/load`（v1 后置）。
- v2 fallback（若将来要支持续发）：映射 ACP "session not found" → `AgentBusinessError{kind: conversationNotFound, retryable:true}`（✅ `conversationNotFound` 是 author-usable kind，`plugin-protocol.ts:145`）→ 触发 `session/new` 重建会话；或用 ACP `session/load`+`resume`。
- **要改的 spec 处**：§7.H chat store 增"重载 session = ReadOnly"；§10 v1 行增"重载禁用 send"；§8.5 明确"send 在重载 session 上返回 `conversationNotFound` 错误而非尝试"。

---

## 7. 🟦 [Major] late/out-of-band session/update 无关联策略 —— 桥契约补"丢弃"

**问题**：`session/update` 在 `AgentTurnResult` 返回后、下次 `sendMessage` 前到达。若桥 buffer-replay：
- 陈旧 `ConversationStarted` → 🟦 `session_actor.rs:782-783`（`(SendMessage, ConversationStarted)` → `correlation_violation=true` → `begin_fatal(DrainTrigger::ProtocolFailure, ConnectionLost)`）→ **插件进程被杀**（fatal，不可恢复）；
- 陈旧 `TextDelta` → 🟦 `session_actor.rs:785`（`(SendMessage, non-ConversationStarted)` 允许）→ 上一轮 assistant 文本当新一轮输出渲染 → **静默数据错乱**。

**桥契约**（§7.D 补"late message 策略"）：
- 策略 = **无在飞 ora 请求时丢弃 late session/update**（🟦 `session_actor.rs:760` `pending.get_mut(id)` 返回 `None` 即无消费方，已暗示）。
- 桥维护"当前 in-flight ora request id"状态：`session/prompt` 返回 `PromptResponse` → 标记 turn 结束 → 后续 `session/update` 直到下次 `startConversation`/`sendMessage` 才消费，期间到达的一律 `drop + 计数 + logger.warn`。
- **绝不 buffer-replay**（否则触发上述 fatal / 错乱）。
- **要改的 spec 处**：§7.D 增"late session/update drop 策略"小节；§9 增"late message 与 session_actor correlation 的不变量"。

---

## 8. 🟦 [Minor] `SessionUpdate` 的 `UserMessageChunk`/`SessionInfoUpdate` 未定义 v1 处理

**问题**：🟦 `crates/contracts/src/acp/session.rs:557-580` `SessionUpdate` 共 11 变体，spec(b) 只映射 5、显式 drop 4，**`UserMessageChunk`（:559，用户消息回显）与 `SessionInfoUpdate`（:577）既未映射也未声明 drop**。

**修订**：v1 —— `UserMessageChunk` drop+计数（ora 已持有用户原 prompt，不需回显）；`SessionInfoUpdate` drop+计数（v2 视语义映射为会话元数据更新）。spec(b) 翻译表补这两行。

---

## 9. ✅ [Minor] `mcp_servers` 无 per-conversation 来源

**问题**：✅ `StartConversationRequest`（`dto.rs:81-87` / `plugin-protocol.ts:89`）字段 `{providerId, installationId, scope, clientRequestId, prompt}`，**无 mcp_servers**。ACP `session/new` 要 `mcp_servers`（🟦 `session.rs:46`）。spec §7.D `AcpAgentProviderOptions` 只列静态 `mcpServers?`，无 per-conversation 来源 → 项目级 `.mcp.json` 无法 per-conversation 到达 ④。

**修订**：v1 传**空** `mcp_servers`（或工具 `options.mcpServers` 静态值）；v2 经 `StartConversationRequest` 扩展字段（需 `contractVersion` 1→2）从项目 `.mcp.json` + Ora 配置传入。spec §7.D + §10 明确。

---

## 10. 🟦 [Minor] `disable`/`uninstall` 在 ④ 活跃 turn 期间 teardown 排序未定义

**问题**：`data_flow` step 6 teardown 说 `disable → $/deactivate → $/exit → Job A reaps ④`，未处理 ④ 有 in-flight `session/prompt` turn 的情况。若直接 deactivate，Job A mid-turn 回收 → `UnknownOutcome`。

**修订**（§9 补 teardown 排序）：`disable`/`uninstall`/`shutdown` 在 ④ 有活跃 turn 时：先 `session/cancel` + 等 `stop_reason=Cancelled`（deadline）→ 再 `$/deactivate` → `$/exit`；超时则 Job A 强回收（`UnknownOutcome`，non-idempotent，符合 §9.3）。桥的 `deactivate` handler 必须先 join ④ 的 cancel 再 exit。

---

## 11. 🟦 [Minor] `session/new` 失败路径未定义

**问题**：`target_design(b)` 说"on session/new response, immediately yield ConversationStarted{conversationId=session_id} → then session/prompt"，**假设 session/new 成功**。若 agent 对 `session/new` 返回 JSON-RPC error（unsupported mcp / invalid cwd / auth required but not done）→ 无 session_id → 不能 yield `ConversationStarted` → spec 未规定 throw。

**修订**（§7.D 补）：`session/new` 返回 error → 桥 `throw`（不 yield `ConversationStarted`）→ `AgentBusinessError{kind: agentUnavailable | unsupportedAgentCapability | authenticationRequired, retryable: <per kind>}`（✅ 均为 author-usable kind）。映射：protocol_version 不支持 → `unsupportedAgentCapability`；auth 需要但未做 → `authenticationRequired`；其余 → `agentUnavailable`。

---

## 12. 🟦 [Minor] 多 `startConversation`（新 ora 会话）agent 复用 vs 泄漏未定义

**问题**：spec 说"lazy spawn on first startConversation (single-flight)"。single-flight 去重**并发** StartConversation，但**顺序**的多次 StartConversation（用户结束会话 A，再开新会话 B，同一插件）未规定：是复用 ④ + 再 `session/new`（正确，④ 可托管多 ACP session），还是 spawn 新 ④（错误——旧 ④ 仍是 ③ 子进程，在 Job A 下存活到 ③ 退出 → agent 进程泄漏）。

**修订**（§7.D 补）：复用 ④ + ACP `session/new`（新 session_id = 新 conversationId）。**不** spawn 新 ④。④ 单进程托管多 ACP session。④ 已 `exited`（见 §4 gap 4）时才重 spawn。

---

## 13. 🟦 [Minor] 桥代码完全缺失 → 所有对话 claim 未运行验证

**问题**：✅ grep 核实 `packages/plugin-sdk` + `packages/plugin-runtime` **零 ACP 代码**；`createAcpAgentProvider`（§7.D）**不存在**。故 B2 spec 所有对话正确性 claim（data_flow step 3-5、翻译表、5 鸿沟处理、§4/§5/§7/§11/§12 本 errata 的桥契约）**尚未运行验证**——设计目前"内部自洽但运行未证"。

**修订**：实现后**必须** E2E（§11.3）覆盖：真实 Bun 插件进程 + 真实 agent（Claude Code ACP）→ `session/new`/`prompt` 流式 → `session/cancel` → Job A 回收。本 errata 所有桥契约（§4 crash、§5 背压、§7 late message、§11 session/new 失败、§12 复用）须有对应单测/集成测。

---

## 14. ✅🟦 [Major, research 回填] ClientCapabilities 广告 + fs/terminal handler

**问题**：B2 spec 的 `createAcpAgentProvider` 用 `ClientCapabilities::default()`（`crates/contracts/src/acp/initialization.rs:134-160`；✅ `FileSystemCapabilities` 默认 read/write=false，`terminal=false`）。所有 client 能力 spec-OPTIONAL（SHOULD，非 MUST；缺省=UNSUPPORTED）→ agent **降级**（非拒绝启动）。但 Ora 作为**拥有 editor + terminal 面板的 IDE**，广告 fs/terminal=false 会**致残** coding agent（Claude Code 无法直接改文件/跑终端，被迫走 `session/request_permission` 或文本 diff 降级路径）。

**依据**：✅ research file §[2]（spec `initialization.mdx` "Capabilities"：OPTIONAL/SHOULD/omitted=UNSUPPORTED）；🟦 Ora `initialization.rs:134-160`/`273-282` + `literals.rs:111-121` `CLIENT_METHOD_NAMES`。

**修订决议**（§7.D 补 + §7.G DTO）：
1. `createAcpAgentProvider` 发 ACP `initialize` 时广告：
   ```rust
   ClientCapabilities::new()
       .fs(FileSystemCapabilities::new().read_text_file(true).write_text_file(true))
       .terminal(true)
   ```
2. 桥**必须实现**对应 agent→client 请求的 handler（🟦 `literals.rs:111-121` `CLIENT_METHOD_NAMES`：`fs/read_text_file`、`fs/write_text_file`、`terminal/create`、`terminal/output`、`terminal/release`、`terminal/wait_for_exit`、`terminal/kill` + `session/request_permission`）：
   - `fs/read_text_file`/`fs/write_text_file`：proxy 到 Ora 宿主 editor/FS，**限定 `scope.workingDirectory`**（拒绝越界路径，防 agent 写工作区外）。
   - `terminal/*`：proxy 到 Ora terminal 面板（`crates/pty` 已有 pty 能力）。
   - `session/request_permission`：v1 auto-Allow+warn（per §10.1）；v2 转前端（需 ora 协议 Plugin→Host Request，升级 pluginApi）。
3. 广告的能力必须与 handler 一致——广告 fs=true 就必须应答 `fs/read_text_file` 请求，否则 agent 发的请求桥无 handler → `session/prompt` 永不返回 → 挂起（per 对抗验证）。
4. 若 v1 不实现 terminal handler，则诚实广告 `terminal=false`（降级），但 **fs 建议 v1 就做**（coding agent 改文件是核心能力）。

## 15. 🔴 [Major, research 回填] agent 启动 = ACP adapter 二进制，非 bare CLI

**问题**：B2 spec §5.1/§6.1/§7.D 说 "Bun.spawn agent (Claude Code/codex/opencode CLI)"，暗示 spawn 的是 agent CLI 且 CLI 自带 ACP。**与事实不符**：Claude Code 和 Codex **不以 ACP 原生**（无 `claude --acp` flag），它们经 **ACP adapter 二进制** 暴露 ACP：
- Claude Code → `claude-agent-acp`（npm `@agentclientprotocol/claude-agent-acp`，原 `zed-industries/claude-agent-acp`；`src/index.ts` stdout 专供 ACP + console→stderr；`src/acp-agent.ts` `initialize` 返回 `protocolVersion:1`，~行 1330）。
- Codex → `codex-acp`（npm `@agentclientprotocol/codex-acp` 优先，`@zed-industries/codex-acp` legacy；`OPENAI_API_KEY=... codex-acp`）。
- OpenCode：registry 列为 ACP-native，但**启动命令 research 未核实**（`sst/opencode` 的 `packages/protocol/src`/`packages/server/src` 未找到 ACP handler，README 未提 ACP）——需查 opencode 官方文档或实测 `opencode acp`。
- `ProtocolVersion = 1`（spec README + claude adapter `acp-agent.ts:~1330`）。

**依据**：✅ research file §[6]（含 `package.json`/`README`/`src/index.ts` 引用 + gh api 路径）。

**修订决议**（§5.1/§6.1/§7.D spawn 模型改）：
1. `createAcpAgentProvider` 的 `Bun.spawn` 目标是 **ACP adapter 二进制**（`claude-agent-acp`/`codex-acp`），**不是** `claude`/`codex` bare CLI。adapter 内部再 wrap 真实 CLI（claude adapter `--cli` passthrough）。
2. adapter 解析：按配置名从"已安装 adapter 注册表"解析路径（`node_modules/.bin/claude-agent-acp` 或 `npx @agentclientprotocol/claude-agent-acp`）。
3. `AcpAgentProviderOptions.spawn.program` = adapter 二进制名；env 带 adapter 所需 API key（Claude: `ANTHROPIC_API_KEY`；Codex: `OPENAI_API_KEY`/`CODEX_API_KEY`），从 `ExtensionContext.storagePath` + manifest grant（per §13.5）。
4. ACP `initialize` 发 `protocolVersion: 1`；agent 回 `1`（或其 latest，Ora 按 spec 版本协商规则接受或断开，`initialization.rs:71-73`）。
5. OpenCode 启动命令**标记 UNVERIFIED**，实现前必须核实。
6. 心智模型替换："agent = 带 `--acp` flag 的本地 CLI" → "agent = ACP server 子进程（常是 wrap 非-ACP agent 的 adapter）"。

## 16. ✅ [research 确认，无修订] 几项 B2 假设经 spec 核实

- **session/close vs session/ended**：spec（`docs/protocol/v1/session-setup.mdx` "Closing Active Sessions"）+ Ora `literals.rs`/`SessionUpdate` 均无 `session/ended`；唯一终态 session 方法 = `session/close`（opt-in `sessionCapabilities.close`）+ baseline `session/cancel` notification（仅取消 in-flight prompt，不关 session）。无修订。
- **session/load replay**：spec **明确规定了 replay**——agent 经 `session/update` notifications（在 response **之前**）回放整个对话，response body = `{"result": null}`（Ora `LoadSessionResponse` 的 `modes`/`config_options` 是 spec-compatible 超集）。**故 B2 open_risk #2"session/load 回放语义不明"已解**——不是 spec 缺失，是 v1 选择不实现（简化）。v2 **移除** `session/load`，replay 并入 `session/resume`+`replayFrom` cursor（`docs/protocol/v2/session-setup.mdx`）。Ora DTOs 是 v1-shaped（`ResumeSessionRequest` 无 `replayFrom`，`SessionUpdate` 只有 chunk 变体）——正确，因真实 agent 广告 v1。v2 迁移是未来工作。
- **baseline 方法**：spec 确认 baseline = `initialize`/`session/new`/`session/prompt`/`session/cancel`/`session/update`（`initialization.mdx` "Session Capabilities"；Ora `initialization.rs:487` doc-comment 一致），其余 opt-in。无修订。
- **StopReason**：5 变体 `end_turn`/`max_tokens`/`max_turn_requests`/`refusal`/`cancelled`（`prompt.rs`），与 spec 一致；`session/cancel` MUST 返回 `cancelled`（即使底层 op 抛错）。**强化 §2（Refusal 必须处理——它是真实 stop_reason）**。
- **v1 vs v2**：Ora DTOs v1-shaped，正确（所有已调研真实 agent 广告 v1）。v2 迁移见 `docs/protocol/v2/migration.mdx`。

## 17. 记忆更正（副产物）

记忆文件 `ora-plugin-protocol-status.md` 记"M3 Windows Job Object FFI 未实现、M4 runtime actor 未做"**已过期**。实际：✅🟦 `crates/process/src/windows_tree.rs`（`CreateProcessW`+`CreateJobObjectW`+`TerminateJobObject`+IOCP，`PROC_THREAD_ATTRIBUTE_JOB_LIST`，fail-closed）+ `crates/plugin-manager/src/runtime/{hub,supervisor,session_actor,transport,invocation}.rs`（`AgentInvocationHandle.next_event()`/`finish()` 流式）**均完整实现**。剩余缺口=应用层未接线（§7.F `crates/application` 装配 + §7.G 路由 + §7.H 前端）+ §7.D plugin-sdk ACP 桥。待更正该记忆文件。

---

## 附：本 errata 的依据置信度

| 依据类型 | 说明 |
|---|---|
| ✅ 本会话一手复核 | `dto.rs:444-471`、`manifest.rs:102-105`、`handshake.rs:39-44`、`plugin-protocol.ts:127/135/145/147`、`agent/index.ts:88-120`、`application/Cargo.toml`、grep acp=0 |
| 🟦 workflow 核实（agent 读码 cited） | `session_actor.rs:760/774-795/782-783`、`rpc.rs:106-121`、`literals.rs` CLIENT_METHOD_NAMES、`initialization.rs` ClientCapabilities、`session.rs:46/557-580`、`prompt.rs:69-88`、`validation.rs:91-93`、`lifecycle.rs:44`、`INVOCATION_EVENT_CAPACITY=64`、`windows_tree.rs` |
| ✅ research 回填 | framing（§1 ✅ 解）、ClientCapabilities + fs/terminal handler（§14 新 Major）、agent 启动=adapter 二进制（§15 新 Major）、session/load replay（§16 确认 spec 已规定）、session/close（§16 确认无 session/ended）、baseline 方法 + StopReason + v1/v2（§16 确认）——见 research file `2026-07-21-acp-transport-and-real-agent-behavior-research.md` |

> ✅ research agent #1（ACP transport + real agent behavior）已完成 2026-07-21，回填见 §1（framing 解）、§14（ClientCapabilities）、§15（adapter 启动）、§16（session/close/session-load/baseline/StopReason 确认）。research agent #2（Ora 现状底稿）仍在跑，回来后用其校准本文件 file:line 引用。届时整体并入 B2 spec。
