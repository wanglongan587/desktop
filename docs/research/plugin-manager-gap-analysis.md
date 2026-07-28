# 插件管理能力差距分析（后端视角）

> 仓库 `D:\project\plugin-manager-20260720`（Ora monorepo）当前的"插件管理"只是一个用**写死 agent id**（`"codex"`）的最小实现；本报告分析将其演进为完整插件管理系统（扫描/安装/生命周期/激活执行，并先支持 agent 插件经 ACP 协议与 agent 对话、后续支持 UI/workbench 插件）所差的能力。分析以源码为据，每条结论附 `文件路径:行号`。

- 调研日期：2026-07-23
- 范围：**仅后端**（Rust crates + 跨语言的契约/SDK 边界），指出前后端契约落点但不展开前端实现
- 关联文档：`docs/research/plugin-acp-agent-conversation.md`（2026-07-22，聚焦 ACP 传输与接线）——本报告是其**插件管理维度的补充与深化**，重叠处仅作锚点引用，重点放在 manifest/registry/scan/install/lifecycle/激活执行/SDK/多类型扩展性

---

## 一句话概述

Ora 的**契约层与进程原语层已就绪**（ACP 全量数据模型、`process`/`pty` spawn 原语、`AcpClient` 抽象、聊天状态机），但**插件管理这一整层几乎不存在**：没有 plugin 类型抽象、没有 manifest、没有 registry/catalog、没有 scanner、没有 installer、没有生命周期状态机、没有 loader/runtime/sandbox、`plugin-sdk` 是 `getNums/returnNums` 玩具脚手架、agent 概念只是 CRUD 元数据且默认 id 硬编码。把"agent 插件 + ACP 对话"打通需要的不只是 ACP 传输，更是一整套插件管理抽象，且这套抽象必须从一开始就为 UI/workbench 插件留扩展位。

---

## 1. 仓库与技术栈概览

Ora 是一个 **pnpm + Cargo 双 workspace 的 monorepo**，pnpm workspace 见 `pnpm-workspace.yaml:1-4`（`apps/*`、`apps/web/client`、`packages/*`）。

### 1.1 Rust 侧（`crates/` + `apps/*/src-tauri` + `apps/web/server` + `xtask`）

| crate | 角色 | 关键文件 |
|---|---|---|
| `ora-contracts` (`crates/contracts`) | 契约 DTO（Rust + 经 `ts_rs` 生成 TS） | `crates/contracts/src/lib.rs:58` |
| `ora-domain` (`crates/domain`) | 纯领域实体 | `crates/domain/src/agent_definition.rs:6`、`session.rs:72` |
| `ora-application` (`crates/application`) | use-case handler（ports/handlers/mappers/id_generator） | `crates/application/src/agent_definition/` |
| `ora-backend` (`crates/backend`) | 组合层，把 handler 接到 SQLite | `crates/backend/src/lib.rs:37`、`bootstrap.rs:49` |
| `ora-db` (`crates/db`) | SQLite 仓储 + 版本化迁移 | `crates/db/src/migration/catalog.rs:9` |
| `ora-process` (`crates/process`) | OS 子进程 spawn 原语 | `crates/process/src/spec.rs:29`、`traits.rs:13` |
| `ora-pty` (`crates/pty`) | PTY 运行时管理 | `crates/pty/src/lib.rs:6` |
| `ora-logging` / `ora-gitlancer` | 日志 / Git 操作库 | — |
| `apps/web/server` | axum HTTP 运行时（仅 CRUD） | `apps/web/server/src/routes.rs:14`、`main.rs:17` |
| `apps/desktop/src-tauri` | Tauri 运行时（仅 CRUD 命令） | `apps/desktop/src-tauri/src/lib.rs:28` |

技术栈：Rust（axum + tokio + tauri + rusqlite）、`ts_rs` 做 Rust→TS 契约同步、`tracing` 结构化日志。

### 1.2 TS 侧（`packages/` + `apps/*/web`）

| package | 角色 | 关键文件 |
|---|---|---|
| `@ora/contracts` (`packages/contracts`) | 生成的 TS 契约 + typed HTTP 客户端 | `packages/contracts/src/client.ts:46` |
| `@ora/chat` (`packages/chat`) | **ACP 客户端消费者状态机**（完整） | `packages/chat/src/client.ts:12`、`store.ts:54` |
| `@ora-space/plugin-sdk` (`packages/plugin-sdk`) | **插件 SDK 雏形（玩具）** | `packages/plugin-sdk/src/host/index.ts:1` |
| `@ora/mock-service` | 内存 mock ACP agent（参考实现） | `packages/mock-service/src/acp.ts` |
| `@ora/app-shell` / `@ora/platform` / `@ora/ui` | UI 外壳 / 平台适配 / 组件库 | `packages/app-shell/src/state/hooks/use-workspace-mutations.ts:17` |

技术栈：TypeScript + React + Vite + TanStack Query + Zustand。

### 1.3 架构约定（来自 `AGENTS.md`）

`AGENTS.md:1-62` 记录了强约定，对插件管理设计有直接约束：**依赖注入 + Trait 接口**（`:3`）、**静态分发优先**（`:4`）、**用 enum 让非法状态不可表达**（`:5`，即状态机用 enum 不用可选字段）、**不保留向后兼容**（`:6`）、**模块 <500 LoC**（`:31`）、**新建 trait 必须有 doc 注释**（`:24`）。这些是后续设计必须遵守的硬约束。

---

## 2. 当前插件管理现状

### 2.1 "插件 id"写死在哪里

当前**没有"插件"概念**，最接近的是"agent"。默认 agent id 硬编码在 UI 侧：

- `packages/app-shell/src/state/hooks/use-workspace-mutations.ts:17`：
  ```ts
  export const DEFAULT_AGENT_ID = "codex";
  ```
  注释（`:11-16`）说明它被 session 对话框和 composer 的"首消息即建 session"路径共享，"so the two cannot drift onto different agents"。即**整个应用隐式只认一个 agent**。
- `useCreateSession`（`use-workspace-mutations.ts:138-176`）建 session 时：
  - `:155-158` 调 `chatStore.getState().newSession({ cwd: project.rootPath, mcpServers: [] })`——**不传任何 agent 标识给 newSession**，ACP `NewSessionRequest` 里没有 agent 选择维度。
  - `:159-161` 持久化 `client.session.create({ taskId, agentId, agentSessionId, status })`，`agentId` 由调用方传入（默认即 `"codex"`）。
- `useAgents`（`use-agents.ts:6-12`）只是 `client.agent.list({})` 列出 agent 记录的 name/description，**不消费任何启动规格**——因为根本没有启动规格可消费。

**结论**：写死的不是"插件 id"，而是"默认 agent id"，且 agent 记录本身无法描述如何启动。这是"插件管理最小实现"的本质——一个占位常量 + 一张 CRUD 表。

### 2.2 AgentDefinition 模型：只有元数据，没有启动规格

- 领域实体 `AgentDefinition`（`crates/domain/src/agent_definition.rs:6-11`）只有 `id / name / description / audit_fields`。`new()`（`:15-33`）仅校验 name 非空。
- 契约 DTO `Agent`（`crates/contracts/src/agent.rs:8-12`）只有 `id / name / description`；`CreateAgentRequest`（`:18-21`）/`UpdateAgentRequest`（`:65-69`）同样只有 name/description。
- 即"agent"在 Ora 领域里**只是一个可配置的 agent 类型元数据**，**没有任何启动规格**——无 `command / args / env / transport / capabilities` 字段，无法据此 spawn 一个真实 agent 进程。这与 `docs/research/plugin-acp-agent-conversation.md` 第 50 条结论一致。

### 2.3 Agent 的 CRUD 全链路（纯元数据，无运行时）

- 应用层 `AgentDefinitionRepository` trait（`crates/application/src/agent_definition/ports.rs:4-34`）仅 `create / find / list / update / soft_delete` 五个存储操作；`AgentDefinitionIdGenerator`（`:37-40`）只产 id。
- handler（`crates/application/src/agent_definition/handlers.rs`）：`CreateAgentDefinitionHandler::handle`（`:38-58`）构造 `AgentDefinition::new(id, name, description, AuditFields)`——纯元数据组装。
- 后端组合 `AgentApi`（`crates/backend/src/agent.rs:15-25`）把 handler 接到 `SqliteAgentDefinitionRepository`，`Backend::open`（`crates/backend/src/lib.rs:49-73`）在 `:69` 构造 `AgentApi`。`Backend`（`:37-45`）组合了 Project/Task/Session/Skill/Agent 五类 CRUD API，**全部是 SQLite 上的元数据 CRUD，无任何运行时/进程/会话驱动**。

### 2.4 Session 与 ACP 的接线点（已建模但未通电）

- `Session`（`crates/domain/src/session.rs:72-79`）：`agent_id: AgentId` + `agent_session_id: Option<String>` + `status: SessionStatus`。
  - `AgentId`（`:8-22`）是 newtype，内置 `TERMINAL = "terminal"`。
  - `SessionStatus`（`:38-41`）只有 `Running / Stopped` 两态——**这是 Ora session 的状态机，不是插件生命周期状态机**。
  - `agent_session_id`（`:76`）就是 ACP agent 返回的 session id，**这正是 Ora session 与 ACP session 的接线点**，但目前只是个被持久化的字符串，生产环境下从未由真实 ACP `session/new` 产生（见 `docs/research/plugin-acp-agent-conversation.md` 第 54、160 条）。

### 2.5 ACP 协议契约：完整建模，零运行时

- 模块清单 `crates/contracts/src/acp/mod.rs:1-20`，覆盖 `authentication / common / content / error / file / initialization / literals / mcp / notification / permission / plan / prompt / rpc / serde_util / session / session_config_options / session_mode / slash_command / terminal / tool_call`——一份相当完整的 ACP 数据模型。
- JSON-RPC 2.0 信封、方法名常量、`initialize` 握手、`session/*` 生命周期、`session/update` 通知变体、`session/prompt`、`ContentBlock`、认证、错误码均忠实于规范（详见 `docs/research/plugin-acp-agent-conversation.md` 第 31-44 条；规范出处 `crates/contracts/src/acp/session.rs:142` 注释直接引用 `https://agentclientprotocol.com/`）。
- **能力声明（capabilities）已建模**——这一点对插件 manifest 设计是重要复用点：
  - `ClientCapabilities`（`crates/contracts/src/acp/initialization.rs:134-160`）：fs / terminal / session。
  - `AgentCapabilities`（`:315-379`）：loadSession / prompt / mcp / session / auth。
  - `SessionCapabilities`（`:497-547`）：list / delete / additional_directories / resume / close。
- Rust→TS 同步：`crates/contracts/src/lib.rs:58-73` 的 `export_typescript_bindings_to` 调 `acp::export`（`crates/contracts/src/acp/mod.rs:26-44`），每个结构体的 `#[ts(export_to = "acp/...")]` 决定输出路径。
- **关键**：**只实现了 client 侧契约**，没有任何代码把 `InitializeRequest`/`NewSessionRequest`/`PromptRequest` 序列化后写进子进程 stdin，也没有代码读 stdout 上的 `session/update`。这条"wire"是断的（`docs/research/plugin-acp-agent-conversation.md` 第 135 条）。

### 2.6 进程 / PTY 原语：可直接复用

- `ProcessSpec`（`crates/process/src/spec.rs:29-153`）：`program / args / cwd / envs / stdin / stdout / stderr / kill_on_drop`，builder 风格。`ProcessStdio`（`:7-15`）有 `Piped / Inherit / Null`，**默认 `Piped`**（`:47-49`）——正好满足 ACP stdio 需求。`kill_on_drop` 默认 `true`（`:37`）。
- `ManagedProcess` trait（`crates/process/src/traits.rs:25-68`）：`take_stdin/take_stdout/take_stderr`（`:37-43`）、`try_wait`（`:46`）、`wait`（`:49`）、`kill`（`:67`，显式进程树级终止）。`ProcessSpawner` trait（`:13-19`）`spawn(spec) -> Self::Process`，**设计为可注入 fake spawner**（注释 `:11-12`），天然适配 DI/测试。
- PTY：`crates/pty/src/lib.rs:6-18` 暴露 `PtyRuntimeManager` / `PtyProcessFactory` 等，可用于 ACP client 侧 `terminal/*` 反向方法。

**判定**：spawn 一个 stdio 子进程并把 stdin/stdout 接到 JSON-RPC 编解码器，**所有原语已就绪**，缺的是上层的 ACP 编解码 + 握手 + 会话驱动。

### 2.7 TS 侧 ACP 消费者：完整，只差真实传输

- `AcpClient` 接口（`packages/chat/src/client.ts:12-17`）：`newSession / prompt / cancel / subscribe(listener)` 四方法，传输无关。
- `createUnavailableAcpClient()`（`packages/chat/src/client.ts:20-31`）：每方法 `throw new Error("ACP transport is not configured")`——Web 生产环境注入的实现。
- `createChatStore(client)`（`packages/chat/src/store.ts:54`）是完整 ACP 消费者：`sendMessage` 调 `client.prompt`、`subscribe` 经 `applySessionUpdate` 装配 `agent_message_chunk / tool_call / plan` 等、`cancelMessage` 调 `client.cancel`（见 `docs/research/plugin-acp-agent-conversation.md` 第 69-74 条）。
- mock ACP agent（`packages/mock-service/src/acp.ts`）+ 一致性测试 `exerciseAcpClientConformance`（`packages/chat/src/conformance.ts`）——真实传输的验证基线已备。

### 2.8 plugin-sdk：玩具脚手架

- 包元数据（`packages/plugin-sdk/package.json`）：`@ora-space/plugin-sdk` v0.1.3，**仅导出 `./host`**（`:11-16`），依赖 `bun-types`。
- 协议形状（`packages/plugin-sdk/src/host/protocol.ts:1-23`）：`HostRequest{id,method,params}`、`SuccessResponse{jsonrpc:"2.0",id,result}`、`ErrorResponse{jsonrpc:"2.0",id,error:{code,message}}`——形态类 JSON-RPC，但**方法名是任意的**，未与 ACP 方法对齐。
- 入口（`packages/plugin-sdk/src/host/index.ts:1-6`）仅导出 `getNums`（`getNums.ts:16-39`，从 stdin 读一行 JSON-RPC 请求，校验 `jsonrpc==="2.0" && id && method` 在 `:24-27`）与 `returnNums`（`returnNums.ts:10-37`，向 stdout 写响应）。
- 底层 `internal/reader.ts:7-37`（stdin 逐行 async iterator，模块级单例 buffer）、`internal/writer.ts:6-8`（stdout 写行）。
- 一个**有价值的细节**：`console-guard.ts:1-19` 在 import 时自动把 `console.log/warn/error` 重定向到 **stderr**（`:11-19`），保证 stdout 仅承载 JSON-RPC——说明作者理解 stdio JSON-RPC 的管道纪律，这套"console 保护 + 行收发"可作为后续 host 协议的基础。
- **判定**：这是验证"host 经 stdin/stdout 与子进程交换 JSON-RPC 行"的占位骨架，**没有** manifest、发现、加载、注册表、生命周期、沙箱，也未路由到 ACP 的 `initialize/session/new/session/prompt`。

### 2.9 传输/命令面：纯 CRUD，无 ACP/插件端点

- Web 路由 `apps/web/server/src/routes.rs:14-75` 的 `build_router`：仅 `/health/*` + `/api/file-system/directory` + projects/tasks/sessions/skills/agents 的 REST CRUD，**无 WebSocket/SSE/流式端点**（`main.rs:17-38` 是 `#[tokio::main]` axum serve）。
- Desktop 命令 `apps/desktop/src-tauri/src/lib.rs:28-56`：25 个 CRUD 命令 + `get_desktop_config` + `set_worktree_root`，**无任何 ACP/插件命令**。
- 前端端点清单 `crates/contracts/src/frontend.rs:114-423` 是一个 `&[FrontendEndpoint]` 静态常量数组，编译期与路由锁步——**其中无 ACP 端点、无插件端点**。新增端点需在此登记（`docs/research/plugin-acp-agent-conversation.md` 第 90、107 条）。

### 2.10 DB 迁移系统：可承载插件安装持久化

- `Migration`（`crates/db/src/migration/catalog.rs:9-45`）：`version + up_statements + down_statements`，**有 up/down 双向**——这对插件安装的"原子性 + 回滚"是直接可复用的模式。
- `MigrationCatalog`（`:48-60`）持有全部 migration + target_versions，`Backend::open` 在 `crates/backend/src/bootstrap.rs:57-60` 经 `default_migration_catalog()` + `DatabaseBootstrapper::system().bootstrap_repository_pool(...)` 启动。新增表（如 plugin 安装记录、启用状态）走加一条 migration 即可（见 `docs/database-migrations.md`）。

### 2.11 既有设计文档

`docs/` 下没有任何关于插件、plugin SDK、多插件类型的设计文档或 ADR；`docs/research/` 是新建目录，仅有 `plugin-acp-agent-conversation.md`。本报告是 `docs/research/` 的第二份。

---

## 3. 能力差距矩阵

> 现状取值：✅ 已就绪 / 🟡 桩或部分 / ❌ 完全缺失。优先级针对"agent 插件 + ACP 对话"这一当前阶段目标。

| 能力 | 现状 | 差距 | 优先级 |
|---|---|---|---|
| ACP 数据模型（Rust+TS） | ✅ `crates/contracts/src/acp/` | 无 | — |
| Rust↔TS 契约同步管道 | ✅ `crates/contracts/src/lib.rs:58` | 无 | — |
| 进程 spawn 原语（stdio/kill） | ✅ `crates/process/src/spec.rs:29` | 无 | — |
| PTY 运行时（terminal/* 复用） | ✅ `crates/pty/src/lib.rs:6` | 无 | — |
| `AcpClient` 抽象 + 一致性测试 | ✅ `packages/chat/src/client.ts:12` | 无 | — |
| 聊天状态机（ACP 消费者） | ✅ `packages/chat/src/store.ts:54` | 无 | — |
| DB 迁移系统（up/down） | ✅ `crates/db/src/migration/catalog.rs:9` | 无 | — |
| **plugin 类型抽象（agent/UI/workbench）** | ❌ 无 | 需新建 enum/trait 体系 | P0 |
| **plugin manifest schema（元数据/版本/依赖/capabilities）** | ❌ 无 | 需新建契约 + 校验 | P0 |
| **plugin 注册中心（registry/catalog/发现）** | ❌ 无 | 需新建 | P0 |
| **plugin 扫描（本地目录 / 远程 / marketplace）** | ❌ 无 | 需新建（本地目录优先） | P1 |
| **plugin 安装（下载/校验/解压/依赖/原子/回滚）** | ❌ 无 | 需新建 | P2 |
| **plugin 生命周期状态机** | 🟡 Ora SessionStatus 仅 Running/Stopped（`session.rs:38`） | 需 plugin 专属状态机 | P0 |
| **plugin 激活/执行抽象（loader/runtime）** | ❌ 无 | 需新建（trait + 实现） | P0 |
| **plugin 沙箱/隔离/权限模型** | ❌ 无 | 需新建 | P2 |
| **AgentDefinition 启动规格字段** | ❌ 仅 name/description（`agent_definition.rs:6`） | 需加 launch 字段或引入 plugin manifest | P0 |
| **ACP stdio 运行时（Rust）** | ❌ 无 | 需新建 crate（复用 process+contracts） | P0 |
| **agent 插件 → ACP 运行时绑定（实例池）** | ❌ 无 | 需 registry 缓存 AcpClient | P0 |
| **initialize / authenticate 握手** | 🟡 契约已建（`initialization.rs:20`），运行时未实现 | 需实现 | P0 |
| **TS↔Rust ACP 通道** | 🟡 仅 `createUnavailableAcpClient`（`client.ts:20`） | 需 Tauri 命令+事件 或 HTTP SSE | P0 |
| **client 侧 ACP 反向方法**（fs/terminal/permission） | 🟡 契约已建（`literals.rs`），无 handler | 需实现 | P1 |
| **plugin SDK API 表面**（host services/extension points/context） | 🟡 `getNums/returnNums` 占位（`host/index.ts:1`） | 需重写为 ACP 方法路由 + 多类型扩展点 | P1 |
| **默认 agent id 写死** | 🟡 `DEFAULT_AGENT_ID="codex"`（`use-workspace-mutations.ts:17`） | 需从 registry 解析 | P0 |
| **插件配置持久化** | 🟡 无专属表 | 需迁移 + 仓储 | P1 |
| **事件总线** | ❌ 无 | 需新建（session/update 等流式事件） | P1 |
| **marketplace / 远程仓库** | ❌ 无 | 后续阶段 | P3 |

---

## 4. 分主题详细差距分析

### 4.1 插件清单/描述模型（manifest schema）

**现状**：完全缺失。最接近的"描述模型"是 `AgentDefinition`（`crates/domain/src/agent_definition.rs:6-11`）和 `Agent` 契约（`crates/contracts/src/agent.rs:8-12`），但二者只有 `id/name/description`，没有版本、依赖、能力声明、入口点、传输方式。`plugin-sdk` 的 `HostRequest`（`packages/plugin-sdk/src/host/protocol.ts:2-6`）是运行时消息格式，不是清单。

**差距**：需要一套 `PluginManifest` schema，至少包含：
- 标识：`id`（稳定唯一）、`name`、`description`、`version`（semver）、`author`、`ora_runtime_version`（兼容性约束，类比 `crates/contracts/src/acp/initialization.rs:10` 的 `ProtocolVersion`）
- **类型**：`kind: PluginKind`（`Agent / Ui / Workbench / ...`）——这是支持多插件类型的根，必须第一天就建（见 4.9）
- **入口点/启动规格**：对 agent 插件即 `command / args / env / cwd / transport(Stdio)`；对 UI/workbench 插件是别的内容（见 4.9）
- **能力声明 capabilities**：可**直接复用** ACP 已建模的 `AgentCapabilities`（`initialization.rs:315`）/`SessionCapabilities`（`:497`）作为 agent 插件的能力声明子集；UI/workbench 插件各自有自己的 capability 集
- 依赖：`dependencies: Vec<PluginDependency>`（id + version_range）
- 权限申请：`permissions: Vec<PermissionRequest>`（fs/terminal/network/...），对齐 ACP client 侧反向方法（`session/request_permission`、`fs/*`、`terminal/*`）
- 校验：manifest schema 校验 + 签名/哈希校验（安全相关，见 4.10）

**建议落点**：`crates/contracts/src/plugin.rs`（新建），经 `lib.rs:58` 的 `export_typescript_bindings_to` 自动生成 `packages/contracts/src/plugin.ts`，与现有契约同步机制一致。manifest 既可随插件包分发（外部贡献插件），也可由 `AgentDefinition` 扩展承载（内置声明式 agent）——这是关键决策点（见 6.1）。

### 4.2 插件注册中心（registry / catalog / 发现）

**现状**：完全缺失。`AgentApi`（`crates/backend/src/agent.rs:15-25`）是 CRUD 直通 SQLite，不是运行时注册表；`docs/research/plugin-acp-agent-conversation.md` 第 3、166 条指出"agent 插件 → ACP 运行时的绑定"完全不存在。

**差距**：需要一个**运行时 PluginRegistry**（区别于 SQLite 元数据表）：
- 按 `plugin_id` 解析 manifest → 构造对应 `PluginKind` 的 runtime handle
- 对 agent 插件：缓存/复用 `AcpClient` 实例（进程池或懒加载），处理崩溃重启、空闲回收、`session/close`
- 对 UI/workbench 插件：缓存对应 runtime handle
- 提供 `list() / get(id) / status(id)` 给 UI 选择器（替换 `use-agents.ts:6` 的纯列表）
- 与持久化的"已安装/已启用"状态表联动（见 4.5）

**建议落点**：新 crate `crates/plugin-runtime`（或 `crates/plugin-manager`），对外暴露 trait（DI，符合 `AGENTS.md:3`），由 `Backend` 或独立 runtime 持有。registry 不属于 SQLite CRUD 层，应与 `AgentApi` 同级而非下沉进 `ora-application`。

### 4.3 插件扫描（scan）

**现状**：完全缺失。无任何"扫目录/读 manifest"代码。grep 全仓 `scan` 命中均为无关词。

**差距**：
- **本地目录扫描**（当前阶段优先）：约定一个插件根目录（如 `app_data_dir/plugins/`，对齐 `apps/desktop/src-tauri/src/lib.rs:65-68` 已有的 `app_data_dir` 解析），递归发现 `plugin.json`/`manifest.json`，解析为 `PluginManifest`，登记进 registry。
- **远程仓库/marketplace 扫描**：后续阶段，需定义 marketplace 协议、索引格式、缓存策略（见 6.x，基于通用知识）。
- 扫描需处理：manifest 校验失败跳过、重复 id 冲突、版本选择、热重载/变更检测。

**建议落点**：`crates/plugin-runtime/src/scanner.rs`（新建）。scanner 应是 trait（注入文件系统抽象以利测试，符合 `AGENTS.md:3,46`）。

### 4.4 插件安装（install）

**现状**：完全缺失。无下载、校验、解压、依赖解析、原子写入、回滚代码。

**差距**：
- 下载：从 marketplace/URL 拉取插件包（后续阶段；agent 插件本地安装可跳过）
- 校验：哈希 + 签名验证 manifest 完整性
- 解压：落到插件根目录
- 依赖解析：据 manifest `dependencies` 解析并安装依赖（拓扑排序、冲突检测）
- **原子性 + 回滚**：可复用 DB migration 的 up/down 双向模式（`crates/db/src/migration/catalog.rs:9-45` 已有 down_statements）——安装写一张 `plugin_installations` 表 + 文件原子写入（先写临时目录再 rename），失败回滚
- 注册：安装成功后登记进 registry + 持久化"installed"状态（见 4.5）

**建议落点**：`crates/plugin-runtime/src/installer.rs`（新建），installer trait 注入下载器/文件系统/校验器。当前阶段若 agent 插件走"内置声明式"（见 6.1），安装可后置；若走"外部贡献插件包"，安装是 P2。

### 4.5 插件生命周期状态机

**现状**：Ora 仅有 `SessionStatus{Running,Stopped}`（`crates/domain/src/session.rs:38-41`），这是 session 运行状态，**不是插件生命周期**。无 installed/enabled/active/running 等概念。

**差距**：需一套 plugin 专属状态机。按 `AGENTS.md:5`"用 enum 让非法状态不可表达"，建议：

```
Installed → Disabled ⇄ Enabled → Activating → Active → (Deactivating) → Enabled
                                                       ↓
                                                    Error
                  Uninstalled（终态，需先 Disabled）
```

- `Installed`：包已落盘 + manifest 已登记，默认未启用
- `Enabled`：用户启用，允许被激活（持久化开关）
- `Active`：runtime handle 已构造、资源已就绪（agent 插件 = AcpClient 已 initialize 握手、进程已 spawn）
- `Running`：有进行中的 session（可复用 `SessionStatus` 语义，但归属插件而非 session 单条记录）
- `Error`：激活/运行失败，可重试或回 Disabled
- `Uninstalled`：移除

状态流转需持久化（enabled/disabled 跨重启）、需事件广播（见 4.10）。

**建议落点**：`crates/domain/src/plugin.rs`（新建 enum `PluginLifecycleState`）+ `crates/plugin-runtime` 的状态流转逻辑。持久化走 `crates/db` 一条新 migration + `plugin_installations`/`plugin_states` 表。

### 4.6 插件激活/执行抽象（loader / runtime / 沙箱）

**现状**：完全缺失。`crates/process` 提供了**通用 spawn 原语**（`ProcessSpec`/`ManagedProcess`），但没有任何"插件 loader"把它们与 manifest/ACP 装配起来。`plugin-sdk` 的 `getNums/returnNums` 是子进程侧的占位，host 侧没有 loader。

**差距**：需要一层 **PluginRuntime** 抽象，按 `PluginKind` 分发：
- trait `PluginRuntime`（DI，`AGENTS.md:3`）：`activate(manifest) -> Handle`、`deactivate(handle)`、`status(handle)`
- 对 **agent 插件**：实现 `AcpStdioRuntime`——复用 `ProcessSpec`（`crates/process/src/spec.rs:29`，设 `stdin/stdout = Piped`）spawn 子进程，把 `take_stdin/take_stdout`（`traits.rs:37-43`）接到 JSON-RPC 编解码器，完成 `initialize`/`authenticate` 握手，暴露异步 `AcpClient` trait（对齐 TS 侧 `packages/chat/src/client.ts:12` 四方法 + subscribe）
- 对 **UI/workbench 插件**：各自的 runtime 实现（见 4.9）
- **沙箱/隔离**：当前 `ProcessSpec` 已有 `cwd / envs`（`spec.rs:71-80`）可做基本隔离；更强隔离（资源限额、文件系统边界、网络禁用）需后续阶段引入（基于通用知识，见 6.x）。ACP 的 `additional_directories`（`initialization.rs:523`）已是文件系统边界协商机制，可作为权限模型的一部分
- **权限**：manifest 声明 permissions，activate 时校验/放行；运行时 ACP client 反向方法（`fs/*`、`terminal/*`、`session/request_permission`）需 handler 守门（见 4.7）

**建议落点**：新 crate `crates/acp-runtime`（ACP stdio 运行时，专注 agent 插件）+ `crates/plugin-runtime`（通用 plugin runtime 抽象 + registry + 状态机）。`acp-runtime` 复用 `ora-contracts::acp` 类型与 `ora-process`。

### 4.7 ACP agent 插件专属：协议实现与对话通路

**现状**：契约完整（`crates/contracts/src/acp/`），TS 消费者完整（`packages/chat`），但**Rust 侧 agent 运行时/传输零实现**——这是 `docs/research/plugin-acp-agent-conversation.md` 反复强调的核心缺口。本节是其插件管理视角的补充。

**差距（agent 插件对话通路，端到端）**：
1. **ACP stdio JSON-RPC 客户端**（Rust）：spawn agent 子进程 → `initialize`（`initialization.rs:20`）协商 `ProtocolVersion` + 交换 `ClientCapabilities`/`AgentCapabilities` → `authenticate`（`authentication.rs`）→ `session/new`（`session.rs:46`）→ `session/prompt`（`prompt.rs:16`）流式收 `session/update`（`session.rs:557`）→ `session/cancel`（`literals.rs`）。全仓无此实现。
2. **agent 插件发现与被调用**：registry 按 `plugin_id`（agent 插件的 manifest id，或 AgentDefinition 扩展的 id）解析启动规格 → spawn → 缓存 `AcpClient` 实例。`useCreateSession`（`use-workspace-mutations.ts:155`）当前调的 `chatStore.newSession` 在 Web 抛错、Desktop 走 mock，需替换为经新通道落到 Rust 运行时。
3. **agent 插件与 agent 的对话通路**：保留现有"`newSession` 返回 sessionId → `client.session.create({agentSessionId})` 持久化"接线（`use-workspace-mutations.ts:159-161`），仅替换传输后端；`Session.agent_session_id`（`session.rs:76`）字段天然承载 ACP session id。
4. **TS↔Rust ACP 通道**（替换 `createUnavailableAcpClient`，`client.ts:20`）：
   - 方案 A（Desktop 优先）：Tauri 命令 `acp_new_session/acp_prompt/acp_cancel` + `acp_update` 事件流（仿 `apps/desktop/src-tauri/src/lib.rs:28` 的 `invoke_handler` + `tauri::Manager` 事件），写 `createTauriAcpClient` 适配 `AcpClient` 接口
   - 方案 B（Web 优先）：`apps/web/server/src/routes.rs` 加流式端点（SSE/WS，如 `POST /api/sessions/{id}/prompt` + `GET /api/sessions/{id}/stream`），`handlers/` 新增 `acp` handler，写 `createHttpAcpClient`
   - 建议 Desktop 先行（进程 spawn 更自然），Web 走"后端代理 agent 进程 + SSE"（与 `docs/research/plugin-acp-agent-conversation.md` 第 202-205 条一致）
5. **client 侧 ACP 反向方法**：`session/request_permission`、`fs/read_text_file|write_text_file`、`terminal/*`（`literals.rs`）——agent 会反向请求，Ora 需实现 handler。`fs/*` 可接现有文件系统服务（`apps/web/server/src/service/file_system.rs`），`terminal/*` 可复用 `crates/pty`（`crates/pty/src/lib.rs:6` 的 `PtyRuntimeManager`），`session/request_permission` 需 UI 权限弹窗。

**判定**：agent 插件对话通路 = (1)+(2)+(3)+(4) 打通，最小闭环见第 7 节。

### 4.8 plugin SDK（面向插件作者的 API 表面）

**现状**：`packages/plugin-sdk` 仅有 `getNums/returnNums`（`host/index.ts:1-6`）+ JSON-RPC 行收发（`reader.ts:7`、`writer.ts:6`）+ console 保护（`console-guard.ts:1-19`）。SDK 给谁用尚不明确——当前形状像是给"被 host 调用的子进程"用（即插件侧），但方法集是任意的，未对齐 ACP。

**差距**：需要明确 SDK 的两类受众与 API 表面：
- **给插件作者（plugin side）**：
  - 对 agent 插件：SDK 应提供 ACP **agent server** 端实现脚手架——读 host 发来的 `initialize/session/new/session/prompt`、回 `InitializeResponse`/`NewSessionResponse`、流式发 `session/update` 通知、处理 `session/cancel`。当前 `getNums/returnNums` 的"读请求-写响应"骨架可演进，但**方法集必须重写为 ACP 方法名**（`crates/contracts/src/acp/literals.rs`），且需支持 notification（无 id 的单向消息，`session/update` 就是 notification）——当前 `HostRequest`（`protocol.ts:2-6`）只建模了有 id 的 request，**没有 notification 形态**，这是 SDK 协议层的硬缺口。
  - 对 UI/workbench 插件：各自的 extension point 契约（见 4.9）
- **host services / extension points / context 对象**：
  - host 暴露给插件的服务（host→plugin 反向调用）：agent 插件通过 ACP 的 `fs/*`、`terminal/*`、`session/request_permission` 调 host——SDK 需提供这些 client 侧方法的调用封装
  - context 对象：插件运行上下文（session id、cwd、capabilities、权限令牌）
- **SDK 与运行时的契约**：SDK 版本 ↔ `ora_runtime_version`（manifest 声明）绑定，不兼容拒绝激活
- **多插件类型用同一套 SDK 抽象 + 不同 capability 扩展**：SDK 应有分层——核心（manifest 声明、生命周期 hook、host 通信管道）+ 类型特定（agent 的 ACP server、UI 的视图注册、workbench 的工具栏注册）。当前 SDK 完全无分层。

**建议落点**：`packages/plugin-sdk` 重构为 `core`（共享）+ `agent`（ACP server）+ `ui`/`workbench`（后续）子模块；协议层 `protocol.ts` 扩展为 request/response/notification 三态并可选对齐 ACP。

### 4.9 扩展性：一套抽象支持 agent/UI/workbench

**现状**：无任何插件类型抽象。`AgentDefinition` 只描述 agent 元数据；UI/workbench 完全无概念。

**差距与设计方向**：
- **`PluginKind` enum**（`AGENTS.md:5` 用 enum）：`Agent / Ui / Workbench / Terminal / ...`，第一天就建，后续加变体即可
- **capability 组合**：每个 manifest 声明 `kind` + 该 kind 的 capabilities 子集。agent 插件复用 ACP `AgentCapabilities`（`initialization.rs:315`）；UI/workbench 各自定义 capability schema
- **接口隔离（ISP）**：`PluginRuntime` 是总 trait，但各 kind 有专属 trait（`AgentPluginRuntime`、`UiPluginRuntime`...），registry 按 kind 分发到对应实现。agent 插件的 `AgentPluginRuntime` 即 `AcpStdioRuntime`（4.6）
- **extension points**：UI 插件注册视图/面板，workbench 插件注册工具栏/命令——这些是 host 侧的注册点，与 agent 插件的"对话"是不同交互模型，需独立设计但共享 manifest/registry/lifecycle 基座
- **类型注册**：registry 支持 `register_kind::<K>()` 注册新类型的 runtime factory，实现开放扩展

**关键决策点**：agent 插件的启动规格承载方式（见 6.1）会显著影响扩展性——若把启动规格硬塞进 `AgentDefinition`，UI/workbench 插件就需另起一套 manifest，导致两套并行抽象；若统一到 `PluginManifest` + `kind`，则一套基座支持多类型（推荐）。

### 4.10 横切关注点

- **配置持久化**：无专属表。enabled/disabled、安装记录、插件级配置需新 migration + 仓储（复用 `crates/db` 模式，`catalog.rs:9`）。
- **事件总线**：无。ACP `session/update` 是流式通知，TS 侧用 `subscribe(listener)`（`client.ts:16`）；Rust 侧需一个事件通道把 update 推到 Tauri 事件 / SSE。建议轻量 `tokio::sync::broadcast` 或 trait 抽象。
- **日志**：`ora-logging`（`crates/logging`）已就绪，`tracing` 结构化日志，插件运行时应挂同一日志拓扑（`apps/desktop/src-tauri/src/lib.rs:71-72` 已 `register_gitlancer_logger`，可仿照加 plugin logger）。
- **错误处理**：`ora-backend` 有 `BackendError`（`crates/backend/src/error.rs`），ACP 有 `Error{code,message,data}`（`crates/contracts/src/acp/error.rs:19`，含 `AuthRequired/-32000`、`ResourceNotFound/-32002`、`RequestCancelled/-32800`）。插件管理层需自己的 error enum（激活失败、manifest 校验失败、依赖冲突...），且要把 ACP `ErrorCode` 翻译到 host 错误。
- **安全/权限模型**：无。manifest 声明 permissions → activate 校验 → 运行时反向方法守门（4.7 第 5 点）。沙箱见 4.6。当前 `ProcessSpec` 的 `cwd/envs`（`spec.rs:71-80`）是最低限度隔离。

---

## 5. 架构隐患与演进路径建议

### 5.1 当前抽象的隐患

1. **"agent"与"插件"概念分离且都不完整**：`AgentDefinition`（`agent_definition.rs:6`）只有元数据，无法启动；`plugin-sdk`（`host/index.ts:1`）是独立玩具，与 agent 无关。两者并行会导致"内置 agent 走 AgentDefinition 扩展、外部插件走 plugin-sdk manifest"的双轨制（见 6.1）。
2. **默认 id 硬编码**（`use-workspace-mutations.ts:17` 的 `"codex"`）：隐式单 agent，UI 选择器（`use-agents.ts:6`）列了 agent 但无法据其启动，与"启用 agent 插件"目标直接冲突。
3. **无类型抽象**：直接做 agent 插件管理而不先建 `PluginKind`，后续加 UI/workbench 时要么重写要么并行两套，违反 `AGENTS.md:6`（不保留兼容层）会更痛。
4. **状态机缺失**：`SessionStatus`（`session.rs:38`）只有 Running/Stopped，无法表达"已安装未启用""激活中""错误"——插件管理强依赖完整生命周期。
5. **SDK 协议层缺 notification**（`protocol.ts:2` 只有 request）：ACP `session/update` 是 notification，SDK 不支持就无法承载 agent 插件对话。
6. **agent 与传输耦合在 UI 侧**：`useCreateSession`（`use-workspace-mutations.ts:155`）直接调 `chatStore.newSession`，UI 既知道 session 语义又知道 ACP 调用——应通过插件管理 runtime 间接化。

### 5.2 推荐演进路径（先打通最小闭环，再补规格与多类型）

> 与 `docs/research/plugin-acp-agent-conversation.md` 第 7 节的顺序互补：那份按"ACP 传输优先"，本报告强调"插件管理抽象优先以免双轨"。

**阶段 0（基座，必做先于 agent 插件）**：
- 建 `PluginKind` enum + `PluginManifest` schema（`crates/contracts/src/plugin.rs`），`kind=Agent` 第一变体，capabilities 复用 ACP `AgentCapabilities`
- 建 `PluginLifecycleState` enum（4.5）+ 持久化迁移（`crates/db`）
- 建 `crates/plugin-runtime`：`PluginRuntime` trait + registry + scanner（本地目录）骨架
- 这一步让"agent 插件"从一开始就躺在统一插件抽象里，而非塞进 `AgentDefinition`

**阶段 1（agent 插件 + ACP 对话最小闭环，P0）**：
1. manifest 承载 agent 启动规格（command/args/env/transport=Stdio，6.1 决策为 manifest 而非 AgentDefinition 扩展）
2. 建 `crates/acp-runtime`：复用 `ora-process`（`spec.rs:29`）spawn + `ora-contracts::acp` 类型，实现 `initialize`/`session/new`/`session/prompt`（流式 `session/update`）/`session/cancel`
3. registry 按 `plugin_id` 缓存 `AcpClient` 实例（4.2）
4. TS↔Rust 通道：Tauri 命令 + `acp_update` 事件（方案 A），写 `createTauriAcpClient`
5. 替换 `use-workspace-mutations.ts:155` 的 mock/Unavailable，落真实 runtime；`DEFAULT_AGENT_ID`（`:17`）改为从 registry 解析默认 enabled agent
6. 用本地 mock agent 二进制或复用 `packages/mock-service` 逻辑做端到端验证（一致性测试 `packages/chat/src/conformance.ts` 兜底）

**阶段 2（合规性与 SDK，P1）**：
- 补 `initialize`/`authenticate` 握手 + client 侧反向方法（`fs/*`、`terminal/*`、`session/request_permission`）
- 重构 `packages/plugin-sdk`：core + agent 子模块，协议层加 notification，方法集对齐 ACP
- 插件配置持久化 + 事件总线
- agent 选择器 UI 接真实 registry

**阶段 3（多类型扩展，P2）**：
- 加 `PluginKind::Ui`/`Workbench` 变体 + 各自 capability schema + runtime factory
- SDK 加 ui/workbench 子模块 + extension points
- 沙箱/权限模型强化

**阶段 4（marketplace，P3）**：
- 远程仓库/扫描/安装（4.3/4.4 完整）、签名校验、依赖解析

### 5.3 关键决策点

1. **agent 启动规格承载：`PluginManifest` vs `AgentDefinition` 扩展**（6.1）——决定是否双轨
2. **TS↔Rust ACP 通道：Tauri 命令+事件 vs HTTP SSE**（4.7 第 4 点）——Desktop/Web 优先序
3. **SDK 是否第一天就分层**（4.8）——影响 UI/workbench 接入成本
4. **沙箱强度起点**（4.6）——进程级 cwd/env 隔离起步还是直接上更强隔离

---

## 6. 基于通用知识、非仓库现状的部分

> 以下内容仓库中**无对应代码或文档**，基于 ACP 官方规范（`https://agentclientprotocol.com/`，仓库 `crates/contracts/src/acp/session.rs:142` 已引用）与插件系统通用设计经验给出，**标注为建议而非现状**。

1. **ACP stdio 传输细节**：ACP 规定 JSON-RPC 2.0 over stdio（子进程 stdin/stdout），每行一条消息。仓库 `crates/contracts/src/acp/rpc.rs` 的 `JsonRpcMessage{jsonrpc:"2.0"}` 与规范一致，但"逐行帧、批量 `JsonRpcBatch`、`$/cancel_request`、notification 无 id"这些传输层行为需在 `crates/acp-runtime` 自行实现（契约已有数据形状，缺编解码 + 帧循环）。**这是基于契约可推断的，但运行时行为属待实现。**

2. **marketplace / 远程仓库协议**：仓库无任何远程插件源概念。以下为通用设计建议：marketplace 索引（插件 id/version/下载 URL/哈希清单）、客户端缓存目录、增量更新、离线回退、版本约束解析（semver range）。**完全为后续阶段建议，非现状。**

3. **沙箱与强隔离**：仓库 `ProcessSpec`（`spec.rs:29`）仅提供 `cwd/envs`，无资源限额、文件系统白名单强制、网络禁用。更强的隔离（cgroup/namespace on Linux、job object on Windows、WASM/容器化插件运行时）属后续安全增强建议。ACP 的 `additional_directories`（`initialization.rs:523`）可作为文件系统边界协商的协议层基础，但强制层需自建。**非现状。**

4. **plugin SDK 的 extension points 具体形状**：UI 插件注册视图、workbench 插件注册工具栏/命令的具体契约——仓库无相关代码。建议参考主流 IDE 插件模型（VSCode extension API 的 contribution points、JetBrains 的 extension points）设计，但需贴合 Ora 的 Tauri/React 架构。**非现状，后续阶段。**

5. **ACP `session/update` 的流式传输到前端的具体协议**：`session/update` 是 notification（无 id），TS 侧用 `subscribe(listener)`（`client.ts:16`）消费。Rust 侧经 Tauri 事件或 SSE 推送时，需保证顺序、背压、断线重连——这些传输质量属性契约层未规定，属运行时实现细节。**建议而非现状。**

---

## 附录：与现有调研文档的关系

`docs/research/plugin-acp-agent-conversation.md`（2026-07-22）已详尽覆盖 ACP 协议契约、AgentDefinition 模型、session/chat 接线、Web/Desktop 传输面、契约边界同步，并提出"补齐顺序"。本报告不重复其 ACP 传输细节，而是：
- 以**插件管理**为轴重新组织差距（manifest/registry/scan/install/lifecycle/SDK/多类型扩展）
- 指出其第 6 步"补全插件管理面"被低估——本报告论证它应**前置于** ACP 传输（阶段 0），否则会形成 agent/插件双轨
- 补充其未展开的 `PluginKind`/manifest/SDK 分层/事件总线/状态机等插件专属能力

两份报告互补：那份是"ACP 传输怎么接通"，本报告是"插件管理这层怎么建"。
