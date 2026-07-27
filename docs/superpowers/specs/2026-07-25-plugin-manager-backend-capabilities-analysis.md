# Ora 插件管理后端能力分析：扫描 / 验证 / 安装 / 执行

> 状态：一手代码审计（全部 cited file:line，本会话内可复现）
> 日期：2026-07-25
> 范围：`crates/plugin-protocol` + `crates/plugin-manager` + `packages/plugin-runtime` + `packages/plugin-sdk` + `crates/process`（后端五件套，**不含前端/应用装配层**）
> 问题：当前插件管理实现是否支持**插件扫描、验证、安装、执行**？这些中间实现能否支持和 **Agent 类型插件对话**？
> 结论速览：**四项后端能力全部已实现且经真实 e2e 验证；与 Agent 类型插件的流式对话端到端可用**（真实 Bun 进程 + Windows Job Object 之上跑通 `startConversation`→`ConversationStarted`→`TextDelta`→`AgentTurnResult`，含 `sendMessage` + cancel）。本文件不评估"前端是否接线"，那部分见 `2026-07-21-ora-plugin-current-state-vs-acp-compatibility.md` §3.4。

---

## 1. TL;DR / 能力判定

| 能力 | 判定 | 关键证据 |
|---|---|---|
| 扫描（scan_installed / scan_candidates） | ✅ DONE | `scanner.rs` 全量枚举 + 诊断投影；`service.rs:scan_installed`/`scan_candidates` |
| 验证（manifest + 兼容性 + 完整性 + 依赖审计） | ✅ DONE | `validation.rs:PackageValidator::validate`；`manifest.rs:parse_plugin_manifest`+`validate_manifest_invariants`；deno_ast 依赖 AST 审计 |
| 安装（digest-bound journal + 原子 rename） | ✅ DONE | `install/pipeline.rs:PluginInstaller::install_authorized_candidate`；staging→receipt→final→state commit |
| 执行（lifecycle + 流式对话 + cancel + 树清理） | ✅ DONE | `runtime/{hub,supervisor,session_actor,handshake,transport,invocation}.rs`；`ports.rs:PluginRuntimeInvocation` |
| 与 Agent 类型插件对话 | ✅ DONE（引擎层 e2e 证明） | `plugin_library_e2e.rs`：真实 Bun + Job 跑通 startConversation 流 + sendMessage + cancel |

**唯一不在本文件范围、但影响"真实 agent 对话"的缺口**：`packages/plugin-sdk/src/acp/`（`createAcpAgentProvider` 桥）**是未提交的工作区草稿**（`git ls-files` 追踪数=0），它负责把 ora-plugin-protocol 翻译成 ACP 以驱动真实 agent CLI（Claude Code/codex/opencode）。引擎本身不依赖它——见 §7。

---

## 2. 五件套定位与分层

```
② Ora 后端进程                      ③ 插件进程（Bun）           ④ 本地 agent CLI
┌─────────────────────────────┐   ┌──────────────────────────┐   ┌──────────────┐
│ ora-plugin-protocol  (线协议) │   │ plugin-runtime (TS bootstrap)│   │ Claude Code  │
│   帧 / JSON-RPC / lifecycle  │◄──┤  5B帧 / $/stream dispatch │   │ codex        │
│   / 8 agent.* 方法 / AgentEvent│   │  → 作者 AsyncGenerator    │   │ opencode ... │
│ ora-plugin-manager (生命周期)  │   │ plugin-sdk (作者 ABI)      │   │  (ACP)       │
│   管理面 + runtime actor      │   │   defineAgentPlugin       │   └──────▲──────┘
│   / handshake / stream 收敛    │   │   AgentProvider 接口      │          │ ACP
│ ora-process (Windows Job)     │   │ plugin-sdk/acp (桥,WIP) ───┼──────────┘
└─────────────────────────────┘   └──────────────────────────┘
```

- **②↔③** 是 ora-plugin-protocol（私有线协议，5B 大端帧 + 严格 JSON-RPC + lifecycle + 8 方法 + `$/stream`）。本文件审计的"执行"即这条线。
- **③↔④** 是 ACP（开放协议）。`plugin-sdk/acp/` 桥负责在 ③ 内把它翻成 ora 的 `AgentEvent` 流。**② 不接触 ACP**——`Grep -i acp` 在 `crates/plugin-manager/src`、`crates/plugin-protocol/src` 均 0 匹配（`2026-07-21-...compatibility.md` §4 一致）。

---

## 3. 扫描（Scan）

### 3.1 入口与枚举

`PluginManagement::scan_installed`（`crates/plugin-manager/src/service.rs:scan_installed`）→ `scan_and_reconcile`（`service.rs`）→ `InstalledScanner::scan_installed`（`crates/plugin-manager/src/scanner.rs`）。

`InstalledScanner::scan_installed`（`scanner.rs`）在 `spawn_blocking` worker 里调 `scan_installed_blocking`（`scanner.rs`）：

- `std::fs::read_dir(plugins_dir)` 枚举 final 目录，跳过 `.staging`/`.trash`（`scanner.rs`）。
- 目录名必须是合法 `PluginId`（`PluginId::parse(name)`），否则记 `InvalidManifest` 诊断（`scanner.rs`）。
- 对每个目录读 `package.json`（`read_manifest_for_catalog`，`scanner.rs`，用 `parse_plugin_manifest`）+ `.ora/receipt.json`（`read_receipt`，`scanner.rs`）。
- 交 `PackageValidator::validate(..., ValidationTarget::Installed { receipt, record })` 做完整证明（见 §4）。
- **state 与磁盘对账**：state 里有但磁盘没有的 → `MissingInstallFiles`；磁盘有但 state 没记录 → `UntrackedInstall`（仍可见于 catalog，但永不进 validated admission）。

### 3.2 catalog 投影

扫描产出 `InstalledScan { catalog, validated, state }`（`scanner.rs`）：
- `catalog: PluginCatalogSnapshot { revision, entries }` —— 公开 DTO，每个 `CatalogEntry` 带 `validity`/`compatibility`/`support`/`integrity`/`diagnostics`。
- `validated: BTreeMap<PluginId, ValidatedPackage>` —— 仅含通过完整证明的包，**只有它进 runtime admission**（`service.rs:admitted_descriptor` 从 `scan.validated` 取）。
- `revision` 在 entries 变化时单调递增（`scanner.rs`），供 registry reconcile 判定新鲜度。

另有 `scan_candidates`（`service.rs:scan_candidates`）：枚举 `discovery_roots` 下的子目录，经 `CandidateAuthority` 铸造 session 绑定的 `CandidateSelection`（不复制、不安装，仅登记可信路径）。

### 3.3 测试

`scanner.rs` 内联测试 `reports_untracked_final_directory`：孤儿 final 目录作为 untracked 可见、不进 validated、重复扫描 catalog 稳定。`management_e2e.rs:scan_install_enable_disable_uninstall_and_restart_without_spawn` 覆盖 scan→install→catalog 两条目。

---

## 4. 验证（Validate）

### 4.1 统一证明入口

`PackageValidator::validate(root, target)`（`crates/plugin-manager/src/validation.rs`）对 candidate/staging/installed/recovery 四种 `ValidationTarget`（`validation.rs`）跑**同一套证明**：

1. `compute_tree_digest(root, limits, tree_mode)` —— 递归算 SHA-256 树摘要 + 文件数 + 总字节（`safe_fs.rs`/`install/digest.rs`）。
2. `parse_plugin_manifest(manifest_bytes)`（`crates/plugin-protocol/src/manifest.rs`）—— 严格 JSON + schema + invariants。
3. `validate_entry_and_artifact`（`validation.rs`）—— Agent 包布局审计。
4. `runtime_compatibility`（`validation.rs`）—— 引擎兼容性。
5. `validate_target_facts`（`validation.rs`）—— installed/staging 的身份与摘要对账。

### 4.2 manifest 严格解析

`parse_plugin_manifest`（`crates/plugin-protocol/src/manifest.rs`）：
- 字节预算 `MAX_MANIFEST_BYTES = 256 KiB`（`manifest.rs:MAX_MANIFEST_BYTES`），JSON 深度 `MAX_MANIFEST_JSON_DEPTH = 64`（`manifest.rs`），经 `parse_strict_json`（`strict_json.rs`）—— **拒重复键、拒超深、拒显式 null**。
- `ora.manifestVersion` 必须 = 1，否则 `UnsupportedManifestVersion`（`manifest.rs`）。
- `#[serde(deny_unknown_fields)]` 在 `PluginManifest`/`AgentEngines`/`AgentContributions`/`AgentContribution` 上——**非法字段不可表示**（`manifest.rs`）。
- `validate_manifest_invariants`（`manifest.rs`）：
  - Agent 必须 `type=module`、`engines.pluginApi=1`、`contributes.agents` 1..=64、每个 contribution `contractVersion=1`、id 唯一、displayName 1..=128 标量值。
  - Workbench 只需 `ora` 引擎匹配 + `contributes.workbench.schemaVersion=1`。

### 4.3 Agent 包布局与依赖审计（`validate_entry_and_artifact`，`validation.rs`）

这是 ora 区别于普通 npm 包的关键防线：
- `main` 必须是 `dist/index.js`（`validation.rs`），且必须是**常规文件**（拒 symlink）。
- **allowlist**：只允许 `package.json` + `main` + 根级 `README*`/`LICENSE*`（`validation.rs`）；其余路径一律 `ArtifactLayout` 拒绝。
- **禁物**：任何 `node_modules`、`.node` 原生模块（`validation.rs`）。
- **依赖 AST 审计**（`validate_materialized_javascript`，`validation.rs`）：用 `deno_ast` 解析 `dist/index.js` 的 import/export/动态 import/require，**只允许 `node:` / `bun` / `bun:` 前缀的内置 specifier**——任何外部依赖边 `ArtifactLayout` 拒绝。这强制插件必须**预打包**（materialized），运行时不装 npm。

### 4.4 兼容性与完整性

- `runtime_compatibility`（`validation.rs`）：`engines.ora`（SemVer range）匹配 host、`plugin_api==1`、`engines.bun` 匹配；Workbench 只查 `ora`。`EngineRange`（`manifest.rs`）支持空格分隔 comparator。
- `validate_target_facts`（`validation.rs`）：
  - `Installed { receipt, record }`：目录名==plugin_id、且 `managed_facts_match`（package/receipt/state 三方 digest+file_count+total_bytes+operation_id 一致）→ `Verified`；否则 `InstalledFactsMismatch`。
  - `Staging { reviewed_id, reviewed_version, reviewed_digest }`：三者与 reviewed 一致 → `NotApplicable`；否则 `SourceChanged`。

### 4.5 测试

`validation.rs:validates_agent_candidate` 覆盖合法 Agent candidate + 引入未解析 import 即被拒。`manifest.rs` 内联测试覆盖 unknown version / deny_unknown_fields / 重复 contribution id。

---

## 5. 安装（Install）

### 5.1 流程

`PluginManagement::install_authorized_candidate`（`service.rs`）→ `PluginInstaller::install_authorized_candidate`（`crates/plugin-manager/src/install/pipeline.rs`）。

`install/pipeline.rs` 的 journal：
1. `lease.assert_held()` + 取 per-plugin mutation gate（`pipeline.rs`，串行化同 id 的 install/enable/disable/uninstall）。
2. final 路径已存在 → `AlreadyInstalled`（`pipeline.rs`）。
3. `current_source_identity(source_root)` vs candidate 的 `source_identity` → 防 root 被换（`pipeline.rs`）。
4. **第一次** `validate(source_root, Candidate)` + `require_authorized_identity`（`pipeline.rs`，核 id/version/digest）。
5. `copy_fresh_package_files`（`pipeline.rs`）：**只复制 reviewed 正则文件流**，create-new 打开、`sync_all` 落盘、**前后 `audit_no_named_streams`**（防 ADS/命名流注入，`safe_fs.rs`）；Windows 用 `FILE_FLAG_OPEN_REPARSE_POINT` no-follow（`pipeline.rs:open_source_no_follow`）。
6. **第二次** source identity + validate（防 copy 期间被改）+ staging validate（`Staging` target，核 reviewed 三元组）。
7. `build_receipt`（`pipeline.rs`）→ `write_receipt`（写 `.ora/receipt.json`，create_new + sync_all）。
8. **post-receipt digest**：对 staging 再算 `InstalledContent` 树摘要，若 != `candidate.content_digest` → `SourceChanged`（`pipeline.rs`）。
9. state journal：`AddPending(Install{phase:Prepared})` → 原子 `rename(staging→final)` → `ReplacePending(FilesCommitted)` → `CompleteInstall`（`pipeline.rs`）。任一阶段失败映射成 `RecoveryRequired{operation_id}`（`pipeline.rs:recovery_error`），InstallReconciler 在下次 bootstrap 自愈（`install/reconcile.rs`）。
10. **总是留 disabled**：`CompleteInstall` 写 `InstalledRecord` 但不设 `UserEnablement::Enabled`（`pipeline.rs` test 断言 `UserEnablement::Disabled`）。

### 5.2 启动恢复

`PluginManagementService::bootstrap_with_lease`（`service.rs`）在构造期就跑 `InstallReconciler::reconcile()`（`service.rs`）——处理中断在 staging/未完成 rename 的 pending install，清 staging、补/撤 state，使重启后 catalog 与 state 自洽。`scan_and_reconcile`（`service.rs`）每次管理操作后重算 registry。

### 5.3 测试

`pipeline.rs:installs_authorized_candidate_disabled` 跑完整 reviewed-source→journaled installed+disabled。`management_e2e.rs:source_change_after_identify_fails_closed_and_consumes_candidate` 证明 identify 后改源 → `ContentDigestMismatch` + candidate 句柄被消费 + 无 final 字节落地。

---

## 6. 执行（Execute）—— 与 Agent 插件对话的引擎

### 6.1 公开面（facade）

`ports.rs` 定义三个 trait：
- `PluginRuntimeControl`（`ports.rs`）：`open_admission`/`close_admission`/`stop_and_reap`/`reset_crash_loop` —— 管理面用来开关 admission、停树、清 crash-loop。
- `PluginRuntimeInvocation`（`ports.rs`）：`start`/`stop`/`invoke(plugin_id, AgentRequest) -> AgentInvocationHandle`/`shutdown_all` —— 对话调用面。
- `RuntimeAdmissionProvider`（`ports.rs`）：`admit(plugin_id) -> ValidatedLaunchDescriptor` + `recheck_after_activate` —— 运行时从管理面取"可启动证明"。

`ValidatedLaunchDescriptor`（`ports.rs`）字段含 `kind: PluginKind`、`entry_path`、`storage_path`、`declared_agents`、`enablement_epoch`、`registry_revision`、`launch_grant`。

### 6.2 多插件 hub

`PluginRuntimeHub`（`crates/plugin-manager/src/runtime/hub.rs`）：
- 同时实现 `PluginRuntimeInvocation` + `PluginRuntimeControl`（`hub.rs`）。
- 每个 `plugin_id` 一个**惰性 single-flight supervisor**（`hub.rs:runtime`：`state.runtimes.get` 或 `spawn_agent_plugin_runtime`）。
- `invoke`（`hub.rs`）：先 `closed_admission` 检查，再 `admission.admit` 取 descriptor（失败则 `close_admission`+`stop_and_reap` 并返回错误），再 `runtime(plugin_id).invoke(request)`（`hub.rs`）。
- `bind(admission, events)`（`hub.rs`）打破管理↔runtime 构造环：先 `PluginRuntimeHub::new`，再 `PluginManagementService::bootstrap_with_lease`，再 `runtime.bind(management, management.runtime_event_sink())`（见 e2e §6.7）。

### 6.3 生命周期握手（`$/initialize` + `$/activate`）

`runtime/handshake.rs:perform_handshake`：
- **kind 守卫**：`descriptor.kind != PluginKind::Agent` → `HandshakeFailed{IdentityMismatch}`（`handshake.rs`）。即 v1 只跑 Agent 插件。
- 构造 `InitializeParams{wire_version:1, host_version, runtime_version, session_id, plugin, paths, declared_agents, limits}`（`handshake.rs`），调 `initialize.validate()`（`lifecycle.rs:35` 校验 wire_version==1/kind==Agent/plugin_api==1）。
- `lifecycle_round_trip(METHOD_INITIALIZE, deadline)`（`handshake.rs`）：写帧→等 writer ack→等 reader Response，严格"response-after-writer-ack"因果序（`handshake.rs`）。
- 校验回包 `wire_version`/`runtime_version`/`session_id`/`plugin.id`/`plugin.version` 全等（`handshake.rs`）。
- `lifecycle_round_trip(METHOD_ACTIVATE, ActivateParams{reason})` → 校验 `ActivateResult` 的 providers 与 `declared_agents` 匹配（`handshake.rs`）。
- **post-activate admission barrier**：`admission.recheck_after_activate(descriptor)`（`handshake.rs`）——核 `enablement_epoch`/`registry_revision`/`content_digest`/`content_owner` 未变，否则 `AdmissionChanged`（防 activate 期间被 disable/卸载）。

### 6.4 对话执行：generation actor

`runtime/session_actor.rs:spawn_generation_actor` 持有 `GenerationTransport`（writer/reader/process 事件流）+ `HandshakeProof`。`run()`（`session_actor.rs`）select：commands / cancel / writer_events / reader_events / process_events。

**接受一次 invoke**（`accept_invocation`，`session_actor.rs`）：
1. `allocate_request_id`（`session_actor.rs`：`HostRequestId::from_sequence`，单调递增）。
2. admission/primary_trigger/已过 deadline/pending 容量/`validate_with_limits` 一系列 fail-closed 检查（`session_actor.rs`）。
3. `request.to_params_value()` + `encode_json_rpc_request` 构造 payload（`session_actor.rs`）。
4. 建 `AgentInvocationHandle`（`invocation.rs:21`：`events: mpsc::Receiver<AgentEvent>` + `completion: oneshot::Receiver<Result<AgentInvocationResult,PluginError>>` + `cancel: mpsc::Sender<String>`）。
5. `spawn_enqueue(WriterLane::Ordinary)` 写出请求帧，并排一个 `InvocationDeadline` 命令（`session_actor.rs`）。

**流式事件**（`handle_notification` → `process_stream`，`session_actor.rs`）：
- 必须是 `$/stream` notification，否则 `DirectionViolation`（`session_actor.rs`）。
- 解析 `StreamParams{id, seq, value: AgentEvent}`，`value.validate_with_limits`（`session_actor.rs`）。
- **seq 单调**：`stream.seq.get() == pending.model.next_stream_sequence`，否则 `InvalidStreamSequence`（`session_actor.rs`）。
- **correlation 规则**（`session_actor.rs:process_stream`）：
  - `StartConversation` + `ConversationStarted`（且 pending 无 conversation_id）→ 绑定 conversation_id，合法。
  - `StartConversation` 已有 conversation_id 后再来 `ConversationStarted`，或 `SendMessage` 来 `ConversationStarted` → **correlation_violation**（fatal）。
  - `StartConversation` 在拿到 conversation_id 前的其它事件 → violation。
  - `SendMessage` 的其它事件 → 合法。
  - 非 streaming 方法收到任何 stream → violation。
- 通过则 `pending.events.try_send(stream.value)` 转发给 handle 的 `next_event()`；满则 `Backpressure` intent（`session_actor.rs`）。

**终态**（`handle_reader_event` → `process_terminal`，`session_actor.rs`）：
- `parse_agent_terminal`（`invocation.rs:parse_agent_terminal`）按 `request.method()` 的闭枚举 `AgentMethod`（8 变体）反序列化：`StartConversation`/`SendMessage` → `AgentInvocationResult::Turn(AgentTurnResult)`，其余 → `AgentInvocationResult::Response(AgentResponse)`（`invocation.rs:parse_agent_success`）。
- **turn correlation**：`AgentInvocationResult::Turn(turn)` 时 `pending.conversation_id != Some(&turn.conversation_id)` → `AgentContractViolation{ConversationCorrelation}`（`session_actor.rs:process_terminal`）。
- 调 `response.validate_for_request(request, limits)` / `result.validate_with_limits(limits)` 做终态校验（`invocation.rs`）。
- `completion.send(result)` 交给 `handle.finish()`（`invocation.rs:finish`）。
- 错误码：`ERROR_REQUEST_CANCELLED`→`PluginError::Cancelled`；`ERROR_SERVER_BUSY`→`PluginBusy`；`ERROR_AGENT_BUSINESS`→`AgentBusinessFailure`（带 `AgentBusinessErrorData`，11 种 `kind`）（`invocation.rs:parse_agent_error`）。

**取消**：`AgentInvocationHandle::cancel()`（`invocation.rs`）/ `cancellation().cancel()`（`invocation.rs`）→ 发 `cancel_tx` → actor `handle_intent(ExplicitCancel)`（`session_actor.rs`）→ 按 `write_certainty`（NotWritten/Written/PossiblyWritten）分支：NotWritten 直接 `complete_with_intent`；Written/PossiblyWritten 发 `$/cancelRequest` notification + `CancelCap` 超时兜底（`session_actor.rs:send_transport_cancel`/`schedule_cancel_cap`）。安全方法（`cancelConversation`）的 ExplicitCancel 被忽略（`session_actor.rs:handle_intent`）。

**停止协议**（`advance_stop`/`begin_exit`，`session_actor.rs`）：
`Stop{reason}` → `WaitingInvocations`（等 pending 清空，逐个 `HostStop` intent）→ `Deactivating`（发 `$/deactivate`，等 Response）→ `Exiting`（发 `$/exit` notification）→ `WaitingForDrain` → 等 stdout/stderr/direct/tree 四者 done → `finish_generation`。任一 fatal（stdout EOF/process exit/protocol）→ `begin_fatal`：`controller.terminate_tree()` + drain 超时 + 所有 pending 用 `settle_fatal_invocation`/`settle_termination_intent` 收敛（`session_actor.rs`）。

### 6.5 进程 spawn（真实 Bun + Windows Job）

`PluginRuntimeAssets::process_spec`（`crates/plugin-manager/src/runtime/generation.rs`）构造 `ProcessSpec`：
- 程序 = `bun_executable`（pinned，不走 PATH）；argv = `--config=<empty-bunfig> --no-env-file run --no-install <bootstrap_entry>`（`generation.rs`，测试 `process_spec_uses_pinned_assets_and_empty_config` 断言）。
- `clear_and_allowlist_environment()`（`generation.rs`）—— `TokioProcessSpawner` 在 `EnvironmentPolicy::ClearAndAllowlist` 时 `command.env_clear()`（`crates/process/src/tokio_process.rs`）。
- launch grant 的环境变量经 `LaunchValueResolver` 解析（`generation.rs`），`ResolvedLaunchValue::Secret` 经 `expose_for_process()` 仅在 spawn 边界暴露。
- `ProcessTreeGenerationLauncher::launch`（`generation.rs`）：`spawn_blocking(spawner.spawn_tree(spec))` → `into_parts()` → `spawn_writer`(stdin)/`spawn_reader`(stdout)/`spawn_stderr_drain`(stderr) + direct_exit/tree_empty watcher（`generation.rs`）。
- 真实实现 `WindowsJobProcessTreeSpawner`（`crates/process/src/windows_tree.rs`）：`CreateJobObjectW` + `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` + `CreateProcessW` 带 `PROC_THREAD_ATTRIBUTE_JOB_LIST`（把 ③ 及其子孙 ④ 全纳入单 Job A，③ 崩/退时整树被 reap）。

### 6.6 插件侧（TS bootstrap）—— 线的另一端

`packages/plugin-runtime/src/bootstrap/session.ts:BootstrapSession`：
- `#readLoop`（`session.ts`）逐帧 `parseInboundEnvelope`，首帧必须 `$/initialize`（`session.ts:#initializeSession`），校验 `runtimeVersion` 回包；`$/activate`（`session.ts:#activateSession`）经 `loadAndActivatePlugin` 加载作者 `defineAgentPlugin` 模块，回 providers。
- `#acceptAgentRequest`（`session.ts`）：`validateAgentRequest` + provider 存在性 + 容量（`maxPendingRequests`/`maxActiveTurns`）+ safety 预留；对 `startConversation`/`sendMessage` 建 `ActiveTurn`（`#turnsByRequest`/`#turnsByConversation`）。
- `#driveGenerator`（`session.ts`）：手动 `next.call(returned)` 驱动作者的 `AsyncGenerator<AgentEvent, AgentTurnResult>`：
  - `step.done` → `validateTurnResult`（核 `conversationId` 与已绑一致）→ `encodeSuccess(id, result)` 终态响应。
  - 否则 `validateAgentEvent` + correlation（`startConversation` 首事件必须 `conversationStarted`；`sendMessage` 不能产 `conversationStarted`；`session.ts:#driveGenerator`）→ `encodeStream(id, sequence, event)` 发 `$/stream` notification，seq 单调。
- `#runCancelConversation`（`session.ts`）：safety lane，一次执行 provider cancel + 验证目标 turn 终态 disposition 一致（`accepted`→`cancelled`；`alreadyStopped`→`completed`/`limit`），保证 cancel 响应在目标终态帧之前。
- `ProtocolWriter`（`transport/writer.ts`）按 lane（control/ordinary/safety）背压写帧；`FrameDecoder`（`transport/frame.ts`）镜像 Rust 5B 大端头。

`packages/plugin-sdk/src/agent/index.ts`：作者 ABI。`AgentProvider` 接口（`agent/index.ts`）定义 `startConversation`/`sendMessage` 返回 `AsyncGenerator<AgentEvent, AgentTurnResult>`，`cancelConversation` 返回 `Promise<CancelConversationResponse>`，外加 5 个普通方法。`defineAgentPlugin`（`agent/index.ts`）是零成本透传定义器。

### 6.7 端到端测试：真实对话证明

`crates/plugin-manager/tests/plugin_library_e2e.rs:complete_management_and_runtime_lifecycle_survives_restart`（`#[ignore]`，跑前需 `task prepare-plugin-runtime`）：

装配（真实）：
```
PluginRuntimeHub::new(config, prepare_runtime_assets(&config).await,
                      ProcessTreeGenerationLauncher::new(WindowsJobProcessTreeSpawner::new()),
                      UnavailableLaunchValueResolver)
PluginManagementService::bootstrap_with_lease(...)   // 真实管理 + InstallReconciler
runtime.bind(Arc::clone(&management), Arc::new(management.runtime_event_sink()))
```
`support/mod.rs:pack_agent_fixture`：用 **真实 SDK pack CLI**（`packages/plugin-sdk/dist/pack/cli.js` + pinned Bun）把作者源 `support/mod.rs:agent_source` 打成 materialized artifact。该源用 `defineAgentPlugin`，`startConversation` 产 `conversationStarted`+`textDelta("hello")`+`AgentTurnResult{completed}`，`sendMessage` 产 `textDelta("pending")` 后等 abort。

断言（真实 Bun + Job）：
1. 未 enable 时 `invoke(DiscoverInstallations)` → `Err(Disabled{User})`（`plugin_library_e2e.rs`）。
2. `enable(agent)` 后并发两个 `invoke(DiscoverInstallations)` → 各 `finish()` 得 `AgentInvocationResult::Response(DiscoverInstallations{installations:[], diagnostics:[NotFound]})`（`plugin_library_e2e.rs`）—— 证明并发 + 普通方法。
3. **`invoke(StartConversation{prompt:"hello"})` → `next_event()`=`ConversationStarted{conversation_id}` → `next_event()`=`TextDelta{channel:Assistant, text:"hello"}` → `finish()`=`AgentInvocationResult::Turn(AgentTurnResult{conversation_id, turn_id:"turn", finish_reason:Completed})`**（`plugin_library_e2e.rs`）—— **流式对话端到端通**。
4. **`invoke(SendMessage{prompt:"cancel me"})` → `next_event()`=`TextDelta("pending")` → `cancel()` → `finish()`=`Err(Cancelled{plugin_id, request_id})`**（`plugin_library_e2e.rs`）—— **流中取消通**。
5. **tamper**：覆写 `dist/index.js` 后 `invoke` → `Err(Disabled{IntegrityMismatch})`（`plugin_library_e2e.rs`）—— post-launch 完整性复检生效。
6. `disable`→`invoke`=`BackendShuttingDown`；`uninstall`(agent+workbench)；`shutdown_all`；重启 bootstrap catalog 空（`plugin_library_e2e.rs`）。

> 该测试是本分析的**决定性证据**：scan/validate/install/execute 四项与 Agent 对话能力在真实进程模型上端到端验证通过。`#[ignore]` 仅因需 pinned Bun 资产（`task prepare-plugin-runtime`），非功能缺口。

另：`management_e2e.rs` 用 `RecordingRuntimeControl`（fake，不 spawn）证明管理生命周期 fail-closed + crash-loop + grant revocation 跨重启。`runtime_windows_e2e.rs`（`#[ignore]`）覆盖 `ora-process` 的 Job 管道本身（`bun_stdio_round_trips_through_windows_job_pipes` / `abrupt_host_exit_closes_the_job_and_kills_descendants`）。

---

## 7. 与"真实 agent（opencode/Claude Code/codex）对话"的关系

引擎层（§6）已能驱动**任何**符合 ora-plugin-protocol v1 的 Agent 插件对话。要接真实 agent CLI，还差 ③ 内的 ACP 桥：

- `packages/plugin-sdk/src/acp/`（`provider.ts`/`client.ts`/`translate.ts`/`wire.ts`/`queue.ts`/`index.ts`）—— `createAcpAgentProvider` 把 `AgentProvider` 的 8 方法翻译成 ACP `session/new`/`session/prompt`/`session/cancel` 等，并把 ACP 事件流翻成 `AgentEvent`。
- **状态**：该目录 `git ls-files` 追踪数 = 0（**未提交的工作区草稿**，`2026-07-21-...compatibility.md` §3.3 当时记为 MISSING 是就已提交代码而言，准确）。`acp/index.ts` 的注释明确示例用法（`spawn:{program:"claude-agent-acp", env:{ANTHROPIC_API_KEY:...}}`）。
- **对引擎的影响**：无。ACP 桥完全在 ③ 插件进程内消费，② 后端只见 ora-plugin-protocol 的 `AgentEvent` 流（`Grep -i acp` 在 `crates/plugin-manager`、`crates/plugin-protocol` 0 匹配）。引擎 e2e（§6.7）用一个手写 `AgentProvider`（不走 ACP）即跑通对话，证明引擎不依赖桥。

**结论**：接真实 opencode/Claude Code/codex，缺的是"一个用 `createAcpAgentProvider` 桥到该 agent CLI 的**插件包**"，而非引擎能力。

---

## 8. 能力边界与已知约束（v1）

| 约束 | 位置 | 说明 |
|---|---|---|
| 只支持 Agent kind | `handshake.rs` kind 守卫 / `service.rs:admitted_descriptor` | Workbench 仅 catalog 可见，`enable` 返 `UnsupportedKind` |
| manifestVersion/pluginApi/contractVersion 必须全 =1 | `manifest.rs` / `lifecycle.rs:validate` | v1 冻结版本轴 |
| main 必须 `dist/index.js` | `validation.rs` | 强制 materialized 布局 |
| 禁外部依赖 | `validation.rs:validate_materialized_javascript` | 仅 `node:`/`bun`/`bun:` 内置 |
| 启动后 admission 复检 | `handshake.rs` post-activate / `service.rs:admitted_descriptor` | epoch/revision/digest/owner 任一变即拒 |
| 应用层未接线 | `crates/application` 无 `ora-plugin-manager` 依赖 / `apps/web/server` 无 `/api/plugins*` | 引擎 OK，但 HTTP/UI 未暴露（见 `2026-07-21-...compatibility.md` §3.4） |

---

## 9. 复现路径

```bash
# 1. 准备 pinned Bun 资产（e2e 前置）
task prepare-plugin-runtime

# 2. 构建 SDK（pack CLI + agent ABI）
pnpm --filter @ora-space/plugin-sdk build

# 3. 跑真实进程 e2e（Windows）
cargo test -p ora-plugin-manager --test plugin_library_e2e -- --ignored --test-threads=1
cargo test -p ora-process bun_stdio_round_trips_through_windows_job_pipes -- --ignored --test-threads=1
cargo test -p ora-process abrupt_host_exit_closes_the_job_and_kills_descendants -- --ignored --test-threads=1
cargo test -p ora-plugin-manager --test runtime_windows_e2e -- --ignored --test-threads=1

# 4. 管理生命周期（不 spawn，任意平台）
cargo test -p ora-plugin-manager --test management_e2e
```

---

## 10. 文件索引（一手源）

| 模块 | 关键文件 |
|---|---|
| 线协议 | `crates/plugin-protocol/src/{frame,json_rpc,strict_json,lifecycle,manifest,identity,agent/{method,dto,leaf,validation}}.rs` |
| 管理面 facade | `crates/plugin-manager/src/{service,ports,config,events,events...}.rs` |
| 扫描 | `crates/plugin-manager/src/scanner.rs` |
| 验证 | `crates/plugin-manager/src/validation.rs` + `crates/plugin-protocol/src/manifest.rs` |
| 安装 | `crates/plugin-manager/src/install/{pipeline,reconcile,digest,receipt}.rs` |
| runtime actor | `crates/plugin-manager/src/runtime/{hub,supervisor,session_actor,handshake,transport,invocation,pending,outcome,startup,state,assets,generation}.rs` |
| 进程树 | `crates/process/src/{spec,traits,tokio_process,windows_tree,windows_tree/{command,pipes}}.rs` |
| TS bootstrap | `packages/plugin-runtime/src/{bootstrap/{main,session,loader,context,contracts},transport/{frame,writer},rpc/envelope,json/strict,generated/plugin-protocol}.ts` |
| TS 作者 SDK | `packages/plugin-sdk/src/{agent/index,pack/{builder,cli,scanner},types/*}.ts` |
| ACP 桥（WIP） | `packages/plugin-sdk/src/acp/{provider,client,translate,wire,queue,index}.ts` |
| e2e | `crates/plugin-manager/tests/{management_e2e,plugin_library_e2e,runtime_windows_e2e}.rs` + `tests/support/mod.rs` |
