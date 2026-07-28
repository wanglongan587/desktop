# 插件管理后端改动说明

> 本文档记录 `plugin-manager-20260720` 分支上，针对"插件管理 + agent 插件经 ACP 对话"所做的**后端改动**（不含前端）。内容基于实际代码，每处附文件路径。

---

## 一、干了什么（先说结论）

在这个分支上，我从零搭建了一套**完整的插件管理体系**，并跑通了"启用 agent 插件、agent 插件经 ACP 协议与 agent 对话"的端到端闭环。

具体做了这些事：

1. **定义了插件的契约和数据模型**——在 Rust 的 contracts 层加了一套插件类型（插件种类、清单、入口点、生命周期状态、增删改查请求响应），同时自动生成对应的 TypeScript 类型供前端使用。
2. **建了插件的持久化层**——在 domain 层定义了插件实体和生命周期状态机；在 db 层加了一张 `plugins` 表和对应的 SQLite 仓储实现，插件记录能存能取。
3. **实现了插件的管理用例**——在 application 层加了扫描、安装、列表、启用、禁用、卸载六个操作，每个操作走依赖注入的 handler 模式，可单测。
4. **在后端 facade 暴露了插件管理接口**——在 backend 层加了 `PluginApi`，把上面六个操作组合起来，通过 `Backend` 统一暴露给 Tauri 命令和 Web 路由。
5. **新建了 plugin-runtime 这个 Rust crate**——它负责真正运行插件进程：拉起插件子进程、通过 `[type:i8][length:i32 BE][payload]` 二进制帧做 JSON-RPC 通信、完成插件通道握手、把 ACP 会话更新通知路由到广播通道。
6. **重写了 plugin-sdk**——把原来只有 `getNums`/`returnNums` 的玩具脚手架，重写成一个真正的双向 JSON-RPC 插件作者 SDK（能收请求、发响应、发通知），使用同一套二进制帧格式。
7. **写了两个参考插件**——`mock-agent`（canned 数据，用于验证管道）和 `opencode`（真实连接 opencode 的 ACP 模式）。
8. **在 Tauri 层接线**——注册了插件管理命令和 agent 操作命令，把插件运行时的会话更新通知经 Tauri 事件推给前端。
9. **修了 Windows 下的进程拉起问题**——Windows 下 Rust 标准库找不到 `.cmd` 脚本（tsx/pnpm），在 host 侧自动用 `cmd /c` 包装。
10. **设计了二进制帧格式**——从换行分隔改为 `[type:i8=1][length:i32 大端序][payload]`，帧层通用（读/写任何 type）、分派层只处理 type=1（JSON），后续加 type=2（文件等）只需改一个 if 分支。

最终验证结果：用 mock-agent 真实进程跑通了端到端集成测试；opencode 的 ACP 线格式经探针验证；在 Desktop 上跑通了 opencode 经 ACP 的真实对话。

---

## 二、为什么这么做

### 背景问题

原来的"插件管理"只是一个写死的常量 `DEFAULT_AGENT_ID = "codex"` 加一张 agent 定义表（`AgentDefinition`，只有 id/name/description，没有启动规格）。ACP 传输是空壳（`createUnavailableAcpClient`，每个方法都抛错）。Rust 侧没有任何 plugin 概念。

也就是说：**插件管理这整层不存在**，但底层的契约和原语（ACP 数据模型、进程 spawn 原语）已经就绪。

### 架构选择（ADR-0001 已记录）

经过一轮设计拷问，确定了以下关键决策（详见 `docs/adr/0001-plugin-system-architecture.md`）：

- **模型 B**：插件带代码、自己持有 ACP（不是纯数据声明）。理由：opencode 是外部二进制，我们不为它写代码；但插件带代码给了灵活性（未来非 ACP agent 也能接入）。
- **子进程 + 标准输入输出 JSON-RPC**：不是进程内加载（WASM 难 spawn 子进程、原生库无隔离），而是子进程（语言无关、强隔离、和 ACP 自身模型一致）。
- **kind-first**：manifest 有 `kind` 枚举（agent/ui/workbench），每种 kind 一个 `PluginRuntime` trait 实现。加一种插件类型 = 扩枚举 + 加 runtime，不改 manifest 形状。
- **通用 agent 契约**：host 和插件之间的通道用 `agent/*` 方法名（不复用 ACP 的 `session/*`），payload 类型共享。这样插件通道契约不和 ACP 线格式耦合。
- **manifest 与 AgentDefinition 分离**：manifest 是跨 kind 的可安装单元；AgentDefinition 是 agent kind 激活后派生的运行态对象，供 chat 域引用。
- **生命周期状态机**：Discovered → Installed → Enabled → Started → Activated，用枚举 + 流转方法，让非法状态不可表示。

### 查证结论

- OpenAI codex CLI **不**支持 ACP。
- opencode (sst/opencode) **支持** ACP——命令 `opencode acp`，JSON-RPC over stdio，功能与终端一致。

所以 agent 用 opencode（有 ACP），codex（无 ACP）不用。opencode 插件是薄代理（Ora 的 agent 通道 ↔ opencode 的 ACP，payload 共享，只做方法名映射 + 转发通知）。

---

## 三、深入分析

### 3.1 分支基线

- 分支 `plugin-manager-20260720`，基于 `8779571`（与 main 的 merge-base）。
- 分支上 main 之外有约 20 个既有提交（chat 锚点导航、workspace 树动画等，非本文档范围）。
- 本文记录的改动**全部未提交**（工作区状态），叠在那些既有提交之上。

### 3.2 插件管理元数据层（Stage 0）

这一层只管"插件记录的存取和状态流转"，不涉及运行时（拉进程、通信）。它完全镜像了仓库既有的 agent_definition 全栈范式（domain 实体 → db 迁移/仓储 → application handler → backend facade）。

#### 契约层 `crates/contracts/src/plugin.rs`（新建）

定义了以下类型，全部通过 ts-rs 自动生成 `packages/contracts/src/plugin.ts`：

- `PluginKind` 枚举（`Agent`/`Ui`/`Workbench`），后两者预留、暂无 runtime。
- `PluginProcessEntrypoint`：插件进程的 spawn 配置（program/args/cwd/envs），是 `ora_process::ProcessSpec` 的可序列化投影。host 读它来拉起插件进程。
- `PluginManifest`：跨 kind 的可安装单元描述符（id/version/kind/entrypoint/displayName/description）。既是磁盘上的 `plugin.json` 格式，也是前端的 DTO。
- `PluginState` 枚举：生命周期状态（Discovered/Installed/Enabled/Started/Activated），镜像 domain 的 `PluginLifecycleState`。
- `Plugin`：受管视图（manifest 字段 + state + sourcePath），给管理界面用。
- `DiscoveredPlugin`：扫描发现的（manifest + sourcePath），安装请求用。
- `InitializeRequest`/`InitializeResponse`：插件通道握手 DTO。
- 六组 CRUD 请求/响应（Scan/Install/List/Enable/Disable/Uninstall）。
- `plugin_methods` 常量模块：插件通道的方法名字符串（`initialize`/`agent/newSession`/`agent/prompt`/`agent/cancel`/`shutdown`/`agent/sessionUpdate`），agent 方法复用 ACP payload 类型但用 `agent/*` 方法名。

在 `lib.rs` 加了 `mod plugin` + `pub use` + `export_typescript_bindings_to` 里调 `plugin::export`。

> 注意：`acp/rpc.rs`（Rust 的 JSON-RPC 信封类型）**没有**导出到 TS——仓库刻意把它留在 Rust 侧。所以 TS 侧的信封是手写的（在 plugin-sdk 的 protocol.ts 里）。

#### 领域层 `crates/domain/src/plugin.rs`（新建）

- `PluginKind` 枚举（镜像 contracts 的，但 domain 独立于 contracts），带 `database_value()`/`from_database_value()`（i64 映射，和 `SessionStatus` 一致）。
- `PluginLifecycleState` 枚举（Discovered/Installed/Enabled/Started/Activated），带 `database_value()`/`from_database_value()` + `transition_to()` 状态机（枚举 + `matches!` 实现，只允许合法流转）。
- `Plugin` 实体（id/kind/version/entrypoint[序列化 JSON 字符串]/displayName/description/state/sourcePath/auditFields）。`entrypoint` 存为不透明字符串——domain 层不解释 spawn 配置，运行时反序列化。
- `PluginId` newtype（在 `ids.rs` 用 `define_id!` 宏）。
- `DomainModelError` 加了 `InvalidPluginLifecycleState`/`InvalidPluginKind`/`InvalidPluginStateTransition`/`EmptyPluginVersion` 变体。

> domain 和 contracts **互不依赖**（两者只依赖 serde/thiserror）。它们之间的类型转换由 application 层的 mapper 做。

#### 数据库层

- `crates/db/src/migration/schema_v0004.rs`（新建，version "0004"）：建 `plugins` 表（id/kind[INTEGER]/version/entrypoint[TEXT]/display_name/description/state[INTEGER]/source_path/created_at/updated_at/is_deleted，共 11 列）。在 `catalog.rs` 的 `default_migration_catalog` 注册。
- `crates/db/src/repository/plugin.rs`（新建，仿 `agent_definition.rs`）：`SqlitePluginRepository` 实现 `PluginRepository` trait（create/find/list/update_state/soft_delete）。entrypoint 用 `serde_json::to_string` 存 TEXT，kind/state 用 i64。在 `repository/mod.rs` + `lib.rs` 导出。
- `crates/db/src/tests.rs`：更新了两个既有测试的断言（加了 plugins 表 + 0004 迁移到表名列表和迁移版本数组）。

> 一个 bug 在 live 跑时暴露：初版 schema_v0004 漏了 `display_name`/`description` 列（domain Plugin 有这俩字段，repository INSERT/SELECT 用了，但建表语句没写），导致 `UNIQUE constraint`/`no such column` 错误。已修复（加了这两列 + kind 改 INTEGER）。

#### 用例层 `crates/application/src/plugin/`（新建，仿 `agent_definition/` 五文件）

- `ports.rs`：`PluginRepository` trait（create/find/list/update_state/soft_delete）、`PluginScanner` trait（扫本地目录→DiscoveredPlugin 清单）、`PluginRepositoryError`/`PluginScannerError`。注意：**没有** `PluginIdGenerator`——插件 id 来自 manifest（manifest 的 `id` 字段是权威的，不生成）。
- `handlers.rs`：六个 handler，每个的泛型参数不同：`ScanPluginsHandler<Scanner>`（只需 Scanner）、`InstallPluginHandler<Repository, ClockSource>`、`ListPluginsHandler<Repository>`（只需 Repository）、`EnablePluginHandler<Repository, ClockSource>`、`DisablePluginHandler<Repository, ClockSource>`、`UninstallPluginHandler<Repository, ClockSource>`。全部静态分发。enable/disable 走 `PluginLifecycleState::transition_to` 状态机校验。一个私有 `transition_plugin_state` helper 复用。
- `mapper.rs`：`map_plugin_to_contract`（domain→contracts Plugin，反序列化 entrypoint JSON）+ `build_plugin_from_discovered`（contracts DiscoveredPlugin→domain Plugin，序列化 entrypoint）+ kind/state 枚举互转。
- `tests.rs`：集成测试（scan→install→enable[幂等]→disable→uninstall→NotFound 全流程）用 FakePluginRepository + FakePluginScanner + FixedClock。
- `error.rs`：加了 `PluginNotFound`/`PluginRepository`/`PluginManifestInvalid`/`PluginScanner`/`PluginStateTransition` 变体 + `from_plugin_*` 转换。
- `Cargo.toml`：加了 `serde` + `serde_json` 依赖（mapper 用）。

#### 后端 facade `crates/backend/src/plugin.rs`（新建，仿 `agent.rs`）

- `PluginApi`：只管元数据 CRUD，持各 handler，**不含运行时**（运行时在 plugin-runtime crate）。构造函数 `new(pool, clock, plugins_root)`，内部建 `SqlitePluginRepository` + `LocalDirPluginScanner`（从 application 导出）。
- `bootstrap.rs`：`Backend` 加 `plugin: Arc<PluginApi>` 字段；`BackendPaths` 加 `plugins_root: PathBuf` 字段；`open` 调 `ensure_directory(&paths.plugins_root)` + 构造 `PluginApi`。
- 六个 Backend 方法（scan_plugins/install_plugin/...）薄包装 handler。
- `error.rs`：`ApplicationError::PluginRepository` 映射为 `BackendError`（注：初版用了静态消息吞掉动态 sqlite 错误，debug 时改成透出真实 message 以排查 live 问题）。
- `plugins_root` 在 4 处构造点提供：Tauri/Web 用 `ORA_PLUGINS_ROOT` 环境变量覆盖（dev 指向 repo 的 plugins/），否则 app-data/plugins；2 个 backend 测试用 tempdir。

### 3.3 plugin-runtime crate `crates/plugin-runtime/`（新建）

这是运行时核心——把插件进程拉起来、和它做 JSON-RPC 通信、把 ACP 会话通知路由出去。依赖 `ora-process`（进程 spawn 原语）、`ora-contracts`（ACP 类型 + 插件方法常量）、tokio、tokio-util。

#### `src/channel.rs`：插件通道二进制帧 JSON-RPC 客户端

通信格式：`[type: i8 = 1][length: i32 大端序][payload: n 字节]`。5 字节帧头 + length 字节 payload。

- `PluginChannel`：持有插件进程的 stdin（写）+ 一个后台 stdout 读取任务（读）。request 用自增 id 关联 oneshot channel 等响应；notification（`agent/sessionUpdate`）路由到 `broadcast::Sender<SessionNotification>`。
- `build_frame(frame_type, payload)`：手动写字节（`push(type) + extend(to_be_bytes) + extend(payload)`），不用 Rust struct → 无 padding（5 字节，不是 8 字节）→ 无对齐 UB。
- `request<Req, Res>(method, params)`：序列化 JSON → `build_frame(1, payload)` → `write_all(&frame)` → 等 oneshot → 反序列化 result。
- `subscribe()`：返回 `broadcast::Receiver<SessionNotification>`，供 adapter 转发到 Tauri 事件。
- reader 任务：`read_exact(5 字节头)` → `i32::from_be_bytes` 解析 length → `read_exact(length 字节 payload)` → `serde_json::from_slice` 解析 JSON → 路由（有 id 给 pending caller，无 id 按 method 分派）。type≠1 帧读到后丢弃（未来改 1 个 if 分支即可分派）。`read_exact` 自动处理分包（读不满就等）和粘包（多帧在缓冲区里逐帧读）。
- MAX_PAYLOAD_SIZE = 16MB 防 corrupt 帧。
- 单元测试：用 `tokio::io::duplex` 模拟插件进程，验证二进制帧 initialize 请求/响应 id 关联。

#### `src/manager.rs`：插件运行时管理器

- `PluginRuntimeManager<Spawner: ProcessSpawner>`：泛型（静态分发，测试可注 fake spawner）。持 `sessions: Mutex<HashMap<plugin_id, ActivePlugin>>` + `broadcast::Sender<SessionNotification>`。
- `activate(plugin_id, entrypoint, source_path)`：用 `build_process_spec` 构造 `ProcessSpec` → `spawner.spawn` → `take_stdin/take_stdout` → 建 `PluginChannel` → 发 `initialize` 握手 → 缓存 handle。这是"Started"状态。
- `new_session/prompt/cancel`：转发到对应 channel 的 `request`，payload 是 ACP 类型。
- `deactivate`：kill 进程 + 移除 handle。
- `subscribe()`：暴露广播。
- `build_process_spec`：从 manifest entrypoint 构造 `ProcessSpec`，cwd 默认为插件 source_path（让 manifest 用相对路径 args）。**Windows 下裸名 program（tsx/pnpm）自动包 `cmd /c`**（std::process::Command 找不到 .cmd shim）。
- 端到端集成测试（`#[ignore]`）：spawn 真实 mock-agent（经 pnpm），驱动 activate→newSession→prompt（收 2 条 sessionUpdate）→cancel→deactivate，全通过。

#### `src/error.rs`

`PluginRuntimeError`（NotActive/AlreadyActive/Spawn/Channel/PluginError）+ `from_serde`/`from_io` helper。

### 3.4 plugin-sdk 重写 `packages/plugin-sdk/`

把 `getNums`/`returnNums` 玩具重写成双向 JSON-RPC 插件作者 SDK。

- `src/protocol.ts`：JSON-RPC 2.0 信封类型（Request/SuccessResponse/Error/Notification/Inbound）+ `methods` 常量（镜像 Rust 的 `plugin_methods`）+ `FRAME_TYPE` 常量（JSON=1）+ `Frame` 接口。
- `src/internal/reader.ts`：`frameIterator()` 累积 stdin Buffer → ≥5 字节解析帧头（`readInt8(0)` + `readInt32BE(1)`）→ ≥5+length 字节提取 payload → yield → 消费已读字节（处理分包/粘包）。`readFrame()` 返回 `{type, payload}`。`readMessage()` = readFrame → type=1 → JSON.parse（servePlugin 透明，非 type=1 帧跳过）。`readFrame`/`writeFrame` 已导出为通用帧 API。
- `src/internal/writer.ts`：`writeFrame(type, payload)` — `Buffer.alloc(5)` + `writeInt8(type, 0)` + `writeInt32BE(length, 1)` + `Buffer.concat([header, payload])` 写 stdout。`writeLine(msg)` = `writeFrame(1, JSON.stringify(msg))`。`sendResponse`/`sendError`/`sendNotification` 走 `writeLine`。
- `src/server.ts`：`servePlugin(handlers, {read})` 调度循环——读消息、分派 request 给 handler、handler 可经 `notify` 回调推通知、handler 报错转 error response、未知方法回 -32601。
- `src/index.ts`：导出 + import console-guard（stdout 保护，console 输出转 stderr）。
- 删了 getNums/returnNums/旧 protocol.ts/旧 host/index.ts + 三个旧测试（getNums.test.ts、returnNums.test.ts、console-guard.test.ts，后两者因用 bun:test 导入而随重写删除）。package.json 原来无测试脚本 + devDeps 只有 `bun-types`；重写后加了 `vitest` + `@types/node` + `"test": "vitest run"` 脚本。tsconfig 的 `types` 从 `["bun-types"]` 改为 `["node"]`。5 个测试（writer 3 + server 2）通过。

### 3.5 参考插件

#### mock-agent `plugins/mock-agent/`

- `plugin.json`：kind=agent，entrypoint `tsx src/adapter.ts`。
- `src/adapter.ts`：用重写后的 plugin-sdk 的 `servePlugin`，实现 initialize/agent/newSession/agent/prompt（吐 2 条 `agent_message_chunk` sessionUpdate + 返回 `stop_reason: end_turn`）/agent/cancel/shutdown。canned 数据。
- 验证：真实进程跑通——喂 initialize/newSession/prompt，输出 5 行正确协议（2 条 notification 正确交错在 prompt 响应前）。

#### opencode `plugins/opencode/`

- `plugin.json`：kind=agent，entrypoint `tsx src/adapter.ts`。
- `src/acp-stdio-client.ts`：`AcpStdioClient` 类——`spawn("opencode", ["acp"], {shell:true})`（shell:true 解决 Windows .cmd），ACP JSON-RPC 客户端（initialize/session/new/session/prompt 流 session/update/session/cancel）。id 关联 + session/update 路由到 prompt 的 onUpdate 回调。
- `src/adapter.ts`：`servePlugin` 桥接 host plugin 通道（`agent/*`）↔ opencode ACP（`session/*`）。host `initialize` 时懒 spawn opencode + ACP initialize。payload 共享，只做方法名映射 + `session/update`→`agent/sessionUpdate` 转发。
- 探针验证（`outputs/probe-opencode-acp.mjs`）：喂 ACP `initialize`（protocolVersion=1, clientCapabilities={}）→ opencode 回 `InitializeResponse`（protocolVersion=1, agentCapabilities, authMethods[opencode-login], agentInfo v1.18.4）。两个假设全部验证通过。
- `authenticate`：opencode 声明 auth 但调 `authenticate({method:"opencode-login"})` 回 "Invalid params"——opencode 已 `opencode auth login` 配置了凭证，故跳过 ACP authenticate 调用（依赖 opencode 自身凭证）。

### 3.6 Tauri 接线 `apps/desktop/src-tauri/`

- `state.rs`：`DesktopState` 加 `plugin_runtime: Arc<PluginRuntimeManager<TokioProcessSpawner>>`。
- `lib.rs`：`bootstrap_desktop` 构造 manager + spawn 一个 task 把 broadcast 的 `SessionNotification` 逐条 `app.emit("agent/sessionUpdate", payload)`；`BackendPaths` 加 `plugins_root`（`ORA_PLUGINS_ROOT` env 覆盖）。
- `commands.rs`：用 `backend_command!` 宏加 6 个 CRUD 命令（scan_plugins/install_plugin/list_plugins/enable_plugin/disable_plugin/uninstall_plugin）；手写 `plugin_activate`/`plugin_deactivate`/`plugin_agent_new_session`/`plugin_agent_prompt`/`plugin_agent_cancel`（需 `State`/`AppHandle`）。
- `error.rs`：加 `From<PluginRuntimeError> for CommandError`。
- `generate_handler!`：注册全部新命令。
- `Cargo.toml`：加 `ora-plugin-runtime` + `ora-process` 依赖（src-tauri 独立 Cargo workspace，相对路径）。

### 3.7 文档与验证

- `CONTEXT.md`：术语表（Host/Plugin/Agent/ACP/Plugin Channel/Plugin SDK/Plugin Manifest/Agent Definition + 状态 Enabled/Started/Activated）。
- `docs/adr/0001-plugin-system-architecture.md`：架构决策记录（模型 B + out-of-process + kind-first + 通用契约），含 4 个被否方案。
- `docs/research/plugin-manager-gap-analysis.md`：差距分析报告（本轮前的研究产出）。
- `docs/research/plugin-acp-agent-conversation.md`：加了 superseded 标注（第一份调研的"传输位置"方案被 ADR-0001 取代）。
- 验证：全量 Rust workspace 测试 + clippy + desktop src-tauri clippy + plugin-sdk 5 测试 + desktop web 8 测试 + app-shell 90 测试 + plugin-runtime 端到端集成测试（`#[ignore]`）+ mock-agent 真实进程验证 + opencode ACP 探针验证 + **Desktop live 跑通 opencode 经 ACP 对话**。

### 3.8 数据库迁移说明

仓库的迁移用 `CREATE TABLE IF NOT EXISTS`——如果表已存在，这条语句是空操作，**不会给已存在的表补列**。

实际情况：schema_v0001 从第一个提交 `648188a` 起**就包含** `agent_id`（git log 确认只有一个提交，未改过）。但用户的旧数据库的 sessions 表是在迁移系统引入之前创建的（缺 agent_id），之后迁移系统跑 `CREATE TABLE IF NOT EXISTS sessions` 是空操作（表已存在），所以 agent_id 列一直补不上。

所以：
- fresh DB：schema_v0001（含 agent_id）+ v0002/v0003/v0004 全应用，正常。
- stale DB（迁移系统之前创建的）：sessions 表缺 agent_id，`IF NOT EXISTS` 不补列，需删库重建。

我自己的 schema_v0004 也有类似问题：初版建表语句漏了 `display_name`/`description` 列（domain Plugin 有这俩字段、repository INSERT/SELECT 用了，但建表语句没写），导致 fresh DB 的 plugins 表缺这两列 → `no such column` 错误。已修 schema_v0004（补 display_name/description + kind 改 INTEGER），删库重建后正常。

这个 `IF NOT EXISTS` 不补列的行为导致 live 调试时删了两次库（一次删 stale sessions 库、一次删 buggy v0004 库）。

### 3.9 Windows .cmd spawn 问题

Rust 的 `std::process::Command::new("tsx")` 在 Windows 下找不到 `tsx.cmd`（.cmd 脚本需要 cmd.exe 解析 PATHEXT）。这个问题影响：
- plugin-runtime 的 `build_process_spec`：manifest 用 `program: "tsx"` → host spawn 失败。修法：Windows 下裸名自动包 `cmd /c`。
- opencode 的 AcpStdioClient 的 `spawn("opencode", ...)`：node 的 spawn 同理。修法：`shell: true`。
- 集成测试用 `pnpm` spawn mock-agent：也走 `cmd /c`（build_process_spec 包装）。
- `apps/desktop/vite.config.ts`：加了 `server.watch.ignored: ["**/src-tauri/target/**"]`——Vite 的文件监视器递归扫项目根（含 `src-tauri/target/`），并发 `cargo run` 写 .dll 时锁冲突（EBUSY）导致 vite 崩。排除后不再冲突。

### 3.10 二进制帧格式设计

插件通道（host ↔ plugin 之间）的通信帧格式：`[type: i8][length: i32 大端序][payload: n 字节]`。

**帧头 5 字节**：type 1 字节（payload 内容选择器，当前 1=JSON，预留 2=file 等）+ length 4 字节（payload 字节数，不含帧头，大端序）。总帧大小 = 5 + length。

**为什么用二进制长度前缀而非换行分隔**：
- 换行分隔（`[JSON][\n]`）的问题：payload 含 `\n` 会断帧；长度前缀是二进制安全的（payload 可含任意字节）。
- 业界模式（gRPC 的 `[compressed-flag:1][length:4][message]`、WebSocket 的 opcode 分派）：帧层按 type 分派，应用层只看到类型化消息。

**Rust 侧 padding 优化**：不用 `struct FrameHeader { type: i8, length: i32 }`（默认 8 字节，i8 后 3 字节 padding）；也不用 `#[repr(packed)]`（5 字节但有引用 UB 风险）。改用手动写字节（`push(type) + extend(to_be_bytes)` → 正好 5 字节，无 padding，无对齐 UB）。

**帧层通用 + 分派层 type=1 专用**：
- `readFrame()`/`writeFrame(type, payload)` 是通用的——读/写任何 type 的帧，不硬编码 type=1。
- `readMessage()`/`writeLine()` 是 type=1 专用——读 type=1 帧 JSON.parse 返回给 servePlugin；写 type=1 帧。
- type≠1 帧当前读到后丢弃。后续加 type=2（文件等）= 把 `type≠1: continue` 改成 `type≠1: 调回调`，改 1 个 if 分支，~25 行，不改帧层。

**分包粘包**：Rust 侧 `read_exact` 自动处理（读不满 5 字节/length 字节就等，读多了逐帧读）；TS 侧 `frameIterator` 累积 Buffer → ≥5 字节解析头 → ≥5+length 字节提取 payload → 消费已读字节保留余量（处理粘包：一帧读完缓冲区里还有下一帧的数据）。

---

## 四、改动文件清单

### 新建（后端 + 插件 + 文档）

- `crates/contracts/src/plugin.rs`
- `crates/domain/src/plugin.rs`
- `crates/db/src/migration/schema_v0004.rs`
- `crates/db/src/repository/plugin.rs`
- `crates/application/src/plugin/{mod,ports,handlers,mapper,tests}.rs`
- `crates/backend/src/plugin.rs`
- `crates/plugin-runtime/`（Cargo.toml + src/{lib,channel,manager,error}.rs）
- `plugins/mock-agent/`（package.json + plugin.json + src/adapter.ts）
- `plugins/opencode/`（package.json + plugin.json + tsconfig.json + src/{acp-stdio-client,adapter}.ts）
- `packages/contracts/src/plugin.ts`（codegen 生成）
- `CONTEXT.md`、`docs/adr/0001-plugin-system-architecture.md`

### 修改（后端 + 接线）

- `crates/contracts/src/lib.rs`（加 plugin 模块导出 + codegen 调用）
- `crates/domain/src/{lib,ids,error}.rs`
- `crates/db/src/{lib,migration/catalog,migration/mod,repository/mod,tests}.rs`
- `crates/application/src/{lib,error}.rs` + `Cargo.toml`
- `crates/backend/src/{bootstrap,error,lib}.rs`
- `apps/desktop/src-tauri/src/{commands,state,lib,error}.rs` + `Cargo.toml`
- `apps/web/server/src/{bootstrap,error}.rs`（plugins_root + ApplicationError→HTTP 映射）
- `packages/plugin-sdk/`（重写：protocol/reader/writer/server/index + 删 getNums/returnNums/旧测试 + package.json 加 vitest 脚本/tsconfig types 改 node）
- `packages/contracts/src/index.ts`（加 plugin 导出）
- `pnpm-workspace.yaml`（加 plugins/*）
- `Cargo.toml`（加 plugin-runtime 到 members + workspace deps）

---

## 五、验证状态

| 验证项 | 结果 |
|---|---|
| Rust workspace 全量测试 | 通过 |
| Rust clippy（workspace + desktop src-tauri） | 净 |
| plugin-sdk vitest（5 测试） | 通过 |
| desktop web vitest（8 测试） | 通过 |
| app-shell vitest（90 测试） | 通过 |
| plugin-runtime 通道单测（duplex + 二进制帧） | 通过 |
| plugin-runtime 端到端集成测试（`#[ignore]`，真实 mock-agent + 二进制帧） | 通过 |
| mock-agent 真实进程（二进制帧 initialize/newSession/prompt） | 5 帧正确协议（2 notification + 3 response） |
| opencode ACP 探针（喂 initialize） | protocolVersion=1 + clientCapabilities={} 验证通过 |
| Desktop live（ORA_PLUGINS_ROOT + opencode auth login + pnpm tauri dev） | create_session 成功 + update_session 流转，对话跑通 |
