# Ora 插件管理 MVP 设计

> 状态：实现基线（Design v3）  
> 目标平台：Windows  
> 范围：插件管理、Agent 插件运行时与 Host↔Plugin 协议；不实现 Claude Code、Codex、OpenCode 等具体插件  
> Ora 事实基线：`D:\project\desktop`，commit `86f30938e206d5e395a5d5f9aefb2ba8c7779a01`  
> VS Code 参考基线：`D:\project\vscode`，commit `be6ed528a432f4a0e19606157b475ee7e580d074`  
> VS Code 分析报告输入：`D:\project\vscode\dev\extension-management-analysis\VSCode插件管理机制深度分析报告.md`，SHA-256 `A4BCF61477AD109167E3630CDC2AE4B92F10F022F23B58C496E79402A7279068`  
> Claude 项目记忆输入：`C:\Users\wanglongan\.claude\projects\D--project-vscode\memory\vscode-extension-mgmt-analysis.md`，SHA-256 `2C445F40CB7604EB61C2ABD22F1752B007BAE9C93A595FCC6F0FBED686C7BE6A`  
> SDK 对齐输入：`D:\chromeDownload\alignment_plugin_manager_sdk_0715.md`，SHA-256 `2BD6581640315FEB7794C1D4D020719A94C59DE592E32194D0EE61F26CD16043`  
> 审核日期：2026-07-16

本文使用以下规范词：

- **必须**：违反即不能通过 MVP 验收。
- **应当**：默认执行；偏离时必须有 ADR、风险说明与替代测试。
- **可以**：不影响 MVP 正确性的可选实现。

本文严格区分三类陈述：

1. **现状事实**：由当前 Ora 或指定 VS Code commit 的源码直接证明。
2. **借鉴**：VS Code 中值得保留的设计原则，但不表示 Ora 复制其实现。
3. **Ora 决策**：根据 Ora 的产品形态、技术栈与风险作出的设计选择。

专家数量、讨论轮数或“已经审过”不是正确性证据。本文是否可实施，以事实引用、状态不变量、故障恢复规则和可执行验收为准。

---

## 0. 结论先行

MVP 的关键裁决如下：

1. `PluginManager` 位于 Ora 的权威 Rust 后端进程中；不新增独立 manager daemon。每个运行中的插件使用一个独立 Bun 子进程。
2. 管理平面与运行平面分开：扫描、识别、安装、验证、启禁用、注册表和运行进程不是一个生命周期状态。
3. 候选目录不会因“被扫描到”而获得执行资格。只有从用户明确选择的本地目录复制到 Ora 受管目录、复验成功、安装记录完整且 effective-enabled 的插件才能运行。
4. 安装成功后默认 `DisabledByUser`。若产品提供“安装并启用”，也必须是“提交安装”与“显式启用”两个有序、可审计的操作。
5. MVP 的 manifest 识别两种插件：`agent` 与 `workbench`。只实现 `agent` executor；`workbench` 可以扫描、识别、验证、安装、列出、保持禁用和卸载，enable 明确返回 `UnsupportedKind`，绝不能退化为 Agent 进程执行或预先授权未来 executor。
6. Host↔Plugin 使用 stdio 字节流，帧格式固定为 `[length:i32 BE][type:i8][payload:length bytes]`。payload 是 UTF-8 JSON；不再使用换行分帧。
7. header 固定 5 字节。Rust/TypeScript 必须逐字段编码和解析；禁止把 Rust `struct { i32, i8 }` 的内存直接写上 wire，禁止用 `repr(C)`、`repr(packed)`、`transmute` 或 bytemuck 规避协议编码。
8. 运行时采用每插件单 actor、单 stdin writer、有界队列、增量 frame reader、独立 stderr drain 与 exit watcher。任何 buffer、队列、pending request 和日志都必须有上限。
9. Windows Job Object 进程树回收是运行 Agent 插件的前置门禁。当前 `ora-process` 只杀直接子进程，不能满足 MVP。
10. 插件进程隔离不是安全沙箱。插件和它启动的 Agent CLI 仍以当前 Windows 用户权限运行；完整性摘要只能检测安装后变化，不能证明发布者身份，也不能防御同一用户的主动竞态篡改。
11. 崩溃后绝不自动重放业务 RPC。未完成的非幂等调用返回 `UnknownOutcome`；仅注册快照和“仍需运行”的意图可以重建。
12. 当前硬编码的 `number_add`、固定插件 id、固定入口、每请求一进程、NDJSON reader/writer 和三态生命周期全部被新设计替换，不保留兼容层。
13. Bun 先执行 Host-owned private bootstrap；插件入口不是协议进程。Host↔bootstrap `wireVersion`、bootstrap↔插件 `pluginApi`、provider 的 Agent `contractVersion` 是三个独立版本轴；manifest 只声明后两者。
14. Agent v1 生命周期固定为 `$/initialize` Request/Response → `$/activate` Request/Response → Running；停止为 `$/deactivate` Request/Response → `$/exit` Notification → wait/kill。没有 `plugin.ready`、`$/ready` 或双重就绪状态。
15. Agent v1 不提供 Plugin→Host 业务 Request、`context.ora`、`globalState` 或单一 `workspaceState`。Host 下发 content-owner `storagePath`；Memento/项目状态需在未来以完整的新 `pluginApi` 契约加入，不能只塞一个无法更新的快照。

---

## 1. 证据基线与当前实现

### 1.1 当前已经证明的最小纵向切片

当前实现证明了以下链路可以工作：

1. Rust 构造固定的 JSON-RPC 2.0 `add` 请求。
2. `ora-process` 通过 `ProcessSpawner` / `ManagedProcess` 启动进程并移交 stdin/stdout/stderr。
3. Rust 向 stdin 写一条 `JSON + LF`，Bun SDK 按行读取。
4. Bun SDK 向 stdout 写一条 `JSON + LF`。
5. Rust 等进程退出，解析 stdout 第一行，校验版本与 request id，返回加法结果。

可保留的基础：

| 现有能力 | 证据 | 设计处理 |
|---|---|---|
| 静态 dispatch 的进程抽象 | `crates/process/src/traits.rs:9-52` | 保留；为进程树能力增加新 trait/实现 |
| `ProcessSpec` 使用 `OsString`、`PathBuf`、cwd、env 与 stdio 策略 | `crates/process/src/spec.rs:27-151` | 保留并增加明确的 `env_clear` 语义 |
| stdout/stderr 并发 drain 的思路 | `crates/plugin-manager/src/process.rs:55-60` | 保留；改成长期、有界、独立任务 |
| exit code、stderr、响应 id 和协议版本校验 | `crates/plugin-manager/src/manager.rs:181-252` | 泛化后保留回归语义 |
| Rust DTO 经 `ts-rs` 导出到 SDK | `crates/plugin-protocol/src/lib.rs:11-23` | 继续作为跨语言 DTO 单一真相源 |

### 1.2 当前实现不是插件管理 MVP

以下是必须作为迁移起点记录的事实：

- 插件 id 固定为 `"1"`，方法固定为 `"add"`，注册表在构造函数中硬编码：`crates/plugin-manager/src/manager.rs:11-13,33-44`。
- 唯一 public 业务方法是 `number_add`，request id 同样固定为 `"1"`：`manager.rs:68-97`。
- 启动命令固定为 `<data_dir>/bin/bun(.exe) <data_dir>/plugins/main.ts`，未按插件 id 或 manifest 选择入口：`manager.rs:144-161`。
- 每个请求启动一个新进程；写完即关闭 stdin，stdout/stderr 读取到 EOF：`crates/plugin-manager/src/process.rs:36-67,69-103`。
- Host 只解析 stdout 第一行，其余输出被忽略：`manager.rs:195-204`。
- 生命周期只有 `Registered / Running / Exited`，且仅是一个可互相覆盖的内存值：`manager.rs:15-28`。
- 没有 manifest、扫描、识别、安装、卸载、启禁用、持久化状态、动态注册表、长期进程句柄或崩溃恢复。
- `apps/web/server` 和 Tauri 壳均没有依赖或构造 `PluginManager`：`apps/web/server/Cargo.toml:10-24`、`apps/desktop/src-tauri/Cargo.toml:20-26`、`apps/desktop/src-tauri/src/lib.rs:10-24`。
- 仓库中没有真实插件入口；当前 happy path 由 fake process 单元测试证明，不是 Rust↔真实 Bun E2E。
- 当前 SDK reader/writer 是换行协议：`packages/plugin-sdk/src/internal/reader.ts:1-37`、`writer.ts:1-8`。
- 当前 `PluginAddParams` 和 `result: i64` 不是通用插件协议；`i64` 映射到 JavaScript `number` 时，超过 `2^53-1` 会失真：`crates/plugin-protocol/src/json_rpc.rs:4-35`。
- 当前未跟踪的 `packages/plugin-sdk/src/types/plugin-manifest.ts` 没有对应的 Rust `identifier.rs` / `manifest.rs` 实现，也没有被 `types/index.ts` 导出，不能当作已实现事实。
- SDK 测试门禁当前有错配：`Taskfile.yml:81` 使用 `@ora/plugin-sdk`，实际包名是 `@ora-space/plugin-sdk`（`packages/plugin-sdk/package.json:2`）；Node test glob 也不会递归覆盖 `tests/host/*`。

### 1.3 当前进程抽象的阻断问题

`TokioManagedProcess::kill` 明确只终止直接子进程，后代进程不会被回收；调用 `kill` 后仍需 `wait` 才能确认最终退出：`crates/process/src/tokio_process.rs:162-170`。

Agent 插件会启动 Claude Code、Codex、OpenCode 等子进程。若只杀 Bun，Agent CLI 及其后代可能继续运行。因此在 Windows Job Object 完成之前，Agent executor 必须 fail closed，不能以“已有 kill”宣称生命周期正确。

---

## 2. MVP 范围

### 2.1 必须交付

1. **scan installed**：扫描 Ora 受管安装根，输出有效插件、无效插件、兼容性状态和诊断。
2. **scan candidates**：扫描用户明确提供的候选根；只发现，不执行、不安装、不改变 enablement。
3. **identify**：对一个候选目录只读解析身份、版本、kind、入口、贡献点、兼容性、文件预算和风险摘要。
4. **validate**：严格校验 manifest schema、id、SemVer、Ora/pluginApi/Bun 兼容、Windows 路径、入口 containment、文件类型与安装收据。
5. **install authorized candidate**：消费 identify 生成、绑定 reviewed digest 的 `CandidateHandle`，把本地目录复制到同卷 staging，在 staging 上复验并要求摘要相等，再原子 rename 到受管目录。
6. **enable / disable**：持久化用户意图；disable 先关闭新调用入口，再停止运行实例并从运行注册表移除。
7. **register / unregister**：单写者 registry 以 revisioned delta 原子增删可运行贡献；不能由插件自行扩大 manifest 声明的贡献集合。
8. **start / stop**：按 plugin id 惰性启动或显式停止一个长期 Bun runtime；并发首次调用只能 spawn 一次。
9. **invoke / stream / cancel**：支持并发请求、流事件、deadline、取消、背压和进程退出清理。
10. **crash handling**：准确归因退出、失败所有 pending、记录崩溃窗口、禁止业务请求自动重放。
11. **uninstall**：逻辑禁用、停止、注销、rename 到 trash、异步安全删除；Windows 文件占用时保留可恢复 tombstone。
12. **restart reconciliation**：清理/恢复 staging、trash、pending removal、孤儿最终目录、损坏状态文件与不匹配收据。
13. **Workbench kind**：纳入 manifest、catalog、状态和管理 API；MVP 不执行。
14. **application integration**：进入 Rust 后端组合根、`AppState`、readiness、graceful shutdown 与 contract/API 层。
15. **observability**：结构化生命周期、安装、协议和进程日志；prompt、token、配置和密钥默认不记录。

### 2.2 明确后置

- 具体 Claude Code、Codex、OpenCode 插件实现。
- Workbench executor 与 UI/配置/IM 贡献点实现。
- plugin-to-plugin RPC broker 与 IM→Agent 调度授权。
- 插件市场、在线下载、发布者签名、恶意列表服务。
- 压缩包、VSIX-like 格式、archive extraction。
- 插件依赖、extension pack、安装脚本、`bun install`。
- 更新、多版本并存、active-version 切换与自动回滚。
- 多 profile、workspace/project scoped enablement。
- 远程宿主、Web Worker 宿主、多服务器与 affinity。
- 隐式 activation event 生成器和文件监听热加载。
- OS sandbox、AppContainer 或受限 token。
- Plugin→Host 业务 RPC、`context.ora`、Host-owned Memento/globalState 与项目状态 broker。

“后置”不表示为其预实现兼容层。扩展性来自清晰的 manifest 判别联合、贡献点注册器、executor trait、可序列化 command/event 和独立的管理/运行模块。

---

## 3. 核心不变量

1. 只有受管安装根内、receipt 完整、摘要匹配、manifest valid、runtime compatible 且 effective-enabled 的 Agent 插件才能注册和启动。
2. `scan` 与 `identify` 永远不执行插件代码、不安装依赖、不运行脚本、不写用户候选目录。
3. “候选插件”与“已安装插件”是两个集合；候选路径永远不能直接成为 runtime cwd 或 entry。
4. plugin id 是规范化 ASCII 小写值；身份比较、目录名、状态键和 registry key 使用同一个 canonical id。
5. 同一 plugin id 在 MVP 中最多有一个已安装实例、一个 mutation 和一个运行 generation。
6. 安装目录只有在 staging 完整复验成功、receipt 已生成后，才通过同卷 rename 变为可见。
7. 安装后默认 Disabled；缺失或损坏 enablement 记录同样 fail closed 为 Disabled。
8. manifest 是插件作者提供的不可变内容；Ora 安装元数据不写回 `package.json`。
9. 用户启用意图、effective enablement、catalog 状态和 runtime 状态互相独立。
10. PID、Starting、Running、pending request 和 generation 永不持久化。
11. `workbench` manifest-valid 不等于 runtime-supported；MVP 中它不得进入 Agent runtime registry。
12. 每个 runtime generation 只 initialize 一次、activate 一次、deactivate 至多一次；activate 成功 Response 是进入 Running 的唯一提交证据，Running 前不接受 Agent 业务请求。
13. 每个连接只有一个 stdin writer；并发任务不得直接写 pipe。
14. 每个 outbound request id 在当前连接与方向内唯一；result/error 只能完成一次。
15. timeout、cancel、response 与 process exit 的竞争由单一 runtime actor 裁决。
16. disable、uninstall、shutdown 一旦关闭 admission，持有旧 registry snapshot 的调用也必须因 enablement epoch/generation 复核而失败。
17. 任意 frame 的 length 在分配前完成有符号与上限校验。
18. 任意 partial frame EOF、未知 frame type 或 type/envelope 不匹配都终止当前连接且不尝试字节重同步；完整 Request frame 的 UTF-8/JSON/Request-envelope 错误按 §12.5 best-effort 回复后仍终止。
19. Host 意外退出或强制 stop 后，Bun 与其所有 Agent 后代最终必须退出。
20. 非幂等业务请求在 timeout、取消超时、连接断开或 crash 后绝不自动重放。
21. 卸载成功时 active record、runtime registry 和进程均不存在；trash 的物理删除可以异步完成。
22. 所有递归删除都不得跟随 symlink、junction 或其他 reparse point，目标必须受预期根目录约束。
23. identify 生成的候选授权绑定 source root identity 与用户看到的 id/version/tree digest；install/staging 任一复验不一致必须返回 `SourceChanged`，不能把“用户审核 A”升级为“安装 B”。
24. v1 Plugin→Host 业务 Request 集合为空；Plugin 只能发送 Host Request 的 terminal Response，以及对已声明 streaming Request 的 `$/stream` Notification。`$/cancelRequest` 只从 Host requester 发往 Plugin responder。未知 Plugin Request 或其他 Plugin Notification 一律 fatal，不为未来 API 预留可调用空壳。

---

## 4. 总体架构

### 4.1 进程拓扑

```text
Web frontend / Tauri WebView
        |
        | typed application contracts
        v
Ora Rust backend（权威进程）
  ├─ PluginManager facade
  ├─ CandidateScanner / InstalledScanner / Validator
  ├─ PluginInstaller / EnablementStore / Reconciler
  ├─ PluginCatalog（诊断视图）
  ├─ RuntimeRegistry（仅有效、兼容、启用、受支持的贡献）
  └─ PluginRuntimeSupervisor
       ├─ RuntimeActor: agent plugin A ── Bun ── Agent process tree
       ├─ RuntimeActor: agent plugin B ── Bun ── Agent process tree
       └─ ...
```

MVP 不新增独立 manager 进程。插件已经一插件一进程，独立 daemon 只会新增 app↔manager 的认证、协议、版本、启动顺序、重连和 split-brain，而不会进一步隔离插件代码。

未来只有在“UI 退出后 IM 必须继续运行”“多窗口共享同一插件实例”“manager 需要不同 Windows token/服务账户”等需求得到验证后，才重新评估 daemon。为未来迁移，manager command/event DTO 应可序列化；MVP 内部仍使用 Rust 类型和静态 dispatch。

### 4.2 分层与职责

| 层 | 职责 | 禁止事项 |
|---|---|---|
| Adapter | HTTP/Tauri 输入输出、认证、contract 映射 | 解析 wire、操作文件、持有进程状态 |
| Application | 用例编排、授权、事务边界 | 直接读插件目录或 child stdio |
| Management | 扫描、验证、安装、状态、catalog/registry | 执行插件业务代码 |
| Runtime | spawn、握手、RPC、背压、stop/crash | 修改安装事实或用户 enablement |
| Process | OS 进程、pipe、Job Object | 理解 JSON-RPC 或插件业务 |
| Private bootstrap | frame codec、dispatch、插件 ABI adapter | 允许插件作者直接控制协议 stdout，或把 private transport 作为 public SDK 导出 |

这与 VS Code“平台原子能力→工作台编排→UI→扩展宿主”的边界原则一致，但 Ora 不复制其 shared process、多宿主 affinity 或 remote/web running location。

### 4.3 推荐模块

`crates/plugin-manager/src/manager.rs` 当前约 800 行，新增功能不得继续堆入该文件。建议私有模块：

```text
ora-plugin-manager/
  src/
    lib.rs
    manager.rs              facade；只做编排
    config.rs
    error.rs
    identity.rs
    manifest.rs
    validation.rs
    discovery.rs
    candidate_authority.rs
    catalog.rs
    package_store.rs        # scan/mutation/maintenance coordinator
    registry.rs
    enablement.rs
    install/
      mod.rs
      digest.rs
      receipt.rs
      reconcile.rs
      safe_fs.rs
    runtime/
      mod.rs
      actor.rs
      state.rs
      supervisor.rs
      handshake.rs
      pending.rs
      stderr.rs
    transport/
      mod.rs
      frame.rs
      reader.rs
      writer.rs
```

`ora-plugin-protocol` 负责 wire/manifest/Agent contract DTO；`ora-process` 负责通用进程树和 pipe；具体 application adapters 位于 application/web server，而不是反向塞进 manager crate。

所有新 trait 必须有 doc comment，优先泛型和关联类型进行静态 dispatch；模块私有并在 `lib.rs` 明确 `pub use` 公共 API。

---

## 5. Manifest、身份与插件类型

### 5.1 Manifest 位置与所有权

插件根固定使用 `package.json`。Ora 字段放在顶层 `ora` 对象中，使 Bun/npm 元数据与 Ora schema 边界明确：

```json
{
  "name": "@ora-plugins/claude-code",
  "version": "0.1.0",
  "type": "module",
  "ora": {
    "manifestVersion": 1,
    "id": "ora.claude-code",
    "displayName": "Claude Code",
    "kind": "agent",
    "main": "dist/index.js",
    "engines": {
      "ora": ">=0.1.0 <0.2.0",
      "pluginApi": 1,
      "bun": ">=1.0.0 <2.0.0"
    },
    "contributes": {
      "agents": [
        {
          "id": "claude-code",
          "displayName": "Claude Code",
          "contractVersion": 1
        }
      ]
    }
  }
}
```

顶层 package.json 可以包含 Bun 所需的标准字段；`ora` 对象使用严格字段集合并拒绝未知字段。Ora 不执行 `scripts`，不运行 `bun install` 或任何 lifecycle hook。Agent v1 的可安装产物必须是 SDK pack 生成的独立 materialized artifact：规范构建等价于 `bun build src/index.ts --target=bun --format=esm --packages=bundle --outfile dist/index.js --metafile <file>`，manifest 的 `main` 指向单个 ESM 产物。除 Bun/JavaScript 内建模块外，运行依赖以及 public `defineAgentPlugin` helper 必须进入 bundle；private bootstrap/transport 绝不能进入插件 bundle。

SDK pack 必须使用 Bun metafile 加真正 ECMAScript parser 拒绝 external/dynamic unresolved package，禁止用正则表达式扫描 JavaScript，也不得执行待打包插件代码。MVP artifact allowlist 只有根 `package.json`、`dist/index.js` 与可选根级 `README*`/`LICENSE*`；暂不支持任意源码/runtime resource，未来需要时以严格 `ora.resources` 相对路径列表升级 manifest。安装器复验 artifact 没有 `node_modules`、workspace link、symlink/junction、native `.node` addon或安装后生成文件；`package.json` 中即使存在 npm `dependencies` 也没有安装语义。漏检依赖在 `--no-install` 下只能导致 `ActivationFailed`，不能回退为在线安装。

这一发布形态是安全边界与可复现性决策：Bun 的 isolated/workspace install 通常会产生链接结构，而安装器又必须拒绝可逃逸的 reparse point。Ora 因此不尝试“复制一个已安装 workspace”，而由 SDK pack/validate 命令生成无链接的 materialized artifact，安装器只验证和复制该 artifact。未来若支持 native addon 或外部依赖，必须先定义平台矩阵、摘要范围和无逃逸复制规则，不能放宽为跟随链接。

pack 输出到输入树之外的全新同卷 staging，完成 allowlist/metafile/parser/链接审计后才 no-replace rename 到目标；不得在源码目录原地删除再生成，也不得让 output 递归位于 input 内。除 Bun/JavaScript built-in 外，static import、dynamic import 与 require 的外部包一律拒绝。质量门禁把 artifact 移到没有源码、没有 parent `node_modules` 且含 Unicode 的路径后再启动，证明产物真正自包含；Host 安装器仍独立验证，不信任 pack 自报 metadata。

插件版本的唯一来源是顶层 `version`，必须是严格 SemVer；Ora 身份的唯一来源是 `ora.id`，不从 npm scope/name 推导。Agent v1 的顶层 `type` 必须为 `"module"`。同一语义不得在顶层与 `ora` 对象各存一份后再定义模糊覆盖规则。

Agent v1 不定义会造成“权限已被 OS 强制隔离”错觉的 manifest permission 字段。插件本身以当前用户权限运行，能直接调用 Bun/Windows API；Host 能真正控制的是启动环境、credential/executable/path 注入和未来 sandbox。v1 `PluginLaunchGrant` 只描述这些 Host 注入项，不包含尚不存在的 Host API 权限，也不信任插件自报。

安装器不得把时间戳、source、摘要或 enablement 写回 `package.json`。VS Code 确实会使用 manifest metadata 和 profile metadata，但 Ora 选择独立 host-owned receipt，以避免修改内容摘要并混淆作者事实与安装事实。

### 5.2 身份规则

`PluginId` 语法：

```text
^[a-z0-9][a-z0-9-]{0,62}(\.[a-z0-9][a-z0-9-]{0,62})+$
```

另外必须：

- ASCII 小写；不做 Unicode normalization 或 locale case-fold。
- 最长 128 字节。
- 拒绝 Windows 设备名 `CON/PRN/AUX/NUL/COM1..9/LPT1..9` 及其带扩展形式。
- 拒绝尾随点、尾随空格、冒号与 alternate data stream 语法。
- 目录项按 Windows case-insensitive 规则检测碰撞；发现碰撞时相关候选均 invalid。
- MVP 不引入 marketplace UUID，也不定义“有 uuid 比 uuid、否则比 id”的双重相等规则。
- 安装事务使用独立 `operation_id`；它不参与插件身份。

`AgentContribution.id` 与 wire `AgentProviderId` 是同一个 **plugin-local provider id**，语法独立于包含点号的 `PluginId`：

```text
^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$
```

它必须是 1..=63 bytes 的 ASCII 小写字符串，在同一 manifest 的 `contributes.agents` 中唯一；不接受点、下划线、首尾连字符、Unicode 或大小写折叠。示例 `claude-code` 合法。application 层全局 `AgentProviderKey` 是结构化的 `(PluginId, AgentProviderId)`，不得靠未转义字符串拼接来建立身份；wire 只发送 local id，并且 activate descriptor 必须与 manifest 的该 id 精确相等。

### 5.3 判别联合

Rust 模型必须使用带关联数据的 enum，而不是一个包含大量 `Option` 字段的 struct：

```rust
pub enum PluginKindManifest {
    Agent {
        main: PluginRelativePath,
        contributes: AgentContributions,
    },
    Workbench {
        contributes: WorkbenchContributions,
    },
}
```

语义：

| kind | schema valid | 可安装/管理 | MVP executor | 运行态 |
|---|---:|---:|---:|---|
| `agent` | 是 | 是 | 是 | 可进入 RuntimeRegistry |
| `workbench` | 是 | 是 | 否 | `UnsupportedKind`，不 spawn |

`workbench` 是一等插件类型，而不是 `ui/config/im` 三个布尔标记的容器。未来 UI、配置、IM 等能力应注册为 workbench contribution point 和对应 executor/validator；不能让所有 manager 分支到处 `match` 可选字段。

### 5.4 Workbench v1 占位 schema

MVP 只冻结最小、不可误执行的形态：

```json
{
  "name": "@ora-plugins/example-workbench",
  "version": "0.1.0",
  "ora": {
    "manifestVersion": 1,
    "id": "ora.example-workbench",
    "displayName": "Example Workbench",
    "kind": "workbench",
    "engines": {
      "ora": ">=0.1.0 <0.2.0"
    },
    "contributes": {
      "workbench": {
        "schemaVersion": 1
      }
    }
  }
}
```

Workbench v1 不接受 `main`、动态 capability 或 UI URL。这样它能被识别、安装和展示，但不会形成一个“看似支持、实际不安全”的半实现。新增具体 contribution 时必须升级并补对应 validator、Host grant 和 executor。

`kind` 与运行技术不能耦合：Agent v1 因被 private bootstrap 加载而必须声明精确 `engines.pluginApi = 1` 与 Bun range；Workbench v1 没有 executor，只声明 `engines.ora`。Rust Host↔private bootstrap 的 Frame/wire version 是 runtime asset 内部契约，不由插件 manifest 声明。未来某个 Workbench contribution 是否需要 Bun、WebView 或无进程静态资源，由对应 executor schema 决定，不由 `kind=workbench` 隐式推断。

### 5.5 Manifest 验证

验证必须区分：

- `ManifestValidity`：文件和 schema 本身是否有效。
- `RuntimeCompatibility`：当前 Ora/pluginApi/Bun/OS 是否支持。
- `RuntimeSupport`：当前 Host 是否实现此 kind/contribution 的 executor。
- `IntegrityStatus`：受管副本是否与 receipt 摘要一致。

解析器先读取有上限的最小 envelope（`ora.manifestVersion`、`ora.id`、`ora.kind`），再路由到对应版本的严格 schema。已知 v1 中的未知字段/错误类型属于 `ManifestValidity::Invalid`；未知 `manifestVersion` 则保留可诊断身份并标为 `RuntimeSupport::UnsupportedSchemaVersion`，不得安装、enable 或 spawn，也不能拿 v1 schema 将其误报为普通字段错误。

因此合法的 workbench 插件是：

```text
manifest_valid = true
runtime_compatibility = compatible
runtime_support = unsupported(kind=workbench)
```

不能将其误报为 manifest invalid，也不能按 agent 入口执行。

Manifest 限制：

| 项目 | MVP 默认 |
|---|---:|
| `package.json` 最大字节数 | 256 KiB |
| plugin id | 128 bytes |
| display name | 128 Unicode scalar values |
| entry relative path | 512 UTF-8 bytes |
| contribution 数量 | 64 |
| JSON nesting depth | 64 |
| 文件目录深度 | 64 |

文件数、目录深度、单文件/总字节数、manifest/frame/JSON 等预算必须集中在 typed `PluginLimits` 配置中，并由 production composition 与测试 fixture 注入；不得在 scanner、installer、runtime 各自散落不同魔法数字。安全上限只能由 Host policy 收紧，不能由 manifest 或 initialize result 提高。

`main` 必须是相对路径；拒绝绝对路径、盘符、UNC、空段、`.`、`..`、尾随点/空格和 ADS。join 后 canonical target 必须仍位于插件根，且是 regular file；路径链上的 symlink/junction/reparse point 均拒绝。Agent v1 还必须通过 materialized-bundle 检查：入口为 JavaScript bundle，包内没有 `node_modules`、reparse point、native addon 或 unresolved non-built-in import。

Ora app version、plugin API version 与 Bun version 是三个插件兼容维度。Agent v1 的 `engines.pluginApi` 必须精确等于 `1`，不做兼容层；wire version 只出现在 runtime asset receipt 与 Host↔bootstrap initialize DTO 中，用于发现错误打包/资产错配。

identify、staging commit 前、installed scan、enable 与每次 start 必须调用同一个 `PackageValidator` 实现，只是传入不同的判别联合 target：`Candidate`、`Staging { reviewed_identity, reviewed_digest }` 或 `Installed { receipt, installed_record }`。成功结果同时携带 normalized manifest、预算统计、tree digest、root/file identity 与 reparse/named-stream/hardlink audit proof；调用者不能只重算 SHA-256 而绕过其他检查。proof 只对本次已打开 handle/枚举快照有效，不持久化、不跨 mutation 复用；start 必须生成新 proof。

---

## 6. 磁盘布局、安装收据与状态

### 6.1 布局

```text
<ORA_DATA_DIR>/
  plugin-runtime/
    <runtime-version>/
      bun.exe
      plugin-host-bootstrap.js
      empty-bunfig.toml
      runtime-receipt.json
  plugins/
    .staging/
      <operation-id>/
    .trash/
      <operation-id>/
    <canonical-plugin-id>/
      package.json
      ... immutable package files ...
      .ora/
        receipt.json
  plugin-system/
    state.json
    state.previous.json
    manager.lock
  plugin-data/
    <canonical-plugin-id>/
      <content-owner-id>/
        ... mutable plugin data ...
```

约束：

- `plugins/` 是代码；`plugin-data/` 是可变数据，两者绝不相同。
- 插件 cwd 是受管代码根；SDK 提供独立 storage path。
- `ContentOwnerId` 使用 Windows 路径安全形式 `sha256-<64 lowercase hex>`；receipt 中的展示摘要可以写成 `sha256:<hex>`，但带冒号的摘要字符串绝不能直接作为目录名。
- 相同 plugin id 与相同 content digest 的重装可以复用原 owner；不同 digest 默认创建新 owner，不能自动读取旧 owner。数据迁移必须是未来的显式、可授权操作。
- owner 在首次 start 前以 create-new/no-follow 方式创建；复用同 digest owner 时，Host 必须从 pinned `plugin-data` root 逐级以 handle 验证 canonical ancestry、普通目录与无 reparse，并核对 state 中的 plugin id/content digest/content owner 绑定。失败时 start 返回 integrity diagnostic，不能把任意同名目录作为 `storagePath`。
- 源包不得包含保留目录 `.ora`。
- `plugins/.staging` 与最终目录必须在同一 volume，commit 才能使用目录 rename。
- manager 启动时必须构造并独占一个进程生命周期的 `ManagerLease`，其底层持有 `manager.lock` handle，直到权威 backend shutdown 才释放。拿不到锁时 readiness 失败，禁止出现两个 state writer。安装/启禁用/卸载等 mutation 只获取进程内的 per-plugin gate/state actor，不得再次获取非重入的 OS lock。
- Windows lease 不能只是“lock 文件存在”。Host 先以 `FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS` directory handles pin 住 canonical `ORA_DATA_DIR` 与其直接 child `plugin-system`，验证同一满足所需原子替换语义的本地 volume、无 reparse，并在 share mode 中排除 delete；随后用 `CreateFileW(GENERIC_READ | GENERIC_WRITE, share=0, OPEN_ALWAYS, FILE_FLAG_OPEN_REPARSE_POINT)` 打开 `manager.lock`，验证它仍是 pinned parent 的直接 regular child、link count=1，并终身持有。sharing/lock violation 映射 `DataDirInUse`；其他 ancestry/type/identity 错误是 bootstrap failure。文件中的 pid/start/nonce 只供诊断，Host crash 后遗留文件本身不表示锁仍被持有。
- 因竞争实例的 `share=0` 会阻止可靠读取 owner metadata，`DataDirInUse` 不承诺返回 owner；不得为改善错误文案放宽共享。双进程 E2E 覆盖竞争、Host crash 后 stale file 重获、lock target replace、junction/reparse 与 multilink。

### 6.2 安装收据

`.ora/receipt.json` 在 staging 中由 Host 生成，并随目录一起原子 commit：

```json
{
  "receiptVersion": 1,
  "pluginId": "ora.claude-code",
  "pluginVersion": "0.1.0",
  "source": "localDirectory",
  "installedAtUnixMs": 1784170000000,
  "contentDigest": "sha256:...",
  "fileCount": 42,
  "totalBytes": 1234567,
  "operationId": "..."
}
```

Receipt 不属于插件内容摘要；源目录中的 `.ora` 被拒绝，避免作者伪造 receipt。扫描与启动都必须验证 receipt 身份、版本与实际 manifest 一致。

### 6.3 内容摘要

摘要算法必须有无歧义规范：

1. 先哈入 domain separator `UTF8("ora-plugin-tree-v1") || 0x00`，避免同一字节序列被误作其他摘要协议。
2. 只包含 regular file，排除 Host 生成的 `.ora/`。
3. 路径转换为相对根的 UTF-8、`/` 分隔形式；无法稳定表示或发生 Windows case-fold 碰撞则拒绝。
4. 按规范化相对路径的 UTF-8 字节升序排序。
5. 每项编码为：`path_len:u32 BE | path_bytes | file_len:u64 BE | file_sha256:32 bytes`。
6. 对 domain separator 与全部项的连接结果再做 SHA-256，得到 tree digest。

必须限制文件数、单文件大小和总大小；建议 MVP 默认：10,000 文件、单文件 64 MiB、总计 512 MiB，均可通过明确配置类型调整。

摘要证明“受管副本是否改变”，不证明作者身份，也不能抵御具有同一 Windows 用户权限的攻击者在复验与 CreateProcess 之间主动竞态修改。文档、UI 和错误不得把它称为签名或 sandbox。

### 6.4 持久状态

`plugin-system/state.json` 只保存 Host 状态，不保存 runtime 状态：

```json
{
  "schemaVersion": 1,
  "revision": 12,
  "plugins": {
    "ora.claude-code": {
      "userEnablement": "disabled",
      "installation": {
        "state": "installed",
        "pluginVersion": "0.1.0",
        "contentDigest": "sha256:...",
        "contentOwner": "sha256-...",
        "installOperationId": "..."
      },
      "crashPolicy": {
        "state": "normal",
        "recentCrashesUnixMs": []
      }
    }
  },
  "pendingOperations": [],
  "launchGrants": {}
}
```

`pendingOperations` 是判别联合，不是无结构数组：

```rust
pub enum PendingOperation {
    Install(PendingInstall),
    Remove(PendingRemoval),
}

pub enum PendingInstallPhase {
    Prepared,
    FilesCommitted,
}

pub enum PendingRemovalPhase {
    Prepared,
    FilesMoved,
}

pub struct PendingInstall {
    pub operation_id: OperationId,
    pub plugin_id: PluginId,
    pub expected_version: PluginVersion,
    pub expected_digest: ContentDigest,
    pub candidate_audit_id: CandidateAuditId,
    pub phase: PendingInstallPhase,
}

pub struct PendingRemoval {
    pub operation_id: OperationId,
    pub plugin_id: PluginId,
    pub expected_digest: ContentDigest,
    pub install_operation_id: OperationId,
    pub trash_location: ManagedTrashLocation,
    pub phase: PendingRemovalPhase,
}
```

`InstalledRecord` 的最小字段是 `plugin_id`（来自 map key）、`plugin_version`、`content_digest`、`content_owner` 与 `install_operation_id`；缺少任一字段都不能完成 state↔receipt 校验。`CandidateAuditId` 指向本次用户选择/受信 discovery→identify→consume 产生的 Host 审计事实；它不是前端传入的任意字符串。`launchGrants` 与插件内容分离，键至少绑定 `plugin_id + content_owner + grant_schema_version`，详见 §14.3。

crash policy 也是判别联合：`Normal { recent_crashes_unix_ms }` 或 `BlockedByCrashLoop { recent_crashes_unix_ms, blocked_at_unix_ms }`。时间窗口有固定上限并在每次 state mutation 时裁剪，不能增长为无界日志。

写入规则：

- 单一 actor/串行提交者拥有内存 snapshot。
- state `revision` 是 `JsonSafeU64`，每次成功 mutation 精确加 1；达到上限时 subsystem fail closed。parser 拒绝 duplicate key、未知顶层/嵌套字段、非整数和回退 revision。
- `schemaVersion` 必须精确为 1；能够解析但版本未知返回独立 `StateVersionUnsupported`，不得把未来 schema 当 corruption 后回退到可能更旧的 backup。MVP 前的开发状态通过显式 reset/one-shot migration 转换；未来升级只能做备份后、事务化、单向 migration，不保留双读兼容层。
- mutation 先基于当前 revision 计算新 snapshot。若已有有效 primary，先把旧 snapshot 写入/flush/原子 replace 为 `state.previous.json`；再写同目录 primary temp、flush、原子 replace `state.json`。primary 成功后才发布内存 snapshot 与事件；backup 写失败则本次 mutation 不开始 primary commit。
- temp 文件名必须不可预测且在同目录 create-new，写完执行 `FlushFileBuffers`。目标存在时使用经封装/测试的 `ReplaceFileW`；首次创建使用 `MoveFileExW(MOVEFILE_WRITE_THROUGH)`。不得假定 `std::fs::rename` 在 Windows 可覆盖目标，也不得把官方不支持的 `REPLACEFILE_WRITE_THROUGH` 标志描述成 durability 保证。由于 `ReplaceFileW` 会尝试保留旧目标的 attributes/ACL/streams，replacement temp 与旧目标必须在调用前审计，replace 后还要从 pinned parent 重新 no-follow 打开结果，复核 direct-child identity、regular/link-count=1、无 named stream，并以 strict schema 重读核对 revision、operation id 与完整 snapshot；任一步不确定都进入下一条 `PersistenceUncertain`，不能把合并来的 ADS 当作无关 metadata。
- 若任一 replace API 返回错误、杀毒软件干预或重读结果使提交点无法判定，不发布候选 snapshot/事件，立即关闭全部 plugin admission并停止 runtime，返回 `PersistenceUncertain`；只有持有 ManagerLease 的 restart/bootstrap reconciliation 重新读取 primary/backup/磁盘事实后才能恢复 mutation。不能凭内存旧值继续声称持久状态可靠。
- 原子 replace 只解决单次文件替换，不替代单写者或进程锁。
- 打开 primary/backup/temp 时从 pinned `plugin-system` handle 证明它们是直接 regular child、无 reparse/named stream、link count=1；不按可替换 path 盲读/覆盖。
- primary 缺失或发生语法/完整性损坏时，只尝试 schema/revision 校验通过的 `state.previous.json`；从 backup 构造的 recovery snapshot 必须把全部插件强制为 `UserEnablement::Disabled`、清空全部 `launchGrants` 并按磁盘事实 reconcile，防止 stale backup 撤销最后一次 disable 或 credential/path grant revoke。MVP 不自动恢复、解析或复用旧 grant；用户必须重新执行显式 `set_launch_grant`。
- 上述 special recovery **不得**对 invalid primary 调用 `ReplaceFileW`，否则旧目标的 ADS/attributes 可能再次合入 clean snapshot，形成永久恢复循环。持有 ManagerLease 与 pinned `plugin-system` parent 时，StateStore 先以 no-follow、`DELETE` access、无 `FILE_SHARE_DELETE` 的 handle 固定 invalid `state.json` identity；用 `SetFileInformationByHandle(FileRenameInfo)`、`ReplaceIfExists=FALSE` 与相对 pinned parent 的不可预测 `state.corrupt.<operation-id>.<nonce>.json` 名把该目录项原子隔离。目标名碰撞只允许有界换 nonce 重试；identity 变化、隔离失败或无法证明原 primary 已消失都使 readiness fail closed。若 primary 本来缺失则跳过隔离。唯一有效 backup 在整个步骤中保持原位，不得被 invalid primary 覆盖。
- 隔离完成后，StateStore 把 disabled、无 grant 的 recovery snapshot 写入同目录 create-new temp，`FlushFileBuffers` 成功后，在确认 `state.json` 仍不存在的前提下使用**不带** `MOVEFILE_REPLACE_EXISTING` 的 `MoveFileExW(MOVEFILE_WRITE_THROUGH)` 首次安装；随后从 pinned parent 重新 no-follow 打开、复核 identity/regular/link-count/streams 并 strict 重读完整 snapshot。隔离与首次安装之间崩溃会留下“primary missing + valid backup”，下一次 bootstrap 重走同一幂等恢复；安装后验证失败或提交点不确定则 readiness fail closed。只有重读验证成功才允许发布内存 snapshot。
- `state.corrupt.*` 永不作为恢复输入。有效 primary/backup 裁决完成后，只能按 StateStore 的 exact quarantine-name grammar 有界扫描；每项经 pinned-parent、no-follow、direct-child identity 校验后用 handle-based disposition 安全删除。未知名、identity 无法证明或正被占用的项保留并报告，不能为清理而放宽目录安全规则。两份 state 都不可用时进入 `StateCorrupt`：所有 final 只成为 `UntrackedInstall/RecoveryRequired` candidate，不自动创建 installed record，也不猜测用户启用意图。
- state temp 永远不是恢复输入。primary/backup 裁决和必要的 recovery commit 完成后，持有 ManagerLease 的 bootstrap 只可清理由 StateStore exact temp-name grammar 创建、且通过 direct-child/regular/no-reparse/no-named-stream/link-count=1 验证的残留 temp；未知文件或无法证明身份的 temp 留在原处并报告，不影响为安全而读取已验证 primary，但不得无界反复扫描/删除。
- PID、Running、Starting、pending RPC 的瞬态细节不写入 state。crash timestamp 与 `crashPolicy` 由 state actor 持久化；进入 `blockedByCrashLoop` 后，重启应用不会隐式复位，只有显式 `reset_crash_loop` 或 disable→enable 用户动作才能清空窗口并恢复为 `normal`。

以上 crash consistency 只承诺通过 process-kill/fault-injection 证明的提交点；没有 power-loss harness 时不声称抵御硬件/卷缓存突然断电。任何不确定恢复仍以 disabled/untracked 为唯一安全默认。

---

## 7. Catalog、Enablement 与 Registry

### 7.1 Catalog

`PluginCatalog` 是诊断视图，保留所有受管候选：

```rust
pub struct CatalogEntry {
    pub plugin_id: Option<PluginId>,
    pub location: PathBuf,
    pub manifest: Option<PluginManifest>,
    pub validity: ManifestValidity,
    pub compatibility: RuntimeCompatibility,
    pub support: RuntimeSupport,
    pub integrity: IntegrityStatus,
    pub diagnostics: Vec<PluginDiagnostic>,
}
```

坏 manifest、engine 不兼容、receipt 缺失或摘要失败的目录不能静默消失；它们必须出现在 catalog 中供 UI 解释，但没有运行资格。

### 7.2 用户意图与 effective enablement

持久化的只有：

```rust
pub enum UserEnablement {
    Enabled,
    Disabled,
}
```

最终状态是派生值：

```rust
pub enum EffectiveEnablement {
    Enabled,
    Disabled(EffectiveDisableReason),
}

pub enum EffectiveDisableReason {
    User,
    InvalidManifest,
    IncompatibleEngine,
    UnsupportedKind,
    IntegrityMismatch,
    PendingRemoval,
    MissingInstallFiles,
    Policy,
    CrashLoop,
}
```

primary reason 使用严格全序：`PendingRemoval > MissingInstallFiles > IntegrityMismatch > InvalidManifest > IncompatibleEngine > UnsupportedKind > Policy > CrashLoop > User > Enabled`。实现可同时返回按此顺序排列的全部 diagnostics，但 registry/admission 只使用唯一 primary reason。高优先级原因通常不改写用户意图；但 MVP 不允许对 unsupported workbench 记录新的 Enabled 意图，避免未来 executor 上线后静默获得执行资格。未来支持某种 Workbench executor 时，必须由用户重新执行 enable/授权动作。

### 7.3 Registry

`RuntimeRegistry` 只包含 `EffectiveEnablement::Enabled` 的受支持 AgentContribution。它是 immutable snapshot + revision：

```rust
pub struct RegistrySnapshot {
    pub revision: u64,
    pub agents_by_provider: HashMap<AgentProviderKey, RegisteredAgent>,
    pub plugins_by_id: HashMap<PluginId, RegisteredPlugin>,
}
```

全局 provider key 为 `<plugin-id>/<agent-local-id>`。多个插件可以实现同一 Agent contract；调用必须显式携带 target plugin id/provider key，不能依赖全局唯一 `agent.*` method。

注册规则：

1. catalog/effective enablement 变化由 registry 单写者计算 delta。
2. 同 canonical plugin id 重复时两者均不运行，不能“后扫描覆盖前者”。
3. 同一插件内 agent local id 重复为 manifest invalid。
4. `register`/`unregister` 是 Host 内部动作；activate result 只能确认 manifest 已声明的 provider id/contractVersion，不能动态扩权。
5. 每个 delta 产生 monotonic revision 事件；先提交 snapshot，再通知消费者。
6. runtime start/invoke 在真正执行前再次核对 enablement epoch 与 registry generation，防止旧 snapshot 绕过 disable/uninstall。

`PackageStoreCoordinator` 为 installed-root 视图提供 read-snapshot permit，并为 commit/repair 提供 write/maintenance permit。install/uninstall 从首次写 `PendingOperation`、改变 final/trash 可见性起，直到 state commit、catalog refresh 与 registry reconcile 完成一直持有 write permit；普通复制到不可执行 staging 可以在此前并行。`scan_installed` 以 read permit 同时取得目录视图与 StateStore revision，绝不会把“final 已 rename、state 尚未提交”发布成一个稳定 snapshot。bootstrap/显式 maintenance repair 取得 exclusive maintenance permit，普通 scan 不删除或收养文件。

锁顺序固定为 `per-plugin mutation gate → PackageStoreCoordinator permit → StateStore command`；StateStore actor 和 registry writer 不反向等待上游锁。每个 registry reconcile command 携带 catalog revision 与 state revision；writer 记录已应用 source revisions并拒绝任一维度回退，防止长 scan 用旧 enablement 覆盖较新的 disable。所有跨 JSON/TypeScript 边界的 revision、epoch、seq 与计数使用 `JsonSafeU64`（0..=9,007,199,254,740,991）；达到上限 fail closed，不 wrap 或静默损失精度。

VS Code 值得借鉴的是“扫描描述、enablement 过滤、registry delta、按需启动”三个独立步骤，而不是把 installed、registered、running 合成一个状态。

---

## 8. 扫描、识别与验证

### 8.1 三个边界

- `scan_candidates(root_ids)`：一层枚举 Host 配置的候选根；结果是带 opaque `SelectionHandle` 的 `CandidateSelection`，只读、不执行。
- `identify(selection_handle)`：消费一次性 native-picker/discovery selection，解析、验证、计算 tree digest 并生成 server-side `CandidateHandle`；不产生安装事实。
- `scan_installed()`：扫描 `plugins/<id>/` 与 receipt/state，构建 catalog 并发现待 reconcile 项。

候选发现永远不能直接加入 RuntimeRegistry。`SelectionHandle` 是 session-bound、短 TTL、opaque、单次 identify 可消费的 Host 记录；`CandidateHandle` 是单次 install-attempt 可消费的下一阶段记录，至少绑定 canonical source path、Windows volume/file identity、expected id/version/tree digest、session、TTL 与 audit id。前端只收到 handles、展示字段和 digest，不收到/回传权威路径。install 必须以 no-follow handle 复核根 identity；staging 必须复验并与 identify digest 相等。identity 或 digest 不同均为 `SourceChanged`，要求重新 identify。

### 8.2 候选验证顺序

1. 使用 `symlink_metadata`/Windows reparse 信息验证根目录类型。
2. 读取有界 `package.json` bytes；验证 UTF-8、JSON object、duplicate key 策略与 nesting depth。
3. 严格反序列化 `ora` schema，拒绝未知字段。
4. 规范化 id、版本、kind、entry、engine 与 contribution。
5. 枚举文件；拒绝 symlink、junction、reparse point、named data stream、hardlink count≠1、设备文件、socket 等非 regular file。
6. 检查 Windows 名称、case collision、路径/目录深度、文件数与大小预算。
7. 验证 `main` containment 与 regular-file 身份。
8. 产出结构化 diagnostics，不执行任何文件。

### 8.3 Installed 验证

在候选验证之上增加：

- 最终目录名必须等于 canonical plugin id。
- `.ora/receipt.json` 存在、schema 正确、id/version 匹配。
- 重算 tree digest 与 receipt 相等。
- state install record 与 receipt 一致。
- 目录不是 `.staging`/`.trash`，也不是 pending removal。

扫描期间发现 invalid 只会关闭其 admission、从 RuntimeRegistry 移除并报告；不会自动删除用户可见数据。

---

## 9. 安装与启动恢复

### 9.1 安装算法

`install_authorized_candidate(candidate_handle)` 按 plugin id 串行，并遵循：

```text
ConsumeCandidateHandle
  -> AssertManagerLeaseHeld
  -> AcquirePluginMutation
  -> CreateSameVolumeStaging(operation_id)
  -> CopyUnnamedStreamsIntoFreshRegularFiles
  -> ValidateStagingAgain
  -> RequireStagingDigestEqualsCandidateDigest
  -> WriteReceipt
  -> AuditWholeStagingIncludingReceipt
  -> AcquirePackageCommitPermit
  -> CommitPendingInstall(Prepared)
  -> RenameStagingToFinal
  -> CommitPendingInstall(FilesCommitted)
  -> CommitStateAsInstalledDisabledAndClearPending
  -> RefreshCatalogAndRegistry
```

详细规则：

1. source 必须由用户明确选择或来自显式 discovery root；identify 消费 selection 并签发 `CandidateHandle`，install 的首次尝试再原子消费 handle。过期、重放、跨 session 或字段不匹配均为 `CandidateHandleInvalid`。
2. source 不能位于 Ora 的受管 `plugins/`、`.staging`、`.trash` 或 `plugin-data/` 下。
3. 复制使用 handle-based no-follow 打开 source 的 unnamed data stream，并在 staging 新建 regular file；不使用会保留 ADS/hardlink 关系的目录复制 API，不执行脚本、不运行 Bun、不解析 shell 命令。
4. 打开 source 根时必须复核 CandidateHandle 绑定的 volume/file identity；复制每项使用 no-follow handle 并在读取前后复核 file identity/type。staging 验证以 staging 的实际 bytes 为准，且 id/version/tree digest 必须与 consumed CandidateHandle 完全相等。目录在 identify 后发生 identity 或内容变化均返回 `SourceChanged`，不会安装“重新验证后恰好仍合法”的另一份内容。
5. 同 id 已安装时 MVP 返回 `AlreadyInstalled`；同 id/version 但不同 digest 也不能静默覆盖。
6. rename 前 final 必须不存在；跨 volume staging 是实现错误。
7. receipt 以 create-new 写入并 flush 后，必须再次审计 staging root、全部目录/包文件与 `.ora/receipt.json` 的 reparse/named-stream/link-count/identity；重算排除 `.ora/` 的 tree digest仍须相等。随后取得 package commit permit，再在 final rename 前提交 `PendingInstall::Prepared`，其中 operation id、plugin id/version、expected digest 与本次 `CandidateAuditId` 均匹配。rename 后提交 `FilesCommitted`，最后一次 state commit 才创建 installed+disabled 并删除 journal。
8. final rename 成功而后续 state 提交失败时，操作返回 `RecoveryRequired`，目录不可运行；启动 reconciler 只有在 final receipt 与 matching `PendingInstall` 全部一致时才完成为 **installed + disabled**。没有 matching intent 的 final 标为 `UntrackedInstall/RecoveryRequired` 并保持隔离；不得仅凭可由同用户伪造的 receipt 自动收养。
9. 安装成功事件只在 state 提交与 catalog 刷新后发布。
10. 安装 API 只产生 installed+disabled，不提供 `InstallDisposition::EnableAfterInstall` 组合操作。调用方必须在收到成功的 `InstalledPlugin` 后显式调用 enable，避免“安装已提交、enable 失败”被一个 `Result` 错误掩盖。

### 9.2 为什么不是“完整文件系统事务”

VS Code 使用临时下载/解压目录并 rename 到最终目录，这是可见性原子化的重要借鉴；但最终目录与 profile 记录不是一个跨资源事务。因此 Ora 必须定义显式提交点和恢复规则，不能把“用了 rename”描述成整个安装流程原子。

### 9.3 启动 reconciliation

在应用 readiness 之前执行，且必须幂等：

| 发现状态 | 恢复动作 |
|---|---|
| `.staging/<op>` 且无 matching pending install | 删除/移入 trash；永不运行 |
| pending install + staging，final 缺失 | 校验 operation/id/digest；可安全重试 rename，或中止并清 journal；永不直接 enable |
| pending install + matching final receipt | 完成 installed + disabled，清 journal；`Prepared`/`FilesCommitted` 都按实际文件事实收敛 |
| final + 有效 receipt + 无 matching state/journal | 标 `UntrackedInstall/RecoveryRequired`，不运行；仅显式 repair 可隔离或重新授权导入 |
| state installed + final 缺失 | 标 `MissingInstallFiles`，effective disabled |
| receipt 缺失/损坏 | 标 invalid，移出 RuntimeRegistry |
| digest 不匹配 | 标 `IntegrityMismatch`，不运行 |
| pending removal + final 存在 + trash 不存在 | 继续 stop，再按 journal 的受管 trash location rename |
| pending removal + final 缺失 + matching trash | 验证/补齐 removal marker；删除 install record、清 tombstone，再异步删除 trash |
| pending removal + final 和 matching trash 同时存在 | `RecoveryRequired`；二者都不覆盖/删除，等待显式 repair |
| pending removal + final/trash 均缺失 | 删除 install record、清 tombstone并报告恢复诊断；不得复活 |
| `.trash` 残留且无 active pending removal | 仅在 `.ora/removal.json` marker 与 installed receipt/受管 location 可验证时有界异步删除；否则隔离并报告 |
| primary state 损坏、backup 有效 | 从 backup 恢复 installation facts，但全部强制 disabled；再按磁盘事实 reconcile |
| primary/backup 均损坏 | 所有 final 标 `UntrackedInstall/RecoveryRequired`；只有显式 repair/re-authorize 可建立 installed+disabled |

运行期普通 scan 只读，不得顺手删除当前 mutation 的 staging。reconcile 只在 bootstrap 或持有 maintenance gate、确认无 active mutation 时运行。

---

## 10. Enable、Disable、Uninstall 与 Shutdown

### 10.1 Enable

1. 读取最新 catalog entry。
2. 若 manifest invalid、engine incompatible、integrity mismatch 或 pending removal，返回结构化原因，不改为 enabled。
3. MVP 对 `workbench` 的 enable 返回 `UnsupportedKind`，不修改 `UserEnablement`；它可安装、列出、保持/设置 disabled 和卸载，但不能预先授权未来 executor。
4. 提交 state revision。
5. 重算 effective enablement 与 registry delta。
6. Agent 默认惰性 start；enable 本身不必立即 spawn。

### 10.2 Disable

按 plugin id 串行：

1. 增加 enablement epoch 并立即关闭 admission。
2. 提交 `UserEnablement::Disabled`。
3. 从 RuntimeRegistry 移除并发布 revision delta。
4. 取消尚未发出的启动/请求；已发出的请求进入 cancellation/stop 流程。
5. graceful stop，超时后 kill 整个 Job，等待/reap。
6. 返回时必须保证没有可接受新请求的 runtime。

用户禁用不是 crash，不进入 crash counter。

### 10.3 Uninstall

```text
CommitPendingRemoval(operation_id, expected_digest, trash_location) + CloseAdmission
  -> Unregister
  -> StopAndReapProcessTree
  -> RenameFinalToTrash
  -> WriteAndFlushRemovalMarker
  -> CommitPendingRemoval(FilesMoved)
  -> RemoveInstallRecordLaunchGrantAndClearPending
  -> PublishCatalogDelta
  -> AsyncSafeDeleteTrash
```

若 Windows 文件占用导致 rename 失败：

- 保持 `PendingRemoval`，effective disabled；
- 不删除 state tombstone，不恢复运行；
- 返回可诊断的 `RemovalPending`；
- 下次启动/maintenance 重试。

每个 removal journal 使用唯一 operation id 和位于受管 `.trash` 根下的路径安全 location。rename 成功是文件提交点；随后在 trash 的 `.ora/removal.json` create-new/flush Host marker，绑定 removal operation id、plugin id、expected digest 与原 install operation id。即使 FilesMoved 状态写入失败，reconcile 也能用“final 缺失 + matching trash receipt/marker + tombstone”完成逻辑卸载；crash 正好发生在 rename 与 marker 之间时，只有 matching tombstone+installed receipt 才可补写 marker。只有 install record 与 tombstone 已在同一 state commit 中删除后，带有效 marker 的 trash 才可进入无状态异步清理。

`final` 与同一 operation 的 trash 同时存在属于冲突，两个都不覆盖/删除并返回 `RecoveryRequired`。没有 active tombstone且缺失/无效 marker 的 orphan trash 只隔离并报告，不能按目录名猜测后自动删除。这样任一崩溃点都不会把插件复活，也不会让不明目录获得递归删除授权。

默认卸载只删除代码，保留 `plugin-data/<id>/<content-owner-id>`。删除数据是单独的 `remove_plugin_data(plugin_id, DataRemovalScope)` 命名操作，不使用含义不明的 bool 参数。

### 10.4 应用 shutdown

1. Tauri 只在 application-level `ExitRequested` 启动 single-flight shutdown；普通窗口 close 不等于全局退出。先关闭 WebView/plugin HTTP admission，停止接受新的 handler、mutation、start/invoke，并 drain 已接受 handler。
2. 关闭 `PackageStoreCoordinator` 新 permit；在途 copy/delete/repair 只在声明的安全检查点响应 cancellation。StateStore 继续服务已取得 permit 的事务，直到提交或留下可由 bootstrap 识别的 staging/trash/pending 状态。
3. 关闭全部 runtime admission，并发向 Running generation 发 §11.6 graceful stop；Starting 的迟到 worker 仍由 cleanup continuation 独占 generation。
4. 各 phase grace 到达后 terminate Job，但保留 Job/process handles，继续等待 direct reap、`ActiveProcesses=0`、stdout EOF、stderr drain、writer/reader/exit task 与 package workers。
5. 只有 HTTP handlers、package mutations、StateStore/registry/runtime actors、blocking workers 和全部 tree 都 settled，才 flush state/event/log、join owner tasks并正常释放 ManagerLease。write-capable actor/supervisor 与 lease 由同一个不可 clone 的 BackendRuntime owner 持有；`Arc<PluginApi>` 在 shutdown 后只能返回 `BackendShuttingDown`，不能拥有独立 writer/lease。
6. hard deadline 到达仍有本地 worker或 actor可能写盘时，关闭剩余 Job handles触发 KILL_ON_JOB_CLOSE，记录未收敛 phase，并**保持 ManagerLease 到整个 Ora 进程无条件退出**；不得 Drop lease后回到仍存活的 Tauri event loop，也不得发布 clean/uninstall-complete/tree-empty 假终态。下一次启动按 journal/磁盘事实 reconcile。

---

## 11. Runtime 与生命周期

### 11.1 单一所有者状态机

```rust
pub enum RuntimeState {
    Stopped,
    Starting { generation: u64, spawn_token: SpawnToken },
    CancellingStart { generation: u64, spawn_token: SpawnToken, reason: StopReason },
    Initializing { generation: u64, pid: u32 },
    Activating { generation: u64, pid: u32 },
    Running { generation: u64, pid: u32 },
    Stopping { generation: u64, pid: u32, reason: StopReason },
    CleanupPending { generation: u64, process_tree: ProcessTreeToken, reason: StopReason },
    Draining { generation: u64, primary_trigger: DrainTrigger, progress: DrainProgress },
    Crashed { generation: u64, exit: ProcessExit },
    CrashLoop { recent_crashes: u32 },
}

pub enum DrainTrigger {
    DirectProcessExit,
    TreeBecameEmpty,
    StdoutBoundaryEof,
    StdoutReadFailure(IoFailure),
    WriterFailure {
        stage: WriterFailureStage,
        failure: IoFailure,
    },
    ProtocolFailure(ProtocolFailure),
    ProcessTreeFailure(ProcessTreeError),
    StopEscalation,
}

pub enum WriterFailureStage {
    Request,
    TransportCancel,
    SessionControl,
}

pub struct DrainProgress {
    pub direct_process: DirectProcessDrain,
    pub stdout: PipeDrain,
    pub stderr: PipeDrain,
    pub tree: TreeDrain,
}

pub enum DirectProcessDrain {
    Awaiting { pid: u32 },
    Reaped { exit: ProcessExit },
}

pub enum PipeDrain {
    Open,
    BoundaryEof,
    Failed(PipeDrainFailure),
}

pub enum PipeDrainFailure {
    Io(IoFailure),
    Protocol(ProtocolFailure),
}

pub enum TreeDrain {
    Active,
    Empty,
}
```

`WriterFailureStage` 由 actor 创建 command 时确定，writer 不从 method string猜测：所有 JSON-RPC Request frame（lifecycle、ordinary Agent、safety Agent）为 `Request`，`$/cancelRequest` 为 `TransportCancel`，`$/exit` 与 best-effort session-fatal diagnostic response 为 `SessionControl`。`primary_trigger` 只在 generation 首次进入 Draining 时赋值且永不替换；后到 fatal event 只能推进兼容的 `DrainProgress`、结算该 event 的 command owner，并写入每 generation 有界的 metadata-only secondary diagnostic（满后计数丢弃），不能替换 primary trigger 或已锁存的 caller cause。

转换：

```text
Stopped -> Starting -> Initializing -> Activating -> Running
             |                                      |
             +-> CancellingStart -> CleanupPending -+-> Stopped
             |                                      |
             +-- start/runtime failure ------------> Draining -> Crashed
Initializing/Activating/Running -- stop -----------> Stopping -> Draining -> Stopped
Crashed -- explicit/new demand + backoff ----------> Starting
Crashed -- threshold reached ----------------------> CrashLoop
```

所有 runtime command、frame、deadline、cancel、writer ack、direct-exit 与 tree-empty event 进入同一 actor mailbox。`DrainProgress` 用 closed enums 表达 EOF-first 时尚未取得 `ProcessExit`、exit-first 时 stdout 尚未 EOF 等真实中间态，不用伪造 exit 或一组含糊 `Option<bool>`。generation 防止旧 reader/exit watcher 的迟到事件污染新进程。

### 11.2 Start single-flight

- 同一 plugin id 多个并发 first invoke 共享一个 start future，只 spawn 一次。
- 单个 waiter 被取消不取消其他 waiter，也不终止已启动的共享 runtime。
- start 前再次验证 receipt digest、effective enablement、registry revision 和 runtime support。
- disable/uninstall/shutdown 或 spawn deadline 在 process handle 尚未返回时，把 `Starting` 原子转换为 `CancellingStart`；该 generation 继续独占 single-flight slot，不能创建第二 generation。
- 底层 spawn worker 必须始终回报同一个 `spawn_token`。迟到成功的 tree handle 立即进入 `CleanupPending`，执行 Job terminate + wait/reap，绝不进入 initialize；迟到失败则收敛到 Stopped。只有 cleanup 完成才能释放 generation slot。

### 11.3 Host-owned bootstrap

Host 不直接执行插件 entry，而执行当前 pinned runtime 中固定的 private `plugin-host-bootstrap.js`。bootstrap 先独占 stdin/stdout、安装 console guard 和 Frame router；只有收到 `$/activate` 后才 dynamic import 已验证 entry，并调用 §13.2 的 `AgentPluginDefinition`。插件作者 SDK 与 private transport/runtime 是两个包边界，public exports 不含 bootstrap、reader、writer、RpcClient 或 dispatcher。

MVP 不在首次运行时联网下载 Bun。仓库新增一个 Host-owned runtime asset manifest，锁定 Bun 版本、每个 Windows artifact 的 SHA-256、bootstrap/config schema 与 wire version；构建流水线校验并把 `bun.exe`、预打包 bootstrap、空 bunfig 和 runtime receipt 作为 Tauri resource 随应用发布。BackendRuntime 启动时把资源复制到同卷 staging、复验摘要后 rename 到版本化目录；每次 spawn 前复验 active runtime receipt/关键文件。缺失或损坏时从随应用的只读资源恢复，恢复失败则 readiness 为 `PluginRuntimeUnavailable`，绝不回退到系统 PATH 上的 bun。

应用升级通过新增版本目录并原子切换 active runtime reference；已有 generation 继续持有旧 runtime lease，最后一个 lease 释放后才清理旧目录。bootstrap 必须是构建期 bundle，不依赖用户或插件 `node_modules`。这项供应链只保证“与 Ora 构建锁定内容一致”，不应表述为第三方发布者签名。

启动必须：

- 直接执行受控 `bun.exe`，不经过 `cmd.exe`、PowerShell 或 shell 字符串。
- cwd = 受管插件根。
- 使用 pinned Bun，`--no-install` 禁止 runtime auto-install，`--no-env-file` 禁止自动加载候选 `.env`。
- 必须显式传 `--config <versioned-runtime>/empty-bunfig.toml`；该文件位于插件根与用户目录之外并随 Ora 固定。不能让 Bun 默认读取插件 cwd 的 `bunfig.toml`，因为其中的 `preload` 可在 bootstrap 之前执行代码、产生副作用或污染 stdout。
- 插件运行依赖必须已进入 `dist/index.js` 单 bundle；Host 不解析 `node_modules`，不运行 `bun install`。
- 清空继承环境后按明确 allowlist 构建环境；`PATH`、用户目录和 Agent 所需 token 是否暴露必须由 Host-owned launch grant/config 决定，不能继承整个 Host 环境。
- stdout 仅供 frame writer；bootstrap 捕获原始 writer 后阻止普通 `console.*`/`process.stdout.write` 进入协议。此保护仍不是 sandbox，恶意同用户插件可尝试直接访问 fd 1；污染会触发 fatal protocol violation。

Bun `--no-install`、`--no-env-file` 与 `--config` 行为必须针对 Ora 锁定的 Bun 版本做真实 E2E，不只依赖文档假设。launch environment 使用 clear+allowlist，因此未明确批准的 Bun 配置环境变量也不得继承。

### 11.4 Windows 进程树

`ora-process` 应新增面向树生命周期的 trait，例如：

```rust
pub trait ProcessTreeSpawner {
    /// Spawns a process tree that is contained before plugin code can execute.
    type ProcessTree: ManagedProcessTree;

    /// Creates the managed tree or fails without allowing an uncontained child to run.
    fn spawn_tree(&self, spec: ProcessSpec) -> std::io::Result<Self::ProcessTree>;
}

/// Terminates a generation without taking exclusive ownership away from exit watchers.
/// Clone must share the same RAII inner; it must not create an unmanaged duplicate Job handle.
pub trait ProcessTreeController: Clone + Send + Sync + 'static {
    /// Requests termination of every process currently assigned to this Job.
    fn terminate_tree(&self) -> Result<(), ProcessTreeError>;
}

/// Separates one tree into capabilities that the supervisor can drive concurrently.
pub struct ProcessTreeParts<Stdin, Stdout, Stderr, Controller, DirectExit, TreeEmpty> {
    /// Pipe endpoints transferred exactly once to the three dedicated I/O tasks.
    pub stdio: PluginStdio<Stdin, Stdout, Stderr>,
    /// Actor-owned controller used for graceful escalation and fatal termination.
    pub controller: Controller,
    /// Owned future that reports and reaps the direct Bun process independently.
    pub direct_exit: DirectExit,
    /// Owned future that proves the Job has no active processes.
    pub tree_empty: TreeEmpty,
}

/// Owns one generation's direct process, complete Job hierarchy, and stdio pipes.
/// Implementations split exactly once into concurrently usable, owned capabilities;
/// the shared inner Job handle remains alive until every capability is dropped.
pub trait ManagedProcessTree {
    type Stdin: AsyncWrite + Unpin + Send;
    type Stdout: AsyncRead + Unpin + Send;
    type Stderr: AsyncRead + Unpin + Send;
    type Controller: ProcessTreeController;
    type DirectExit: Future<Output = Result<ProcessExit, ProcessTreeError>> + Send + 'static;
    type TreeEmpty: Future<Output = Result<(), ProcessTreeError>> + Send + 'static;

    /// Consumes the aggregate owner and transfers stdio, termination, direct-exit,
    /// and tree-empty capabilities exactly once to the generation supervisor.
    fn into_parts(self) -> Result<ProcessTreeParts<
        Self::Stdin,
        Self::Stdout,
        Self::Stderr,
        Self::Controller,
        Self::DirectExit,
        Self::TreeEmpty,
    >, ProcessTreeError>;
}
```

MVP 目标为 Windows 10/11，冻结唯一的 `PROC_THREAD_ATTRIBUTE_JOB_LIST` 创建路径；不实现会形成第二套行为的 suspended-assign fallback，更不允许普通 `Command::spawn` 后 assign。具体不变量：

1. 为每个 generation 创建不可继承的 Job，设置 `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`，明确不设置 `BREAKAWAY_OK/SILENT_BREAKAWAY_OK`；在 Job 为空且 CreateProcess 前关联 generation 独占的 completion port/key。port 只作 wakeup，不能作为 tree-empty 证明。
2. `CreatePipe` 不能满足 async overlapped Host I/O，因此用不可预测的 `\\.\pipe\ora-plugin-<nonce>-<stream>` 本地名称、当前用户/SYSTEM 限制 DACL、`PIPE_REJECT_REMOTE_CLIENTS | PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT` 创建三组 named-pipe pair；Host server ends 以 `FILE_FLAG_OVERLAPPED` 创建并在 CreateProcess 前完成/挂起可收敛的 connect，child client ends 是适合 stdio 的同步 byte-stream handles。只有 child stdin-read/stdout-write/stderr-write 标记 inheritable；Host ends、Job、port、process-management handles 都不可继承。`STARTUPINFOEXW` 同时设置 `PROC_THREAD_ATTRIBUTE_JOB_LIST` 与 `PROC_THREAD_ATTRIBUTE_HANDLE_LIST`，`STARTF_USESTDHANDLES` 的 `hStdInput/hStdOutput/hStdError` 必须指向同一三个 child handles，`bInheritHandles=TRUE`，flags 至少含 `EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT`。HANDLE_LIST 是继承白名单，不能代替 hStd* 绑定；pipe message mode 永远不能成为 framing 的第二来源。
3. 一个 RAII `SpawnTransaction` 拥有 Job/port、两组 attribute payload、地址稳定的 attribute-list backing、全部 pipe ends，以及成功后暂存的 `hProcess/hThread`。传给 `UpdateProcThreadAttribute` 的 backing 在 `DeleteProcThreadAttributeList` 前地址/生命周期稳定；任一失败出口由单一 Drop 路径关闭/释放，不在 early return 手工拼资源清理。CreateProcess 成功后删除 attribute list、关闭 `hThread` 和 parent 中三个 child-stdio 副本，再把 direct process、Job/port/key 与三个 Host pipe ends移交 `ManagedProcessTree`；漏关 child 副本会使 EOF 永不到达，属于测试失败。
4. `into_parts` 返回的 controller、direct-exit future 与 tree-empty future 共享一个 RAII inner，但互不长期借用 `&mut ManagedProcessTree`：actor 可在 direct watcher 等待 Bun 退出时并发调用 `TerminateJobObject`，direct exit 与 tree empty 分别上报。controller terminate 后仍保留 inner Job/port/process handles；tree-empty watcher 在进入 wait、收到 completion wakeup、周期 poll 与 deadline 前后都查询 `QueryInformationJobObject(JobObjectBasicAccountingInformation)`。只有 direct child 已由独立 watcher reap且 `ActiveProcesses == 0` 才完成 generation cleanup。ZERO 消息可能迟到/丢失，generation/key 不匹配的消息丢弃；超时返回 `TreeCleanupTimeout`，cleanup continuation 仍持有全部 parts/inner 直到空树或进程退出。
5. Host crash/drop 由 KILL_ON_JOB_CLOSE 兜底。若目标 OS、父 Job/nested-Job policy、attribute API 或 async pipe 创建无法满足上述路径，start 在任何 Bun/bootstrap/plugin code 执行前返回 `TreeKillUnavailable`，不降级。Job 只约束实际创建进其 hierarchy 的进程；WMI/计划任务/外部服务等 brokered execution 不在此保证内，因此仍不能宣称 sandbox。

| 资源 | CreateProcess 前 owner | 成功移交后 owner | 必须关闭/释放点 |
|---|---|---|---|
| Job + completion port/key | `SpawnTransaction` | `ProcessTreeParts` 的共享 RAII inner | tree empty 且各 owned capability join/drop 后；Host crash 由 Drop/KILL_ON_JOB_CLOSE |
| attribute list + payload backing | `SpawnTransaction` | 无 | CreateProcess 返回后先 `DeleteProcThreadAttributeList`，再释放 backing |
| `hProcess` / `hThread` | transaction 暂存 | direct-exit watcher / 无 | `hThread` 立即关闭；`hProcess` direct reap 后关闭 |
| Host stdin/stdout/stderr ends | transaction | dedicated writer/reader/stderr task | task EOF/stop 后关闭 |
| child stdio copies | transaction + child inheritance | child | parent 在成功后立即关闭；失败由 transaction 关闭 |

`TokioProcessSpawner` 可继续用于普通 leaf process 和测试，但不能作为生产 Agent runtime 的“已满足进程树保证”实现。

### 11.5 Reader、Writer、stderr 与 exit

每 generation 至少五个协作者：

1. **writer task**：唯一 stdin owner；消费容量受控的 writer command，编码完整 frame并 `write_all`，回 actor `FrameWritten` ack。该名称不冒充 peer acknowledgement 或额外 durability。
2. **reader task**：只做增量 framing、UTF-8/JSON/envelope 校验并把 typed event 投给 actor；不得在 reader 中等待业务 handler。
3. **stderr task**：始终尽快按 byte chunk 读取到 EOF，绝不因限速或慢日志 sink 暂停 pipe 读取；限速只作用于日志发布和 ring-buffer 保留，超额 bytes 立即丢弃并累计 `dropped_bytes`。不得用无界 `read_line`，单个无 LF 的 stderr 输出也只能占固定 ring 容量。
4. **direct-exit watcher**：独立等待/reap Bun direct process，立即向 actor 报告 exit；它不等待仍存活的孙进程，因此可实现真实的 exit-first 排列。
5. **tree-empty watcher**：独立等待 Job `ActiveProcesses == 0`，与 controller terminate 可并发；reader 到 stdout EOF 且 direct/tree 状态收敛后，actor 再失败剩余 pending，避免“进程先退出但 pipe 中已有最终响应”被误判。

禁止对长期 stdout/stderr 使用 `read_to_string`。

actor 收到 process/tree exit、stdout boundary EOF、reader/writer fatal error 任一事件即关闭 admission并进入 `Draining`，只用 `DrainTrigger + DrainProgress` 的 closed state记录进展：

- EOF-first 在 Running 中是 crash，立即请求终止 Job；partial EOF/decoder error 同样先终止再 drain。
- Initializing/Activating/Running 中的非预期 direct-exit 必须在 actor 处理该 event 时立即调用 controller terminate 整个 Job，保留 Job/process/pipe handles并继续消费 stdout 到 boundary EOF；不能让仍活着且持有 stdout handle 的 Agent 后代运行到普通 drain deadline。Stopping 中预期 direct-exit 不立即升级，继续等待 tree grace；到期才产生 `StopEscalation`。
- exit-first drain 仍接受该 generation pipe 在 boundary EOF 前形成的完整合法 frame；reader 对单 generation 保持 FIFO，使 pipe 中已读 response 先于随后 EOF event 被 actor 处理。Windows pipe 不携带“由哪个后代、在 direct-exit 前后何时写入”的证明，因此这只是同一受信插件 generation 的正确性规则，不是进程身份认证；立即 terminate Job 的目的同时是关闭后代写端、限制竞态窗口并保证 EOF 可达。
- 只有 `stdout_done && process_tree_done` 后才完成剩余 pending；stderr 继续无阻塞排空，但受 shutdown hard deadline 约束。
- Stopping 中的 boundary EOF 只有在预期 stop 且不存在 partial frame 时才是 clean。

Bun SDK 与 Rust 对称：唯一 stdout writer + 有界 frame/byte queue。所有生产者只能提交完整 frame；`process.stdout.write(frame)` 返回 false 时 writer 等待 `drain`，插件业务代码禁止直写 stdout。

reader→actor 使用有界 typed-event lanes，并为 writer ack、terminal response、deadline、EOF/exit 等 control event 预留 frame slot 和 byte budget；writer ack 的不可借用 slot 不得被 stdout data flood 占满。Rust actor 只路由，不 await 业务 handler；Rust inbound 只接受 Host pending 的 Response 与合法 `$/stream`，两者都不能无声 drop。private Bun bootstrap 的 ordinary Agent handler executor 有界，ordinary admission 满时对合法 ordinary Request 回复 `ServerBusy(-32010)`；`agent.cancelConversation` 使用容量至少等于 active-turn 上限的独立 safety executor/slot/parsed-byte reserve，不与 ordinary executor 竞争，也绝不返回 `-32010`。safety admission、dispatch 或 terminal 失败立即成为 runtime-fatal，由 Host terminate Job。bootstrap 另有 inbound control reserve保证 lifecycle 与 `$/cancelRequest` 可达。v1 Rust Host 没有 Plugin→Host handler executor，因为 Plugin Request 集合为空。

### 11.6 Stop 与 crash

Stop：

1. 关闭 admission。
2. actor 先为每个尚无更早 intent 的 active ordinary request 登记 `HostStop` termination intent，再发送 transport cancel 并在受 `min(stop_cancel_total, D_inv)` 约束的 grace 内 drain；Queued request 直接以 `Cancelled/NotWritten` 移除。若无法 drain，进入 `StopEscalation`/terminate，不并发调用插件 `deactivate()`。已接受的 `cancelConversation` safety action 不改成 HostStop，仍按 §13.1 收敛。
3. 仅在 activate 曾成功且协议仍健康时发送 `$/deactivate` Request，等待其 bounded Response；error/timeout 记 diagnostic，但不能阻止后续回收。
4. 若 private transport 仍健康（包括 initialize/activate 尚未成功的 Starting），发送 `$/exit` Notification，等待完整 frame 的 `FrameWritten` ack 后关闭 Host stdin。bootstrap 收到 exit 后关闭自己的 admission、排空 stdout writer并退出。
5. exit 写失败、stdout 协议失败或 exit deadline 内未退出则 terminate Job。
6. 等 stdout EOF、stderr drain、direct child reap 与 Job tree empty，原子完成所有 pending。

Crash：

- 任一 generation 的非预期 EOF/exit、协议/握手失败和 tree 异常都进入失败/清理；只有已到 Running 的非预期 runtime 失败计入 crash window，initialize/activate 失败作为 start failure 单独观测。用户 disable/uninstall/shutdown 不计 crash。
- runtime actor 为**没有自身 termination intent 的 pending**在首个 fatal/drain trigger 锁存 `FatalSettlementCause::ConnectionLost { stage }` 或 `ProcessExited { exit_code? }`。映射唯一固定为：boundary EOF/reader I/O/protocol fatal → `ResponseRead`；writer 的 `Request/TransportCancel/SessionControl` failure → `RequestWrite/TransportCancelWrite/SessionDrain`；`StopEscalation` 或无直接 exit 证据的 tree/watcher 异常 → `SessionDrain`；direct-exit/tree-empty 证据 → `ProcessExited`。该映射只用于**首个** fatal trigger；`Draining.primary_trigger` 与每个 pending 的 fatal cause 都是 write-once。`StopEscalation` 必须在调用 controller terminate **之前、同一 actor turn**为全部无自身 intent 的 bystander 锁存 `ConnectionLost(SessionDrain)`，结果不依赖 OS exit watcher 先后。一旦任一 fatal cause 为某个 bystander 锁存，admission 已关闭，后到 caller cancel/HostStop/`D_inv` 只回收 waiter，不能创建 intent或改写 cause。这样也覆盖“另一个 invocation 的 cancel/grace 或 write failure 迫使 Host kill Job”的情形。后续 drain event 可补齐 exit code 或 secondary-failure diagnostic，但不能切换 variant。失败 writer command 所属 request 仍先按其自身 `WriteFailed`/first-intent/safety 矩阵收敛；若它没有 intent 且早先 fatal cause 已锁存，`bytes_written` 只决定 `NotWritten/PossiblyWritten`，caller 分类仍使用早先 cause，不改成后到 writer stage。只有 writer failure 本身是 primary trigger 时，无自身 intent 的 owner/bystander 才使用 writer 映射出的 fatal cause。已有 ExplicitCancel/HostStop/Backpressure/HardDeadline/safety intent 保留其自身 fallback，不被 fatal cause 改写。无 termination intent 的 pending 在 generation pipe drain 完成后按 §12.6 fatal-settlement 矩阵收敛；不是一律 `PluginExited`。所有路径都绝不自动重放。
- MVP 使用每插件滑动窗口，例如 5 分钟内 3 次进入 `CrashLoop`；阈值和 backoff 是配置值并有测试。
- 不做无人需求时的主动自动重启。普通 crash 在 backoff 后可由新的显式 start/invoke 创建 generation；一旦持久化为 `CrashLoop`，start/invoke 均 fail closed，必须调用 `reset_crash_loop` 或执行 disable→enable 才复位，且复位提交独立 state revision/audit event。

所有 deadline 使用注入的 monotonic clock，并以 phase-specific typed config 冻结起止点：spawn（创建 start worker→取得受管 tree 或完成迟到清理）、initialize/activate/deactivate（对应 Request Written→terminal Response）、invocation（API accept→terminal outcome）、transport cancel（actor accept→cancel Written→handler settled）、business cancel（safety admission→active turn terminal Written→cancel Response）、exit/tree（exit Written→direct reap+ActiveProcesses=0）与 stderr/stdout drain。默认数值必须由真实 Windows/Bun E2E 定标；不能用一个“plugin timeout”同时覆盖这些不同风险，也不能因 caller 放弃 waiter而取消共享 start/tree cleanup。

---

## 12. Wire Protocol v1

### 12.1 分层

```text
stdio byte stream
  -> FrameCodec: length + type + payload bytes
  -> UTF-8 / JSON parser
  -> type-specific JSON-RPC envelope validation
  -> handshake / request router / notification dispatcher
  -> Agent Contract DTO
```

目标格式是 Ora 自有协议。VS Code 源码证明了显式宽度、大端整数、独立 framing 和 RPC 分层的价值，但其 RPC `MessageBuffer` 与 PersistentProtocol header 都不是 Ora 的 5-byte 格式，本文不声称逐字复制 VS Code。

### 12.2 精确帧格式

```text
offset  size  field
0       4     length: signed i32, big-endian
4       1     type: signed i8
5       N     payload: exactly length bytes of UTF-8 JSON
```

规范：

- `HEADER_LEN = 5`。
- `length` **只表示 payload 字节数**，不包含 4-byte length，也不包含 1-byte type。
- 总帧字节数 = `5 + length`。
- `length` 按用户要求是 signed `i32`；必须先拒绝 `length <= 0`，再拒绝 `length > MAX_PAYLOAD_BYTES`，最后才转换为 `usize` 或分配。
- MVP `MAX_PAYLOAD_BYTES = 8 * 1024 * 1024`。
- payload 长度是 UTF-8 bytes，不是 Rust char 数，也不是 JavaScript `string.length`。
- payload 后不追加 LF/CRLF；JSON 内的合法空白或换行不影响 framing。
- payload 必须是一个顶层 JSON object；不支持 JSON-RPC batch array；不允许 BOM。

`type` 稳定码：

| i8 | 名称 | payload envelope |
|---:|---|---|
| 1 | Request | JSON-RPC request |
| 2 | Response | JSON-RPC success/error response |
| 3 | Notification | JSON-RPC notification |

`0`、负值和 `4..127` 全部保留。MVP 收到未知 type 即 `ProtocolViolation` 并终止连接，不猜测、不跳过、不扫描“下一个像 header 的位置”。

encoder 与 decoder 使用同一约束：encoder 也必须拒绝空 payload、超过 8 MiB 的 payload和未知 type，不能生成 decoder 永远不会接受的 frame。

### 12.3 Padding、对齐与端序

以下做法全部禁止：

```rust
#[repr(C)]
struct Header {
    length: i32,
    frame_type: i8,
}
```

然后把 `Header` 的内存直接写出是不正确的，原因是：

1. Rust/C ABI 通常会把该结构补齐到 8 字节，尾部可能有 3 字节 padding；wire 要求恰好 5 字节。
2. struct 内存使用 native endianness；wire 要求 big-endian。
3. padding bytes 可能未初始化，布局也不是跨编译器/语言协议。
4. `repr(packed)` 会引入未对齐访问风险，且仍不解决端序和协议演进。

Rust 必须显式编码：

```rust
let mut header = [0_u8; 5];
header[..4].copy_from_slice(&payload_len_i32.to_be_bytes());
header[4] = frame_type_i8.to_be_bytes()[0];
```

Rust 解码必须使用 `i32::from_be_bytes` 和 `i8::from_be_bytes`。Bun/TypeScript 使用 `Buffer.writeInt32BE` / `readInt32BE` 与 `writeInt8` / `readInt8`。不得用 u32/u8 读取后再靠 cast 猜测 signed 语义。

JSON DTO 通过 serde/JSON 字段序列化，不受 Rust struct padding 影响；padding 问题只发生在错误的 raw-memory wire encoding。

### 12.4 增量解析

stdio 是字节流，不保留写边界。一次 read 可能只包含 header 的一个字节，也可能包含多个完整帧和下一个 partial frame。

Rust reader：

1. 在帧边界读取第一个 byte；0 bytes 表示 clean boundary EOF。
2. `read_exact` 累积剩余 3 个 length bytes。
3. 解析 signed i32 并在 allocation 前校验。
4. 读取 1-byte signed type；未知 type 可立即失败。
5. `read_exact` 恰好读取 length payload bytes。
6. 校验 UTF-8、JSON 与 envelope；循环下一帧。

Bun reader 使用 `Buffer`/`Uint8Array` + cursor/ring buffer；数据不足时等待，足够时切出一帧。不得对每个 chunk 反复 `Buffer.concat` 造成 O(n²)。

EOF 语义：

- frame boundary EOF 不是一条消息。Running 时视为连接异常；Stopping 且预期退出时可视为正常。
- header、type 或 payload 中途 EOF 一律 `UnexpectedEof` + fatal protocol violation。
- one-shot 路径若保留，也必须复用同一 codec，不能回退到 `read_to_string`/`lines()`。

### 12.5 JSON-RPC envelope

Request：

```json
{"jsonrpc":"2.0","id":"h:1","method":"agent.sendMessage","params":{}}
```

Response：

```json
{"jsonrpc":"2.0","id":"h:1","result":{}}
```

或：

```json
{"jsonrpc":"2.0","id":"h:1","error":{"code":-32602,"message":"invalid params","data":{}}}
```

Notification：

```json
{"jsonrpc":"2.0","method":"$/stream","params":{"id":"h:1","seq":1,"value":{}}}
```

严格规则：

- `jsonrpc` 必须等于 `"2.0"`。
- envelope 层的普通 id 是最长 128 UTF-8 bytes 的非空 string；这使 Host 能对带任意合法 string id 的方向违约 Request 回显诊断。session role 层规定 v1 只有 Host 能创建 Request，且合法 Host id 必须是 `h:<JsonSafeU64>`；Plugin 创建任何 Request（即使伪造 `h:`）都在回复 `-32601` 后 fatal。Response 原样回显其 Request id。唯一 null 例外是下面两种 session-fatal 诊断 Response。
- Request 有 id+method，无 result/error；`params` 可以省略，存在时由 method 的 typed DTO 校验。
- Response 有 id，result/error 恰好一个，无 method。
- Notification 有 method，无 id/result/error；`params` 可以省略，存在时同样受 method DTO 约束。
- frame type 必须与 envelope 形态一致。
- JSON 树任意 object depth 都不允许 duplicate key；默认最大 depth 为 64，检测必须发生在转换为通用 `Value`/DTO 之前，不能让“最后一个字段覆盖前一个字段”进入授权判断。
- 协议控制整数（protocolVersion、seq、error.code 和未来计数）必须是 schema 规定的整数并落在 JavaScript safe-integer 范围；`error.code` 进一步限制为 i32。params/result 中的 number 由具体 DTO 决定：整数仍受 safe range 约束，只有 DTO 明确声明时允许有限浮点数；NaN/Infinity 不是 JSON。超大整数使用规范化十进制 string。
- 对完整、length/type 均合法的 Request frame：UTF-8/JSON/duplicate-key/depth 失败时，使用 control reserve 和短 write deadline best-effort 回复 type=Response、`id:null`、`-32700`；JSON 可解析但不是合法 Request envelope 时同样回复 `id:null`、`-32600`。无论诊断是否写成，随后都终止 session。该 null id 不匹配 pending。
- 声明为 Response/Notification 的 frame 若 UTF-8/JSON/envelope 无效或与声明 type 不匹配，直接 fatal，不发送反向 error；声明为 Request 的 mismatch 已由上一条统一返回 `-32600` 后 fatal。unknown type、非法 length 与 partial EOF 同样直接 fatal。
- 已识别的合法 Request 中，未知 method 回复 `-32601`，typed params 失败回复 `-32602`，连接可继续。`-32603` 只表示 router/bootstrap 内部异常；Agent 业务失败使用 Ora server error 与 closed `data.kind`。
- method registry 按方向封闭：Plugin→Host 只允许 `$/stream` Notification；Host→Plugin 只允许 `$/cancelRequest` 与 `$/exit` Notification。其他合法但未知 Notification 因 JSON-RPC 不允许回 Response，直接记录 metadata-only violation 并 fatal。
- envelope 不允许未定义的顶层字段；params/result/error.data 内部字段由对应 typed DTO 决定。`error.data` 若存在必须是 JSON object。

这是一份明确的 “JSON-RPC 2.0 over Ora Frame v1” profile：采用 JSON-RPC 的 Request/Response/error 语义，但不支持 batch；`$/...` 与 `-32800` 是 Ora 借用 LSP 拼写/取消语义的扩展，不表示 Ora 实现 LSP。

### 12.6 请求关联与取消

- v1 只有 Host outbound request router；Plugin 只响应 Host Request。收到 Plugin→Host Request 时 best-effort 回复 `-32601` 后终止 generation，因为它违反协商的 `pluginApi=1`，不能把未来方法当作当前空壳。
- id 最长 128 UTF-8 bytes；method 最长 256 bytes；每连接普通 pending 默认上限 128。
- private bootstrap 检查 inbound Host Request id：同一 in-flight id 重复是 protocol violation，不能覆盖已有 handler。Rust Host 不维护“Plugin inbound Request”表，只检查 Response 是否匹配自己的 pending。Host id generator 在同一 session/generation 内不得复用已完成 id。
- late/unknown response id 记录 metadata-only warn 后丢弃；不能完成新 generation 的请求。
- 取消通知的 wire method 固定写作 `$/cancelRequest`，params 为 `{ "id": "..." }`；v1 只允许原 Host requester 向 Plugin responder 发送。duplicate/unknown cancel 可由合法 race 产生，按 metadata-only warn+drop并限速，不作为 fatal。
- runtime actor 是 pending table、priority scheduler、caller completion 与 wire state 的唯一所有者，状态至少为 `Queued / WriteStarted { bytes_written, deferred_events } / Written / Cancelling`。writer 只是容量 1 的 I/O worker：actor 选择一个带 monotonic sequence/可选 `causal_after` 的完整 frame command并原子转为 `WriteStarted`；writer 逐次累计成功写入的 bytes，最终回 `FrameWritten` 或 `WriteFailed { bytes_written }`，actor 才转为 `Written`、`NotWritten`/`PossiblyWritten` 结果或 fatal drain。不存在 actor/writer 两份可分叉的 request state。
- cancel、deadline、correlated stream 与 terminal 都在进入 actor 时取得单调 `actor_sequence`。普通 invocation 另有 closed `termination_intent = ExplicitCancel | HostStop | Backpressure | HardDeadline`；最早 sequence 的有效 intent 唯一决定 fallback 原因，后到 intent 不能把它改写成另一种 caller error，但后到的 invocation deadline 仍必须执行下面的绝对 cleanup cap。普通 `Queued` invocation 的 cancel/deadline/HostStop 从 scheduler 删除并完成 `NotWritten`，不发送 cancel-before-request；已被 Host 接受的 `cancelConversation` 即使其 safety frame尚在 Queued，也不能撤销，必须走 §13.1 的 terminate+`CancellationUnconfirmed` 例外。`WriteStarted` 不能因普通 cancel中断 partial frame，也不能假设 writer ack 一定先于 peer 回包：相关事件按 sequence 放入该 request 的 `deferred_events`，等待 writer 结论。该队列同时受单 invocation event/byte cap；满时 reader data lane 停止交付而不 drop，writer ack 使用 §11.5 的独立 reserve，因此不会与回包形成死锁或被 data flood 饿死。重复同类 intent 合并；stream/terminal 与首次/后续 intent 仍全部保序。
- `WriteStarted + FrameWritten` 原子转为 `Written`；该 ack 只是确认 wire 状态的 causal gate，随后严格按原 `actor_sequence` 重放全部 deferred stream、terminal、cancel 与 deadline。这样 response-before-ack→deadline→ack 仍由 response 胜出，而 deadline-before-response→ack 会先冻结 timeout；任务调度顺序不能改变结果。`WriteFailed` 时 deferred inbound 不能覆盖本端写入分类：全部标为未采纳并随 session fatal 回收；只有 `bytes_written: 0` 可按 `NotWritten`，任意 `bytes_written > 0` 或底层不能证明为零都按 `PossiblyWritten`。PossiblyWritten 的 non-idempotent 请求按触发原因分别为 `UnknownOutcome { CancellationUnconfirmed | DeadlineExceeded | ConnectionLost }`；普通 idempotent 请求分别为 `Cancelled`、`RequestTimedOut` 或 `TransportFailed`，且同样不自动重放。已被 Host 接受的 `cancelConversation` 是 §13.1 明确定义的 safety-control 例外，不适用这个普通 idempotent 结论。
- writer 只发送一个同时携带 generation、command owner、`bytes_written`、`WriterFailureStage` 与 `IoFailure` 的最终 `WriteFailed` completion。actor 在**同一 turn**按固定顺序处理：先用 `bytes_written` 确定 owner 的 `NotWritten/PossiblyWritten`；若 generation 尚未有 primary fatal trigger，再把该事实设为 `DrainTrigger::WriterFailure`、关闭 admission，并为所有无自身 intent 的 owner/bystander 锁存 writer stage 映射的 cause；若 primary 已存在，则只写入有界 secondary diagnostic；最后根据最早 first-intent、safety 规则或 write-once fatal cause 完成 owner。三步之间不能处理新的 deadline/cancel。writer failure 为 primary 时，Request/transport-cancel/session-control 分别映射 `RequestWrite/TransportCancelWrite/SessionDrain`，不能等 EOF/direct-exit 后再重分类。writer failure 后到时不替换 `Draining.primary_trigger`，不改写 owner/bystander 早先的 fatal cause。已有 termination intent 的 Request owner 保留 first-intent outcome；accepted safety Request 保留 §13.1 safety outcome；session-control command 没有业务 owner。
- 本文的 `Written` 指完整 `5 + N` bytes 的 `write_all` 成功（实现可以随后调用 `flush`），只证明 bytes 已交给本端 pipe API，不表示 peer 已读取。writer ack 必须携带 generation 与 frame/request id。
- 每个 method 在 contract 中声明 `InvocationSemantics::Idempotent` 或 `NonIdempotent`；具体矩阵由 §13 冻结，Host 对两类都永不自动重放。
- invocation outcome deadline `D_inv = api_accept_monotonic + invocation_timeout`，覆盖 start single-flight、runtime admission、scheduler 排队、write 与 response 等待；使用可注入 monotonic clock。Running 前已超时的单个 waiter从队列移除且不发 Request，也不取消其他 waiter共享的 start。deadline event 使用不可借用 control slot；与 terminal 的同刻竞态只按 `actor_sequence` 裁决，已取得更早 sequence、仍在等待 `FrameWritten` replay 的 terminal 仍算 terminal-first。
- 对 §13.1 safety 以外的普通 invocation 使用双时钟。若 ExplicitCancel/HostStop/Backpressure intent 在 `D_inv` 前先发生，其 cleanup cap 精确为 `min(intent_accept + transport_cancel_total, D_inv)`；cancel frame 还必须在该 cap 内满足更短的 control-write deadline，terminal grace 也不得越过该 cap。到 cap 尚未确认就立即产生 `StopEscalation` 并 terminate Job，后到 hard-deadline event只执行回收，不能把 fallback 原因改成 `DeadlineExceeded`，也绝不从 `D_inv` 重新授予 grace。若这条 cap 在 `WriteStarted` 期间到达且不存在更早 sequence 的 deferred terminal，允许为终止损坏 session而中断 writer/关闭 Job；不能为等待 writer ack 越过 cap。
- **普通 invocation 的 cancellation first**：ExplicitCancel 来自 caller，HostStop 来自 disable/uninstall/shutdown，但两者使用同一 outcome 矩阵。`NotWritten` 立即完成 `Cancelled`；`Written` 发送 cancel。cleanup cap 前 success/业务 error 表示 cancel 输掉竞态并由 terminal 胜出，`-32800` 完成为 `Cancelled`。cap 前没有 terminal则终止 Job：idempotent 完成 `Cancelled`，non-idempotent 完成 `UnknownOutcome { cause: CancellationUnconfirmed }`。后到 backpressure/deadline不改写此结果规则。`cancelConversation` safety action 一旦接受，caller cancel 只 detach waiter；HostStop 继续等待原 safety future，stop deadline 到达则按 §13.1 terminate，不进入本分支。
- **hard deadline first**：仅当更早 sequence 没有 terminal、ExplicitCancel、HostStop 或 Backpressure intent时，`D_inv` 才成为 `HardDeadline` intent并立即冻结 API outcome。普通 `NotWritten` invocation 为 `RequestTimedOut`；普通 Written idempotent 为 `RequestTimedOut`；Written non-idempotent 为 `UnknownOutcome { cause: DeadlineExceeded }`。对普通 Written 请求，冻结 caller 后按 Request→cancel causal order发送 `$/cancelRequest`；这是独立的、有限 post-outcome cleanup deadline，允许 Job 短暂晚于 `D_inv` 存活但绝不延长或改变 invocation outcome。`WriteStarted` 保存该 intent，`FrameWritten` 后在剩余 cleanup budget 内发送 cancel；zero/partial/unknown write failure 按 §12.6 写入矩阵收敛，cleanup 到期仍未 settle则产生 `StopEscalation` 并 terminate Job。late terminal 只回收 tombstone/审计。已接受的 `cancelConversation` deadline 不发送会撤销 safety handler 的 `$/cancelRequest`，而是立即 terminate Job并按 §13.1 完成 `CancellationUnconfirmed`。

`FrameWritten` 后，已有 termination intent 的普通 invocation 使用下列唯一结果矩阵；此时 connection/session fatal 等价于“cap 前无 terminal”并先完成 Job drain。无 termination intent 的 fatal 使用后面的独立矩阵；`cancelConversation` 只使用 §13.1 的 safety 矩阵：

| 最早有效 settlement/intent | cap 前 terminal | cap 前无 terminal |
|---|---|---|
| terminal | 原 success / business error | 不适用，不发送 cancel |
| `ExplicitCancel` / `HostStop` | success/error 表示 cancel 输掉；`-32800` → `Cancelled` | terminate；idempotent → `Cancelled`，non-idempotent → `UnknownOutcome(CancellationUnconfirmed)` |
| `Backpressure` | 任意合法 terminal → `BackpressureExceeded`，已交付事件保持 incomplete | terminate；idempotent → `BackpressureExceeded`，non-idempotent → `UnknownOutcome(CancellationUnconfirmed)` |
| `HardDeadline` | caller 已冻结；terminal 只回收 tombstone | cleanup deadline 后 terminate；caller 仍为 idempotent `RequestTimedOut` / non-idempotent `UnknownOutcome(DeadlineExceeded)` |

若没有任何 termination intent，writer/connection/process/StopEscalation fatal 使用独立 `FatalSettlementCause`，不能假装存在 cancel cap。unexpected direct-exit 已按 §11.5 立即请求 terminate Job，但仍保留 pipe handles读取该 generation 到 boundary EOF；因此 EOF 前收到并通过 correlation/DTO 校验的 Written terminal仍正常胜出。boundary EOF/reader fatal 后不再接受 frame。`WriteStarted` 先等待 writer 的 `FrameWritten/WriteFailed`；drain deadline 内仍无法证明零 bytes 时按 `PossiblyWritten`，deferred inbound 在本节 WriteFailed 路径仍不采纳。剩余 pending 的唯一矩阵为：

| fatal cause | `NotWritten`（含 write-failed zero） | idempotent `Written/PossiblyWritten` | non-idempotent `Written/PossiblyWritten` |
|---|---|---|---|
| `ConnectionLost { stage }` | `TransportFailed(stage)` | `TransportFailed(stage)` | `UnknownOutcome(ConnectionLost)` |
| `ProcessExited { exit_code? }` | `PluginExited(exit_code)` | `PluginExited(exit_code)` | `UnknownOutcome(ProcessExited)` |

generation 级 protocol diagnostic 仍可精确为 `ProtocolViolation`；上表只定义该 generation 中尚未完成 invocation 的稳定 caller boundary。EOF-first 与 direct-exit-first 的 cause 由 §11.6 首个 fatal trigger锁存；已有 termination intent 时继续使用上一张表的 first-intent fallback，而不是改成 `ConnectionLost/ProcessExited`。

- caller completion 与 wire tombstone 分开；terminal Response、generation exit 或 grace timeout 才回收已写 entry。receiver 收到 cancel 后仍必须产生 terminal Response；Host 永不自动重试。
- cancel/grace 导致整 Job 回收时，其他 pending 若已有自身 termination intent则用 first-intent 矩阵；否则作为 bystander 使用 fatal-settlement 矩阵。不能统一报 crash，任何 Written non-idempotent 无 terminal 都必须是带精确 cause 的 `UnknownOutcome`。
- `agent.cancelConversation` 是停止一个既存 active turn 的业务级 safety action；`$/cancelRequest` 只取消单个 RPC id，二者不能互换。一旦 safety action 被 Host 接受，HTTP caller 断开只 detach waiter，不能再用 transport cancel 撤销“取消动作”；并发重复 business cancel join 同一 stop future。

### 12.7 流与背压

`$/stream` notification：

```json
{
  "jsonrpc": "2.0",
  "method": "$/stream",
  "params": {
    "id": "h:1",
    "seq": 1,
    "value": {"kind":"textDelta","text":"..."}
  }
}
```

规则：

- v1 仅允许 Plugin→Host，为 Host 发起且 Agent contract 标记为 streaming 的 Request 发送；非 streaming method 发 stream 是 fatal。
- `value` 必须是该 method 在 `ora-plugin-protocol` 中生成的 stream-event 判别联合，不允许无约束 `unknown`/`Value`。
- 同一 active request 的 `seq` 从 1 开始严格递增；gap 或 duplicate 是 fatal protocol violation，终止连接。Cancelling tombstone 上的 late stream 丢弃并计数；terminal Response 之后的 stream 是 fatal。其他 unknown id 按 late-response 策略 warn+drop，超过滑动窗口阈值则终止连接。
- private bootstrap 对每个 streaming invocation 只串行调用一次 AsyncGenerator `next()`；event 通过有界 enqueue ack 后才取下一项。terminal writer command 携带 `causal_after_seq`，唯一 writer 必须确认该 request 的全部 `seq <= causal_after_seq` frame 已 `write_all` 后才写 terminal Response。control-lane 优先级不能破坏这一 per-request causal barrier。
- reader→actor 对单 generation 保持 FIFO，使 stream event 必定先于 pipe 中随后到达的 terminal Response。
- stream consumer channel 有界；满时停止向 caller 交付并把 partial events 标记 incomplete，并在尚无更早 termination intent 时登记 `Backpressure` intent、发送 transport cancel；不能静默 drop，也不能无限阻塞整个 reader。它不是尚未 settled 的 Written 调用的立即终态：在 `min(backpressure_accept + transport_cancel_total, D_inv)` 前收到任意合法 terminal，当前 invocation 完成确定的 `BackpressureExceeded`；到 cap 仍无 terminal则 terminate Job，当前 idempotent invocation 完成 `BackpressureExceeded`，non-idempotent 完成 `UnknownOutcome { cause: CancellationUnconfirmed }`。后到 hard deadline只执行这个 cap而不改写原因；其他 pending 仍按 §12.6 各自矩阵收敛。
- stream 永远不能替代 terminal Response；terminal error 前已交付的 partial events 标记 incomplete，不能冒充成功结果。
- 除全局 8 MiB frame cap 外，Agent method registry 进一步冻结：单 stream event payload 最大 256 KiB、单 terminal Agent success/error 最大 1 MiB、分页 result 受条目数与该 byte cap 双重限制。writer queue 默认最多 256 frames / 16 MiB；其中至少 32 frames / 2 MiB 是 control 总预算，并进一步划分为不可被 ordinary terminal 借用的 lifecycle/transport-cancel reserve、不可被 ordinary handler 借用的 business-safety reserve，以及 ordinary terminal reserve。各非借用子预算必须始终容纳其最大合法 frame与 active safety 上限；配置校验不满足时 readiness fail closed。
- 每 invocation 的未消费 event/bytes、每 plugin 的 inbound/outbound queue/pending/active-turn bytes，以及所有 active plugin 的 Host global bytes 都分别有 `PluginLimits` 上限。全局预算耗尽时拒绝新 ordinary invocation，不能通过启动更多插件线性耗尽内存；safety cancel 与 stop 仍使用预留预算。
- terminal Response、`-32010`、transport/business cancel、lifecycle 或 exit 无法在各自 control deadline 内进入保留 lane/完成写入时，不得 drop、降级到 ordinary queue 或仅向 caller 返回 Busy；这是 runtime-fatal，Host 立即关闭 admission并按 Written/UnknownOutcome 矩阵回收整个 Job。
- enqueue 与 write 都有 deadline。
- 不使用“session 终身累计字节上限”；长期 Agent 会自然超过。限制对象是单帧、同时排队 bytes、单 request stream buffer、pending 数和滑动窗口速率。

### 12.8 握手

wire version 与 Ora app version、manifest `engines.ora`、`engines.pluginApi` 分离。wire 由同一 Ora build 的 Rust Host/private bootstrap/runtime receipt 锁定，不由插件作者协商。

```text
Host spawn private bootstrap（尚未 import 插件 entry）
  -> Host Request $/initialize
  <- Bootstrap Response（必须是首个 child frame）
  -> Host Request $/activate
     bootstrap dynamic import + structural validation
     await plugin activate(context)
     install and verify provider dispatch table
  <- Bootstrap Response
  -> Host recheck generation + enablement epoch + registry revision
  -> RuntimeState::Running（原子开放 admission）
```

`$/initialize` params 精确为：

```json
{
  "wireVersion": 1,
  "hostVersion": "0.1.0",
  "runtimeVersion": "0.1.0",
  "sessionId": "...",
  "plugin": {
    "id": "ora.claude-code",
    "version": "0.1.0",
    "kind": "agent",
    "pluginApi": 1,
    "contentOwner": "sha256-..."
  },
  "paths": {
    "extensionPath": "D:\\\\...\\\\plugins\\\\ora.claude-code",
    "entryPath": "D:\\\\...\\\\plugins\\\\ora.claude-code\\\\dist\\\\index.js",
    "storagePath": "D:\\\\...\\\\plugin-data\\\\ora.claude-code\\\\sha256-..."
  },
  "declaredAgents": [
    {"id":"claude-code","contractVersion":1}
  ],
  "limits": {
    "maxFrameBytes": 8388608,
    "maxPendingRequests": 128,
    "maxAgentEventBytes": 262144,
    "maxAgentResultBytes": 1048576,
    "maxAgentPromptBytes": 1048576,
    "maxActiveTurns": 64,
    "maxPageItems": 100
  }
}
```

initialize result 精确为：

```json
{
  "wireVersion": 1,
  "runtimeVersion": "0.1.0",
  "sessionId": "...",
  "plugin": {
    "id": "ora.claude-code",
    "version": "0.1.0"
  }
}
```

这一步只确认 private bootstrap/runtime/session，绝不声称插件 provider 已加载。identity echo 是资产错配交叉检查，不是发布者认证。`extensionPath` 是受管代码根，`entryPath` 是 Host 从本次 Installed validation proof 计算的受管绝对 entry，`storagePath` 是当前 content owner 的专属可变目录。`entryPath` 只供 private bootstrap 在 activate 时 import，不进入作者 `ExtensionContext`；bootstrap 还要复核它位于 `extensionPath` 下。三者都不是客户端或插件 response 可重新选择的字段；不下发 `ORA_DATA_DIR`、credential value、`globalState` 或唯一 workspace。

`sessionId` 每 generation 由 CSPRNG 生成且永不复用，initialize result 必须精确回显；它绑定 pipe/generation，不是发布者认证。`limits.maxFrameBytes` 与 wire v1 常量必须精确相等，只用于发现错误资产，不是协商值；`maxPendingRequests` 只计 ordinary handlers，business-safety slots 另按 `maxActiveTurns` 非借用保留。其余 limits 来自 Host `PluginLimits` 且不得超过 v1 hard cap。Plugin 没有 limits response，也不能提高任何 Host 上限。

`$/activate` params 精确为 `{ "reason": "lazyInvocation" | "manualStart" }`。bootstrap 此时才 import entry、验证 §13.2 default export、await activate 并安装 handler。result 精确为 `{ "providers": [{ "id": "...", "contractVersion": 1 }] }`；providers 必须与 manifest `contributes.agents` 按 canonical id 排序后深度相等，额外、缺失、重复或版本不同均为 `ActivationFailed`。只有 Host 收到完整 success Response、再次通过 generation/epoch/revision gate 后才标记 Running。

initialize/activate 各有独立 deadline。initialize 前或 activate 完成前的 Agent业务 Response/stream、任意 Plugin Request、重复 lifecycle method均 fatal。不存在 `plugin.ready` 或 `$/ready`；activate success Response 已是双向、可确认的 admission barrier。

停止协议精确为：`$/deactivate` Request，params 为 `{ "reason": "manualStop" | "disable" | "uninstall" | "shutdown" | "grantChanged" }`、result 为 null；随后发送无 params 的 `$/exit` Notification。仅协议健康且 activate 成功的 generation 调用 deactivate；异常连接或 active request 无法在 grace 内 drain 时直接 terminate/drain，不执行插件清理代码。所有 lifecycle DTO、Agent DTO 与 Frame fixture 由 Rust 真相源生成 TypeScript，SDK 团队不得另写相似 interface。

未来 plugin module 能力通过新的 exact `pluginApi` 版本加入。wire v1 不在连接内协商：runtime asset 不匹配直接 readiness/start 失败；不兼容 framing 需要新的 runtime 启动约定，不能靠已经无法解析的 JSON handshake 修复。

### 12.9 Golden vectors

规范仓库只维护一份机器可读 fixture，每项固定为 `{ type, payload_utf8, payload_len, header_hex, frame_hex }`；`frame_hex = header_hex || UTF-8(payload_utf8)`。下表是该 fixture 的人类可读投影，length 不含 5-byte header：

| payload | bytes | header hex |
|---|---:|---|
| `{"jsonrpc":"2.0","id":"h:1","method":"ping","params":{}}` | 56 | `00 00 00 38 01` |
| `{"jsonrpc":"2.0","id":"h:1","result":"ok"}` | 42 | `00 00 00 2a 02` |
| `{"jsonrpc":"2.0","method":"$/exit"}` | 35 | `00 00 00 23 03` |
| `{"jsonrpc":"2.0","method":"$/stream","params":{"id":"h:1","seq":1,"value":{"kind":"textDelta","text":"你好"}}}` | 112 | `00 00 00 70 03` |

非法向量：

| hex | 原因 |
|---|---|
| `00 00 00 00 01` | zero length |
| `ff ff ff ff 01` | negative length |
| `00 80 00 01 01` | 8 MiB + 1，超限 |
| `00 00 00 02 7f 7b 7d` | unknown type 127 |

这些向量必须由一个生成器校验 payload byte length/header/full frame，并同时供 Rust encode→TS decode 和 TS encode→Rust decode 使用，不能在两端各写一份可能同时漂移的 fixture。测试还必须覆盖最大合法帧、最大值 + 1、`i32::MIN`、负 type、JSON 内部空白/换行、payload 后无 LF，以及 encoder 对零长/超限/未知 type 的拒绝。

### 12.10 stdout/stderr 与 Windows

- stdout 是唯一协议通道；stderr 永远不进入 FrameDecoder。
- Rust 与 Bun 都使用 byte API，不经过文本行转换。
- 直接 Bun child pipe 不依赖 CRT text mode；如果未来接入使用 MSVCRT stdio 的 native helper，该 helper 必须自行把 stdin/stdout 设为 binary mode。
- 必须有 Windows 真实 Bun 子进程 E2E，验证 5-byte header、中文 UTF-8、每字节分片、多个帧合并和 stderr flood。

---

## 13. Agent Contract v1 与作者 SDK ABI

插件管理器不实现具体 Agent，但需要冻结最小管理契约，使 Claude/Codex/OpenCode 插件能被一致选择和调用。

### 13.1 Agent 方法契约

Agent Contract v1 固定由 AgentContribution 声明 provider identity，业务方法由 Host registry 固定，不由 manifest 任意声明：

```text
agent.discoverInstallations
agent.getConfigurationSummary
agent.listSkills
agent.listMcpServers
agent.listConversations
agent.startConversation    // 支持 $/stream，创建会话并发送首条 prompt
agent.sendMessage          // 支持 $/stream
agent.cancelConversation   // safety-control 业务方法
```

原则：

- application route 始终携带全局 provider key；进入 wire 后使用其 agent local id。除 discovery 外携带该 provider 返回的 installation id，涉及会话时再携带 conversation id；这些 id 不能跨 provider 猜测或复用。
- installation/config/skill/MCP/conversation 的纯数据 DTO、closed enum、method 常量与 error data 由 `ora-plugin-protocol` 定义并经 ts-rs/xtask 生成。
- prompt、token、配置正文和凭据不进入默认 tracing。
- v1 invocation semantics 冻结为：`discoverInstallations/getConfigurationSummary/listSkills/listMcpServers/listConversations/cancelConversation` 是 idempotent；`startConversation/sendMessage` 是 non-idempotent。Host 两类都不自动重试；non-idempotent Written 请求在 timeout/crash 且无 terminal response 时返回 `UnknownOutcome`。`cancelConversation` 的“可重复调用”只说明 provider 应安全处理重复停止，不代表 Host 能在丢失 safety terminal 时推断结果；Host application actor 接受该 safety action 后，§13.1 的 `CancellationUnconfirmed` 规则覆盖 §12.6 的普通 idempotent transport 结果。
- Plugin→Host 业务 Request 在 v1 中不存在。日志进入 stderr；路径与环境由 initialize/launch grant 下发。插件不能发送 raw method string 动态扩权。
- activate result 的 provider descriptor 只能与 manifest 声明集合精确相等，不能动态注册新 Agent 或权限。
- 需要项目/工作树上下文的方法在每次 typed Agent request 中携带 Host-issued closed `AgentScope`；不能把应用当前窗口或单一 workspace 隐式变成进程全局状态。

`AgentScope` 是使用 `type` discriminant 的判别联合：`global`、`project { projectHandle, workingDirectory }` 或 `worktree { projectHandle, worktreeHandle, workingDirectory }`。handle 是 session-bound opaque newtype；`workingDirectory` 是 Host 从当前项目模型解析、canonicalize 后随本次请求下发的绝对路径，使插件无需 Plugin→Host API 也能以正确 cwd 启动 Agent CLI。WebView 不能提交或覆盖该路径；application/runtime 在真正 enqueue/write 前再次验证 handle 和 cwd 仍属当前 session。Request 达到 Written 后项目关闭/路径变化不能静默替换 wire 上的 scope，产品只能显式 cancel；下一请求重新签发。该 descriptor 只表达调用上下文和审计事实，不是 filesystem sandbox，插件不得缓存为未来请求的权威状态。

各 Request 的 v1 JSON 字段精确冻结如下；字段名使用 lowerCamelCase，所有对象递归拒绝未知字段，`?` 只表示该字段可省略而不表示接受 `null`：

```text
DiscoverInstallationsRequest  { providerId, scope }
GetConfigurationSummaryRequest { providerId, installationId, scope }
ListSkillsRequest              { providerId, installationId, scope, cursor?, limit }
ListMcpServersRequest          { providerId, installationId, scope, cursor?, limit }
ListConversationsRequest       { providerId, installationId, scope, cursor?, limit }
StartConversationRequest       { providerId, installationId, scope, clientRequestId, prompt }
SendMessageRequest             { providerId, installationId, conversationId, scope, clientRequestId, prompt }
CancelConversationRequest      { providerId, installationId, conversationId, scope }
```

下列 leaf-newtype 表、Rust data model 与其序列化规则共同构成全部 Agent v1 wire DTO 的规范定义；实现时放入 `ora-plugin-protocol::agent`，由它生成 TypeScript、JSON Schema 与 golden fixture，不允许 SDK 再手写同名 data interface。表中 `string`/`number` newtype 都以 JSON primitive **透明**编码，绝不编码成 `{ "value": ... }` object：

| leaf type | JSON primitive | v1 validation |
|---|---|---|
| `AgentProviderId` | string | 精确使用 §5.2 local provider-id grammar，且等于 manifest contribution id |
| `AgentInstallationId`、`AgentConversationId`、`AgentTurnId`、`AgentCursor`、`AgentResourceId`、`AgentToolCallId` | string | 1..=256 UTF-8 bytes；首尾 trim 后不变；拒绝 NUL、C0/C1 control、`/`、`\\`、`:` |
| `AgentConfigurationKey` | string | 1..=512 ASCII bytes；`^[A-Za-z0-9][A-Za-z0-9._-]{0,511}$`；它不适用上一行的 256-byte cap |
| `ProjectHandle`、`WorktreeHandle` | string | Host-issued/session-bound；使用 `AgentInstallationId` 等 opaque identity 的 1..=256-byte/control/path-separator 规则 |
| `ClientRequestId` | string | Host-issued canonical lower-case UUID text，精确为 `8-4-4-4-12` hex 形式 |
| `HostResolvedAbsolutePath` | string | 1..=32 KiB UTF-8 bytes、无 NUL；必须是 Host canonicalize 后签发的绝对 Windows path |
| `AgentPrompt` | string | 1..=1 MiB UTF-8 bytes、无 NUL；保留空白与换行，不做 trim/Unicode normalization |
| `Rfc3339Timestamp` | string | 最长 64 ASCII bytes；严格 RFC 3339，必须带 `Z` 或 numeric offset |
| `AgentPageLimit` | number | JSON integer，`1..=100`，不接受 quoted number |
| `JsonSafeU64` | number | JSON integer，`0..=2^53-1`，不接受 quoted number、fraction 或 `2^53` |
| `FiniteJsonNumber` | number | finite JSON number；拒绝 NaN、±Infinity 与溢出 |

普通 `String` 字段仍是 JSON string，并受下文逐字段/类别 hard cap 约束；“透明 primitive”不放宽 object 的 unknown-field/null 规则。

```rust
pub enum AgentScope {
    Global,
    Project {
        project_handle: ProjectHandle,
        working_directory: HostResolvedAbsolutePath,
    },
    Worktree {
        project_handle: ProjectHandle,
        worktree_handle: WorktreeHandle,
        working_directory: HostResolvedAbsolutePath,
    },
}

pub struct DiscoverInstallationsRequest {
    pub provider_id: AgentProviderId,
    pub scope: AgentScope,
}

pub struct GetConfigurationSummaryRequest {
    pub provider_id: AgentProviderId,
    pub installation_id: AgentInstallationId,
    pub scope: AgentScope,
}

pub struct ListSkillsRequest {
    pub provider_id: AgentProviderId,
    pub installation_id: AgentInstallationId,
    pub scope: AgentScope,
    pub cursor: Option<AgentCursor>,
    pub limit: AgentPageLimit,
}

pub struct ListMcpServersRequest {
    pub provider_id: AgentProviderId,
    pub installation_id: AgentInstallationId,
    pub scope: AgentScope,
    pub cursor: Option<AgentCursor>,
    pub limit: AgentPageLimit,
}

pub struct ListConversationsRequest {
    pub provider_id: AgentProviderId,
    pub installation_id: AgentInstallationId,
    pub scope: AgentScope,
    pub cursor: Option<AgentCursor>,
    pub limit: AgentPageLimit,
}

pub struct StartConversationRequest {
    pub provider_id: AgentProviderId,
    pub installation_id: AgentInstallationId,
    pub scope: AgentScope,
    pub client_request_id: ClientRequestId,
    pub prompt: AgentPrompt,
}

pub struct SendMessageRequest {
    pub provider_id: AgentProviderId,
    pub installation_id: AgentInstallationId,
    pub conversation_id: AgentConversationId,
    pub scope: AgentScope,
    pub client_request_id: ClientRequestId,
    pub prompt: AgentPrompt,
}

pub struct CancelConversationRequest {
    pub provider_id: AgentProviderId,
    pub installation_id: AgentInstallationId,
    pub conversation_id: AgentConversationId,
    pub scope: AgentScope,
}

pub struct DiscoverInstallationsResponse {
    pub installations: Vec<AgentInstallation>,
    pub diagnostics: Vec<AgentDiscoveryDiagnostic>,
}

pub struct AgentInstallation {
    pub installation_id: AgentInstallationId,
    pub display_name: String,
    pub version: Option<String>,
    pub location_display: Option<String>,
    pub availability: AgentAvailability,
}

pub enum AgentAvailability {
    Available,
    Unavailable { reason: String },
}

pub struct AgentDiscoveryDiagnostic {
    pub kind: AgentDiscoveryDiagnosticKind,
    pub message: String,
}

pub enum AgentDiscoveryDiagnosticKind {
    NotFound,
    PermissionDenied,
    ProbeFailed,
}

pub struct GetConfigurationSummaryResponse {
    pub items: Vec<AgentConfigurationItem>,
}

pub struct AgentConfigurationItem {
    pub key: AgentConfigurationKey,
    pub display_name: String,
    pub source: AgentResourceSource,
    pub value: AgentConfigurationValue,
}

pub enum AgentConfigurationValue {
    Unset,
    Redacted,
    Boolean { value: bool },
    Number { value: FiniteJsonNumber },
    String { value: String },
    StringList { value: Vec<String> },
}

pub struct ListSkillsResponse {
    pub items: Vec<AgentSkillSummary>,
    pub next_cursor: Option<AgentCursor>,
}

pub struct AgentSkillSummary {
    pub id: AgentResourceId,
    pub display_name: String,
    pub description: Option<String>,
    pub source: AgentResourceSource,
}

pub struct ListMcpServersResponse {
    pub items: Vec<AgentMcpServerSummary>,
    pub next_cursor: Option<AgentCursor>,
}

pub struct AgentMcpServerSummary {
    pub id: AgentResourceId,
    pub display_name: String,
    pub transport: AgentMcpTransport,
    pub enabled: bool,
    pub source: AgentResourceSource,
}

pub enum AgentResourceSource {
    User,
    Project,
    Worktree,
    BuiltIn,
    Unknown { display: Option<String> },
}

pub enum AgentMcpTransport {
    Stdio,
    Http,
    Sse,
    Unknown,
}

pub struct ListConversationsResponse {
    pub items: Vec<AgentConversationSummary>,
    pub next_cursor: Option<AgentCursor>,
}

pub struct AgentConversationSummary {
    pub conversation_id: AgentConversationId,
    pub title: Option<String>,
    pub updated_at: Option<Rfc3339Timestamp>,
}

pub enum AgentEvent {
    ConversationStarted { conversation_id: AgentConversationId },
    TextDelta { channel: AgentOutputChannel, text: String },
    Status { phase: String, message: Option<String> },
    ToolCall { call_id: AgentToolCallId, name: String, summary: Option<String> },
    ToolResult { call_id: AgentToolCallId, is_error: bool, summary: Option<String> },
    Usage { usage: AgentUsage },
}

pub enum AgentOutputChannel {
    Assistant,
    Reasoning,
    Tool,
}

pub struct AgentUsage {
    pub input_tokens: Option<JsonSafeU64>,
    pub output_tokens: Option<JsonSafeU64>,
    pub cost_micros: Option<JsonSafeU64>,
}

pub struct AgentTurnResult {
    pub conversation_id: AgentConversationId,
    pub turn_id: Option<AgentTurnId>,
    pub finish_reason: AgentFinishReason,
    pub usage: Option<AgentUsage>,
}

pub enum AgentFinishReason {
    Completed,
    Cancelled,
    Limit,
}

pub struct CancelConversationResponse {
    pub disposition: CancelDisposition,
}

pub enum CancelDisposition {
    Accepted,
    AlreadyStopped,
}
```

JSON projection 精确遵循：struct field 为 lowerCamelCase；Agent DTO 中每个 object 都递归拒绝未知字段，只有另行定义的 `AgentBusinessError.details` 是受限 extension bag；`Option<T>` 只可省略，显式 `null` 拒绝。`AgentScope`、`AgentAvailability`、`AgentConfigurationValue` 与 `AgentResourceSource` 使用 `type` tag；例如 `{ "type":"project", "projectHandle":"...", "workingDirectory":"..." }`、`{ "type":"unavailable", "reason":"..." }`、`{ "type":"redacted" }`、`{ "type":"number", "value":1.5 }`。`AgentEvent` 使用 `kind` tag（`conversationStarted/textDelta/status/toolCall/toolResult/usage`）。其余 enum 均为 lowerCamelCase JSON string；因此 cancel result 精确为 `{ "disposition":"accepted" }` 或 `{ "disposition":"alreadyStopped" }`，不存在 boolean/空 object 的替代编码。

容器约束同样属于 contract：分页 `items.len <= request.limit`；同一 response 内 installation/resource/conversation id 必须唯一；`AgentUsage` 出现时至少一个字段存在；`FiniteJsonNumber` 拒绝 NaN/Infinity。`NotFound` discovery 的 canonical 结果是空 `installations` 加至少一个 `notFound` diagnostic。configuration secret 只能编码为 `{ "type":"redacted" }`，protocol 没有 secret 原文 variant。任何 shape、tag、null、duplicate id、关联或 cap 违约都是 `AgentContractViolation`，不是可忽略的额外字段。

Agent v1 hard cap 冻结如下；计数均同时受 §12.7 aggregate/frame cap，所有 bytes 都指 UTF-8 bytes。exact initialize `limits` 只有 7 项：`maxFrameBytes` 必须精确等于 8 MiB wire 常量；其余 ordinary pending、Agent event、Agent terminal result/error、prompt、active turn 与 page items 六项可按 generation 收紧。Host 对这六项下发正整数且不得高于下表/§12 hard cap；bootstrap 在 initialize success 前验证并安装同一组实际值，不另行回显。其余 opaque id、configuration key、普通 string/list、scope path、discovery/config collection 等 cap 都是 `wireVersion=1` 的固定常量，不在 `PluginLimits` 中动态覆盖，也不需要在 initialize 重复传输。若未来要配置它们，必须先扩展 exact initialize DTO、升级相应 wire contract并增加生成 drift/golden，而不能只改一端常量。

| 项目 | v1 hard cap |
|---|---:|
| opaque id / cursor / call id / turn id | 256 bytes |
| display name / tool name / status phase / version | 512 bytes |
| configuration key | 512 ASCII bytes；使用 leaf table 的专用 grammar |
| diagnostic、description、summary、title、config string | 4 KiB |
| config string-list | 128 items，每项 4 KiB |
| prompt | 1 MiB |
| scope `workingDirectory` | 32 KiB |
| discovery installations / diagnostics | 128 / 64 |
| configuration items | 256 |
| paginated items | 100 |
| active provisional+bound turns / plugin | 64 |
| stream event / terminal result-or-error | 256 KiB / 1 MiB |

其中 `providerId` 必须等于 manifest 中本插件的 local Agent contribution id；application 使用的全局 provider key 不进入插件 wire。`installationId`、`conversationId` 与 cursor 都只能在产生它们的 `(plugin content owner, providerId, generation)` 范围内使用，Host 在 enqueue 前验证路由归属，bootstrap/provider 仍需验证业务存在性。分页 `limit` 是整数 `1..=100`；省略 cursor 表示第一页，返回结果统一使用 `{ items, nextCursor? }`，最后一页省略 `nextCursor`。`clientRequestId` 是 Host 生成并校验的 UUID correlation id，只用于端到端日志/结果关联，不授予自动重试或幂等语义。

| method | 关键 request 字段 | result / stream | 约束 |
|---|---|---|---|
| `discoverInstallations` | provider id、`AgentScope` | installations + diagnostics | 未找到返回空集合与 `NotFound` diagnostic，不是 transport error |
| `getConfigurationSummary` | provider/installation/scope | configuration items | 值只能是 unset/redacted/bool/finite number/string/string-list；协议没有 secret 原文 variant |
| `listSkills` | provider/installation/scope/cursor/limit | page of safe skill summaries | cursor 绑定 provider+generation，limit 1..=100 |
| `listMcpServers` | 同上 | page of safe MCP summaries | command/env/token 等敏感配置不回传，只返回 transport/enabled/source 等展示字段 |
| `listConversations` | 同上 | conversation summaries page | timestamp 若存在为 RFC3339；cursor 不跨 generation |
| `startConversation` | target/scope/clientRequestId/prompt | `AgentEvent*` + terminal `AgentTurnResult` | 首个业务 event 恰为 `conversationStarted`，id 与 terminal 相同 |
| `sendMessage` | target/scope/conversationId/clientRequestId/prompt | `AgentEvent*` + terminal `AgentTurnResult` | 不发送 conversationStarted；terminal conversationId 必须等于 request |
| `cancelConversation` | target/conversationId | `Accepted` / `AlreadyStopped` | `Accepted` 表示 active turn 已终止，不是异步承诺 |

Agent provider/installation/conversation/turn/cursor/resource/config-key/tool-call/client-request identity 都是 leaf table 定义的 validated newtype，不是裸 `String` alias。`AgentProviderId` 精确使用 §5.2 的 local grammar；插件产生的 installation/conversation/turn/cursor/resource/tool-call opaque identity 使用 256-byte 通用规则；`AgentConfigurationKey` 使用独立 512-byte ASCII grammar，明确不属于该通用组。外部 Agent 的不合规原生 id/key 由 provider 映射而不是原样暴露。插件返回的 path/location/source 只允许作为已脱敏 display data，绝不能反向成为 Host 的文件选择、cwd、install 或 grant 授权。`clientRequestId`、`updatedAt`、`AgentPrompt`、number newtype 的 primitive projection 与边界均以 leaf table 为准；所有 count/token/cost 使用 `JsonSafeU64`。

`AgentEvent` 是 closed union：`conversationStarted { conversationId }`、`textDelta { channel, text }`、`status { phase, message? }`、`toolCall`、`toolResult`、`usage`；`AgentTurnResult` 固定含 conversation id、可选 turn id、`completed|cancelled|limit` finish reason 与可选 usage。跨 JSON 的 token/cost 计数使用 `JsonSafeU64`。stream 仍由 §12.7 的 seq 和 terminal Response 裁决，AsyncGenerator 的 `return` 值不能丢失。

bootstrap 在编码前严格验证每个 provider event/result/business-error details；必须手工驱动 AsyncGenerator `.next()` 以取得最终 return value，不能用会丢弃 return 的简单 `for await`。非法 DTO、conversation correlation、额外 stream 或 generator protocol 是 `AgentContractViolation` 并终止 generation，不得伪装成可继续的普通业务失败。

`sendMessage` 对同一 `(provider, installation, conversation)` 最多一个 active turn。`startConversation` 在首个 `conversationStarted` 之前先以 `(provider, installation, requestId)` 注册 provisional active turn；收到合法首事件后原子绑定到 conversation key，若该 key 已有 active turn则为 `AgentContractViolation` 并终止 generation。绑定前只能按 Host request id 做 transport cancel，不能猜测 conversation id。每插件 provisional+bound active turns 的总数受 `PluginLimits` 上限。

`cancelConversation` 在 Host outbound 与 bootstrap inbound 两侧都使用独立 safety pending/handler slot、parsed-byte budget 与 control byte reserve，容量至少覆盖 active-turn 上限，并且只能在目标 turn Request 已达到 Written 后上 wire。若 application actor 查到没有 active/provisional turn，可在本地返回 `AlreadyStopped`；若存在并创建或加入 stop future，则 safety action 自该线性化点起已被 Host 接受。并发重复 cancel join 同一 stop future；caller 断开只 detach waiter。bootstrap 以 safety request admission 与 active-turn terminal 的本地 actor sequence 线性化竞态：目标先终止，或在取消实际生效前以 `completed/limit` 终止时，先等待该 terminal `FrameWritten` 再返回 `AlreadyStopped`；取消生效时，只有 `finishReason=cancelled` terminal 已 `FrameWritten` 后才能返回 `Accepted`。两种 result 都证明此刻不存在 active turn，区别只是停止原因。

一旦 safety action 被 Host 接受，在取得合法 `Accepted/AlreadyStopped` terminal 前，Host 不得因 `cancelConversation` 被声明为 idempotent 而返回普通 `TransportFailed/Cancelled/RequestTimedOut`。safety invocation deadline 与 terminal 同样按 actor sequence 裁决：已取得更早 sequence、仍等待 writer-ack replay 的合法 terminal 胜出；否则 deadline 到达时不发送会撤销 safety handler 的 transport cancel，而是立即终止并等待 Job tree 收敛。Host/bootstrap safety admission、zero/partial/unknown write、`-32010`、其他 result/error、连接丢失、reserve/dispatch/write failure 或 business-cancel grace 到期中的任一路径，也都先完成同一 tree drain，再把 active non-idempotent turn与本次 safety invocation分别完成为 `UnknownOutcome { cause: CancellationUnconfirmed }`。这一定义是 §12.6 普通 idempotent矩阵的方法级例外；即使 tree kill 已停止计费，也不能伪造 provider 曾返回 `Accepted`。不能仅返回 busy 后任由 Agent 工作。

Workbench 后续也应通过 Host 预注册 contribution point 获得 API，而不是开放任意动态 capability。

### 13.2 插件 entry 的精确 ABI

Agent artifact 的 `main` 必须 default-export 一个 plain structural definition。推荐作者 API：

```ts
import { defineAgentPlugin } from "@ora-space/plugin-sdk/agent";

export default defineAgentPlugin({
  kind: "agent",
  pluginApi: 1,

  async activate(context) {
    return {
      providers: [/* generated AgentProvider implementations */],
    };
  },

  async deactivate() {
    // Release plugin-owned resources. Host still owns process-tree cleanup.
  },
});
```

public TypeScript 行为 ABI 由 SDK 手写为极小 structural interface，并直接引用 Rust 生成的 DTO/enum/method registry；`ts-rs` 不负责表达 callback、Promise 或 AsyncGenerator。语义等价于：

```ts
export interface AgentPluginDefinition {
  readonly kind: "agent";
  readonly pluginApi: 1;
  activate(context: ExtensionContext): AgentActivation | Promise<AgentActivation>;
  deactivate?(): void | Promise<void>;
}

export interface AgentActivation {
  readonly providers: readonly AgentProvider[];
}

export interface ExtensionContext {
  readonly plugin: Readonly<{ id: string; version: string }>;
  readonly sessionId: string;
  readonly extensionPath: string;
  readonly storagePath: string;
  readonly logger: PluginLogger;
  readonly shutdownSignal: AbortSignal;
  readonly subscriptions: SubscriptionStore;
  readonly errors: Readonly<{
    business(input: AgentBusinessErrorInput): AgentBusinessError;
  }>;
}

export type JsonValue =
  | null
  | boolean
  | number
  | string
  | readonly JsonValue[]
  | { readonly [key: string]: JsonValue };

export type AuthorBusinessFailureKind = Exclude<
  AgentBusinessFailureKind,
  "providerFailure"
>;

export interface AgentBusinessErrorInput {
  readonly kind: AuthorBusinessFailureKind;
  readonly message: string;
  readonly retryable?: boolean;
  readonly details?: Readonly<Record<string, JsonValue>>;
}

export interface AgentBusinessError extends Error {
  readonly name: "AgentBusinessError";
  readonly kind: AuthorBusinessFailureKind;
  readonly retryable: boolean;
  readonly details?: Readonly<Record<string, JsonValue>>;
}

export interface PluginLogger {
  debug(message: string): void;
  info(message: string): void;
  warn(message: string): void;
  error(message: string): void;
}

export interface Disposable {
  dispose(): void | Promise<void>;
}

export interface SubscriptionStore {
  add<T extends Disposable>(disposable: T): T;
}

export interface AgentCallContext {
  readonly requestId: string;
  readonly signal: AbortSignal;
}

export interface AgentProvider {
  readonly id: string;
  readonly contractVersion: 1;
  discoverInstallations(
    call: AgentCallContext,
    request: DiscoverInstallationsRequest,
  ): Promise<DiscoverInstallationsResponse>;
  getConfigurationSummary(
    call: AgentCallContext,
    request: GetConfigurationSummaryRequest,
  ): Promise<GetConfigurationSummaryResponse>;
  listSkills(call: AgentCallContext, request: ListSkillsRequest): Promise<ListSkillsResponse>;
  listMcpServers(
    call: AgentCallContext,
    request: ListMcpServersRequest,
  ): Promise<ListMcpServersResponse>;
  listConversations(
    call: AgentCallContext,
    request: ListConversationsRequest,
  ): Promise<ListConversationsResponse>;
  startConversation(
    call: AgentCallContext,
    request: StartConversationRequest,
  ): AsyncGenerator<AgentEvent, AgentTurnResult, void>;
  sendMessage(
    call: AgentCallContext,
    request: SendMessageRequest,
  ): AsyncGenerator<AgentEvent, AgentTurnResult, void>;
  cancelConversation(
    call: AgentCallContext,
    request: CancelConversationRequest,
  ): Promise<CancelConversationResponse>;
}

export function defineAgentPlugin<T extends AgentPluginDefinition>(definition: T): T;
```

各 method params/result/stream event、`AgentBusinessFailureKind`、method constants 与 JSON schemas 是 generated closed data types；`AgentPluginDefinition`、`ExtensionContext`、`AgentCallContext`、`AgentProvider`、`AgentBusinessErrorInput/Error`、`PluginLogger`、`Disposable` 与 `SubscriptionStore` 是上面精确定义的 SDK 手写行为 ABI。CI 必须把行为接口的 method keys/signatures与生成的 method registry 做 compile-time drift check，不能复制一份弱化为 `unknown` 的平行 DTO。`SubscriptionStore.add` 只接受一个 disposable、返回同一对象并在 successful add 后登记；bootstrap 在 stop 时只执行一次 LIFO disposal，并对每个 sync/async `dispose()` 设置有界 deadline。logger 四个方法只接受 bounded message、写 stderr且应用统一脱敏，不接受任意 metadata object。

`context.errors.business()` 在 TypeScript 与 runtime 都拒绝 bootstrap-reserved `providerFailure`；该 kind 只能由 bootstrap 对普通 throw/rejected Promise/generator throw 生成。factory 校验 finite/plain/acyclic `JsonValue`、填充 `retryable=false`、创建 private bootstrap brand，并返回上面的 public structural view；bootstrap 按 private brand识别，绝不按 SDK `instanceof` 或作者可伪造的 `name/kind` 字段识别。

冻结规则：

- `defineAgentPlugin` 是无 I/O 的 identity helper：不读取 stdin、不写 stdout、不握手、不保存全局 singleton、不注册 manifest，只返回同形 plain object。
- default export 必须是 non-null、非 Array/Function、prototype 为 `Object.prototype` 或 null 的 plain object；`kind/pluginApi/activate/deactivate` 使用 own data-property/函数 shape 验证，不执行 accessor。泛型 helper 允许作者附加字段，但 bootstrap 完全忽略它们，附加字段永远不能注册 method/权限。getter/Proxy trap 抛错或 thenable 冒充均为 `ActivationFailed`。
- 禁止依赖 definition 的 `instanceof`、private class、跨 bundle `Symbol` 或包实例身份；作者手写同形 object 接受相同验证。Provider 自身可以是 structural class instance，但 bootstrap 不做 `instanceof`。
- `activate(context)` 每 generation 恰好调用一次；bootstrap 校验 provider id/contractVersion 和全部必需 method，与 manifest 深比较，然后读取并 bind method reference，创建 immutable dispatch snapshot。之后插件修改 provider object 不得改变已安装 dispatch；额外 method 被忽略。任何 import、shape、activate、provider mismatch 都是 `ActivationFailed` 并终止 generation。
- `deactivate()` 在 successful activate 后至多调用一次。bootstrap 先关闭 dispatch、abort/drain per-call signals，再触发 `shutdownSignal`，调用 definition `deactivate()`，最后无条件 LIFO dispose subscriptions；每阶段有独立 deadline，任一清理失败都记录在 deactivate error/diagnostic，但不能跳过后续 disposal 或阻止 Job 回收。activate 在部分初始化后失败时不调用 deactivate，但仍触发 shutdownSignal 并 dispose 已登记 subscriptions。
- public package 只导出 `@ora-space/plugin-sdk/agent` 与 `./types`；private lifecycle/Frame/RPC/bootstrap/transport 不在 public exports，也不进入插件 bundle。测试 harness 需求未冻结，MVP 不导出 `./testing` 空壳。

export map 不使用 wildcard：`./agent` 是唯一包含 runtime value 的 subpath（纯 `defineAgentPlugin`），并 re-export 作者行为 ABI；`./types` 只包含 manifest 与 Agent data DTO/type，不泄漏 initialize/activate/Frame/RpcClient。发布内容是构建后的 `dist`，不发布 `src/internal`。对 `/host`、`/bootstrap`、`/internal`、reader/writer 和 private lifecycle 的 import 必须在 package test 中失败。

`ExtensionContext` v1 精确包含：plugin id/version、session id、`extensionPath`、content-owner `storagePath`、stderr-backed logger、generation-level `shutdownSignal`、subscriptions 与本地 business-error factory。它不包含 entry、`ORA_DATA_DIR`、secret facade、raw `invokeHost`、`context.ora`、`globalState` 或 `workspaceState`。`storagePath` 允许插件自管持久文件，但不是 sandbox；Host 不把其中任意内容解释为 manager state 或 secret store。

每次 provider 调用获得新的 `AgentCallContext`。bootstrap 把对应 `$/cancelRequest` 与 generation stop 组合进该调用的 `signal`；generation shutdown 另外触发 `ExtensionContext.shutdownSignal`。invocation hard deadline 只由 Host monotonic clock 裁决，并通过 §12.6 的 `$/cancelRequest` 使 bootstrap signal abort；v1 Agent DTO/envelope 不传 wall-clock timestamp，也不声称两进程时钟同步。实现若增加 bootstrap 本地 watchdog，它只能是更宽的 fail-safe 上限，超时必须按 fatal handler leak 处理，不能代替 Host cancel、提前生成 `-32800` 或改变 caller outcome。只有 provider Promise/AsyncGenerator 已 settle、不会再产出 stream，bootstrap 才能以 `-32800` 确认 transport cancellation；signal 只是请求取消，不能让 writer 在 handler 仍运行时提前伪报完成。

`context.errors.business(input)` 是纯本地 factory，不发 Plugin→Host RPC。它验证作者可创建的 closed kind、message/retryable 与有界 plain-JSON details，拒绝 BigInt、undefined、函数、循环引用、非有限 number 与自定义 prototype，并创建由当前 private bootstrap 自身可识别的 error（例如 private WeakSet/brand）。这不违反 default definition 的 structural ABI：插件不导入 brand，也不依赖 SDK 包实例。只有该 factory 创建的 error 映射 `-32000 + data.kind`；signal 已确认取消映射 `-32800`，其他 throw/rejected Promise/generator failure 脱敏归一为保留的 `-32000/ProviderFailure`，作者不能主动创建 `ProviderFailure`。stack、prompt、token 和 secret 不上 wire；`retryable` 只供 UI 提示，绝不触发自动重放。

未来若事实需求证明必须由 Host 管理 Memento/项目状态，应升级 `pluginApi` 并同时冻结 typed Request、CAS/revision、quota、损坏恢复、project identity 与 lifecycle availability。只在 initialize 中添加一个 JSON snapshot 不构成完整功能，v1 明确禁止。

---

## 14. 安全与信任模型

### 14.1 威胁模型

MVP 假设用户明确选择本地插件目录，并理解它会以当前用户权限执行代码。以下机制提供纵深防御：

- 受管复制，避免直接执行可变 source。
- manifest/路径/大小/摘要校验。
- 一插件一进程和 Job Object 生命周期隔离。
- 环境/凭据/可执行文件/路径注入 grant；不向插件开放 Host API。
- 有界 framing、JSON、队列、pending、日志和速率。
- 默认 disabled 与显式 enable。

这些机制不提供：

- 文件系统或网络 sandbox。
- 对同一 Windows 用户的秘密隔离。
- 发布者真实性。
- 对具有相同用户权限攻击者的强 TOCTOU 防护。

### 14.2 文件系统

必须防护：

- 绝对路径、`..`、UNC、盘符、ADS、设备名和尾随点/空格。
- symlink、junction、mount point 和其他 reparse point 逃逸。
- Windows case-insensitive collision。
- 文件数、单文件/总字节数和目录深度炸弹。
- 删除时路径被交换或越根。

Windows 上不能用“路径里没有冒号”代替 ADS 检查：扫描器必须通过 Win32 stream enumeration 检查每个源文件，仅把 unnamed data stream 复制到 Host 新建的 staging regular file；不得复制 named stream、安全描述符中的额外数据，也不得保留源 hardlink 关系。staging 复验与 installed scan 都必须证明：无 reparse point、无 named stream、regular file link count 恰为 1、相对路径深度不超过 64，且 canonical containment 仍成立。删除器以目录 handle 为根逐项 no-follow 删除并复验 file identity；遇到交换、额外 link 或无法枚举的对象必须停止并进入可诊断恢复状态，不能退化为字符串前缀判断。

stream audit 的 Win32 结果必须被精确解释，而不是把所有失败当成“没有 ADS”：先以排除 `FILE_SHARE_DELETE` 的 share mode pin 住待审计 object，并保持到 `FindClose` 与末次 identity 复核之后；再对该 handle 的 verified final path 调用 `FindFirstStreamW(FindStreamInfoStandard)` 并用 `FindNextStreamW` 枚举到 `ERROR_HANDLE_EOF`，枚举前后复核 object file identity。regular file 的结果集合必须恰好只有 unnamed `::$DATA`，目录允许没有 unnamed stream但任何 named `$DATA` 都拒绝。`FindFirstStreamW` 返回 `ERROR_HANDLE_EOF` 只在该对象确实无 stream 时是正常结果；`ERROR_INVALID_PARAMETER` 只在对 pinned volume 调用 `GetVolumeInformationByHandleW` 明确证明不支持 `FILE_NAMED_STREAMS` 时可作为“ADS 不可表示”接受，支持 named stream 的 volume 上则是验证失败。其他错误、枚举中 identity/metadata 变化或关闭前无法复核都 fail closed；成功取得的 search handle 必须用 `FindClose`。source identify、fresh-file copy 前后、staging commit 前、installed scan/start，以及 Host 新建的 receipt/removal marker 都复用同一审计器，避免只检查入口文件或只检查源树。

`storagePath` 在首次 activate 前由 Host 创建并以 content owner 绑定；一旦插件运行，其中所有内容都视为不可信 mutable data。Host 不扫描它来注册代码，不把其中 JSON 合并进 manager state；`remove_plugin_data` 也必须复用 handle-based no-follow 删除器，拒绝 reparse/越根，而不是因为目录由 Host 创建就递归跟随插件后来放入的链接。

`SafeTreeDeleter` 的 Windows 证明义务冻结如下，适用于 staging/trash/plugin-data，不能以一次 canonicalize + `remove_dir_all` 代替：

1. 用 `FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS` 打开并 pin 受控根与待删根；share mode 明确排除 `FILE_SHARE_DELETE`，待删根持 DELETE access。验证同卷、file identity、final path、待删根是受控根直接 child且根本身不是 reparse；无法取得 pin 时保留 PendingRemoval。
2. 从已打开 parent handle 枚举 child name，拒绝分隔符/ADS/设备名异常；以 no-follow、所需 READ_ATTRIBUTES/LIST_DIRECTORY/DELETE access 且明确排除 `FILE_SHARE_DELETE` 的 share mode 打开每个 child，并从 open 持有到该 child disposition 完成/handle close。随后查询 `FileAttributeTagInfo`、`FileIdInfo` 与 final path，证明仍是该 parent 的直接 child；拿不到这种 handle 即 fail closed。受控根、待删根和递归栈全部 ancestor handles 在后代完成前保持打开。
3. 每次 descent 与每次 disposition 前从受控根开始复核整条 handle chain 的 volume/file identity、final path 与 direct-child 关系。reparse object 绝不进入，只删除其已 pin 的链接对象；只有普通目录递归。
4. 对持有 `DELETE` access 的已验证 handle 使用文档化的 `SetFileInformationByHandle(FileDispositionInfo, FILE_DISPOSITION_INFO { DeleteFile = TRUE })` 标记 delete-on-close；本版不依赖 `FileDispositionInfoEx` 的 POSIX/ignore-readonly 扩展语义。关闭 child 并确认 disposition 成功后才推进。父目录在复核为空后按同一方式删除；若出现新 entry、identity/path/attribute 变化、sharing violation、只读/ACL 阻断或无法证明 ancestry，立即 fail closed并留给 repair，不修改 ACL/属性后重试，也不无限追逐攻击者新建文件。

MVP 只安装目录，不处理 archive，因此 zip-slip/zip-bomb 属于未来 archive installer 的强制验收项，而不是当前已有能力。

### 14.3 进程与环境

- `ProcessSpec` 需新增明确 `EnvironmentPolicy::ClearAndAllowlist`，避免调用者写难读 bool。
- Host-owned `PluginLaunchGrant` 绑定 `plugin_id + content_owner + grant_schema_version + revision`，只记录目标环境变量到 Host configuration/credential/discovered executable/authorized path reference 的映射。state 只持久化引用和授权元数据，不保存 secret value；值由注入的 resolver 在每次启动时解析为受保护内存值。
- Agent 默认基础环境只包含运行 pinned Bun 所必需且经过 E2E 的固定集合，例如 `SystemRoot/WINDIR/TEMP/TMP`。`PATH/PATHEXT/COMSPEC`、用户目录、Agent executable/config path 与 API token 都必须来自显式 grant；任何基础集合增删都属于产品安全变更。
- install 不创建 grant。用户通过独立的 `set_launch_grant` 同意具体注入后，插件才能在启动环境中获得它们；空 grant 合法，但需要额外配置的插件会结构化失败。更新或 revoke grant 会先关闭 admission、停止当前 generation，再提交 grant revision；卸载逻辑提交时删除该安装的 grant，重装不得静默复用旧授权。
- secrets/token 只按用户授权注入；日志只记录变量名、credential reference 类型和 grant revision，不记录值。credential 缺失/锁定时 start 返回 `LaunchGrantUnavailable`，不能回退为继承 Host 环境。
- 这些 grant 只约束 Host 主动注入的值，不是 OS sandbox；同用户插件仍可能直接使用 Bun/Windows API 访问用户可访问的资源。
- args 以 `OsString` 数组传递，不拼 shell 命令。
- Bun 禁止 auto-install 与自动 `.env` 加载。
- stderr ring buffer 截断时记录计数，不把全部内容常驻内存。

授权模型使用判别联合，避免把 secret 字符串混入普通配置：

```rust
pub struct PluginLaunchGrant {
    pub plugin_id: PluginId,
    pub content_owner: ContentOwnerId,
    pub schema_version: u32,
    pub revision: GrantRevision,
    pub environment: Vec<EnvironmentBinding>,
}

pub struct EnvironmentBinding {
    pub target: EnvironmentVariableName,
    pub value: LaunchValueReference,
}

pub enum LaunchValueReference {
    HostConfiguration { key: ConfigurationKey },
    Credential { key: CredentialKey },
    DiscoveredExecutable { provider: AgentProviderKey },
    AuthorizedPath { path_id: AuthorizedPathId },
}

pub enum ResolvedLaunchValue {
    Plain { value: OsString },
    Secret { value: SecretValue },
}

/// Resolves user-authorized launch references only at process launch time.
/// Implementations must not persist or log resolved values, including paths.
pub trait LaunchValueResolver {
    /// Resolves one stored reference or reports that the grant is unavailable.
    fn resolve(
        &self,
        reference: &LaunchValueReference,
    ) -> impl Future<Output = Result<ResolvedLaunchValue, LaunchValueResolutionError>>;
}
```

### 14.4 协议安全

- header 校验先于 allocation。
- frame、JSON depth、string、array、pending、queue bytes、stream rate 全部有 cap。
- 协议错误日志不原样记录整个 payload。
- decoder 必须 fuzz：任意 bytes 不得 panic、越界或分配超过 cap。
- framing 只保证消息边界，不提供认证。Host 通过专属 child pipe 与 generation/session binding 确认“此连接属于本次受管启动”；plugin id/version 来自已验证 manifest+receipt 并由 handshake 交叉检查。两者都不证明发布者身份。

---

## 15. 对外 API 与应用集成

### 15.1 Rust facade

避免 bool/含义不明的 `Option` 参数：

```rust
pub enum DataRemovalScope {
    CurrentContentOwner,
    AllOwners,
}

/// Provides serialized management operations over installed and candidate plugins.
/// Implementations must hold a process-lifetime ManagerLease and route mutations
/// through the state actor plus the per-plugin gate.
pub trait PluginManagement {
    /// Returns the current installed catalog snapshot without executing plugins.
    fn scan_installed(&self) -> impl Future<Output = Result<PluginCatalogSnapshot, PluginError>>;

    /// Scans configured roots and returns inert descriptions plus opaque selections.
    fn scan_candidates(
        &self,
        roots: Vec<DiscoveryRootId>,
    ) -> impl Future<Output = Result<Vec<CandidateSelection>, PluginError>>;

    /// Consumes one selection, validates and hashes its source, and mints a reviewed candidate.
    fn identify(
        &self,
        selection: SelectionHandle,
    ) -> impl Future<Output = Result<IdentifiedPlugin, PluginError>>;

    /// Consumes an identified candidate whose reviewed digest is bound to this session.
    fn install_authorized_candidate(
        &self,
        candidate: CandidateHandle,
    ) -> impl Future<Output = Result<InstalledPlugin, PluginError>>;

    /// Enables a supported, valid plugin after rechecking all admission gates.
    fn enable(&self, plugin_id: PluginId) -> impl Future<Output = Result<(), PluginError>>;

    /// Disables a plugin and guarantees runtime admission is closed on return.
    fn disable(&self, plugin_id: PluginId) -> impl Future<Output = Result<(), PluginError>>;

    /// Removes plugin code through the recoverable tombstone/trash workflow.
    fn uninstall(&self, plugin_id: PluginId) -> impl Future<Output = Result<(), PluginError>>;

    /// Stores a user-approved launch grant after stopping any active generation.
    fn set_launch_grant(
        &self,
        grant: PluginLaunchGrant,
    ) -> impl Future<Output = Result<(), PluginError>>;

    /// Returns only grant metadata and references, never resolved secret values.
    fn get_launch_grant(
        &self,
        plugin_id: PluginId,
    ) -> impl Future<Output = Result<Option<PluginLaunchGrant>, PluginError>>;

    /// Revokes all Host-injected launch values for the selected installation.
    fn revoke_launch_grant(
        &self,
        plugin_id: PluginId,
    ) -> impl Future<Output = Result<(), PluginError>>;

    /// Clears the persisted crash-loop policy after an explicit user action.
    fn reset_crash_loop(
        &self,
        plugin_id: PluginId,
    ) -> impl Future<Output = Result<(), PluginError>>;

    /// Removes mutable plugin data while the target owner is not running.
    fn remove_plugin_data(
        &self,
        plugin_id: PluginId,
        scope: DataRemovalScope,
    ) -> impl Future<Output = Result<(), PluginError>>;
}
```

`remove_plugin_data` 使用同一 per-plugin gate；当前 owner 必须已 stop，`AllOwners` 还需要 adapter 提供一次性 destructive-confirmation capability。它不作为 uninstall 的布尔选项，也不接受插件自身调用。

`SelectionHandle` 与 `CandidateHandle` 均由注入的 `CandidateAuthority` 生成并以 CSPRNG token 对外表示，调用者不能构造内部字段。前者绑定 management session、canonical source、source root identity、发现根/native-picker audit id、过期时间和单次 identify 权限；后者在 identify 时进一步冻结 expected id/version/tree digest、candidate audit id、session、TTL 和单次 install 权限。handle store 的 consume 必须原子化；install 在任何复制前消费 handle，复核 root identity，并在 staging 上重新验证/计算 digest；identity 或 digest 与用户刚确认的事实不等即返回 `SourceChanged`，不得让调用者用同一 token 重试。纯扫描器/复制器仍可在内部接收 `PathBuf`，但路径不穿过公开 facade 或 HTTP DTO。

运行 API 与管理 API 分开：

```rust
/// Starts, stops, and invokes only effective-enabled Agent plugins.
/// Implementations own generation isolation and never expose the stdio protocol.
pub trait AgentPluginRuntime {
    /// Starts or joins the plugin's single-flight generation.
    fn start(&self, plugin_id: PluginId) -> impl Future<Output = Result<(), PluginError>>;

    /// Stops and reaps the complete managed process tree for one plugin.
    fn stop(
        &self,
        plugin_id: PluginId,
        reason: StopReason,
    ) -> impl Future<Output = Result<(), PluginError>>;

    /// Invokes a typed Agent contract and returns its bounded event handle.
    fn invoke(
        &self,
        request: AgentInvocation,
    ) -> impl Future<Output = Result<AgentInvocationHandle, PluginError>>;
}
```

### 15.2 组合根

当前 `apps/web/server/src/bootstrap.rs:18-39` 构造现有 API 并注入 `AppState`，`AppState` 位于 `apps/web/server/src/app_state.rs:7-29`；当前 Tauri 并未启动它。MVP 冻结以下 Windows 生产拓扑，而不是保留条件句：

1. 把 web server 的 bootstrap 提取为可嵌入的 `BackendRuntime` library；`apps/desktop/src-tauri` 是 packaged desktop 的最外层生命周期 owner，并在本进程的 managed async task 中构造且仅构造一个 BackendRuntime，不再另起一个拥有 manager 的 sidecar。library 不创建第二个 Tokio runtime、不安装 global tracing subscriber、不注册 Ctrl-C/OS signal；这些由 Tauri 与 standalone binary 各自的 composition root 拥有。
2. BackendRuntime 构造真实 state store、scanner、validator、process-tree spawner、runtime supervisor 与 manager，并持有 `ManagerLease` 到 shutdown。`AppState` 持有 `Arc<PluginApi>`。
3. Tauri 通过 typed startup config 传递唯一 `ORA_DATA_DIR`；production backend 固定 bind `127.0.0.1:0` 后把实际端口通过内存 bootstrap channel 交给 WebView。当前 `0.0.0.0:32578` 默认必须改变；MVP production 禁止 wildcard/LAN bind。
4. reconcile、installed scan 和认证初始化完成后才标 readiness/开放 WebView；bootstrap 失败则桌面启动失败并显示诊断，不能启动一个无 manager 的降级写者。
5. Tauri application `ExitRequested` 触发一次 §10.4 shutdown；window close 只遵循产品窗口策略。正常路径完全 drain/join 后才释放 ManagerLease；hard-timeout 路径保持 lease并无条件退出进程。若 server task/manager supervisor 意外退出，立刻关闭 WebView/API admission并进入同一 fail-closed 全局退出，不留下“UI 可用但 backend writer 已死”的状态。
6. standalone `apps/web/server` binary 仅作为开发/测试组合根，使用同一个 BackendRuntime；manager.lock 保证它不能与 packaged runtime 同时写同一 data dir。

### 15.3 Adapter/API

HTTP 路径、管理面 request/response 与 adapter stream envelope 在 `ora-contracts` 定义，由现有 ts-rs 生成链导出到 TypeScript；handler/service/routes 只能引用 contract 常量。Agent domain/wire DTO 的唯一真相源始终是 `ora-plugin-protocol`：`ora-contracts` 只能直接引用这些 Rust types，或定义带显式转换测试的 application wrapper，绝不能复制 Agent event/result/error shape。最小管理面统一使用现有 `/api` 前缀：

```text
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
POST   /api/plugins/{id}/start
POST   /api/plugins/{id}/stop
POST   /api/agent-invocations
DELETE /api/agent-invocations/{invocation-id}
```

路径敏感 DTO 固定为：`scan { rootIds: DiscoveryRootId[] }`、`identify { selectionHandle: SelectionHandle }`、`install { candidateHandle: CandidateHandle }`。响应中的 handle 是不透明 bearer string；`CandidateSelection` 只含安全展示字段与 selection handle，`IdentifiedPlugin` 才含完整 diagnostics、reviewed id/version/digest/risk summary 与 candidate handle。DTO 使用 strict unknown-field rejection；同一个请求不能同时携带 path、第二种 handle 或客户端声明的 identity/digest。

`POST /api/agent-invocations` 返回 authenticated HTTP streaming response（`application/x-ndjson`），response header 包含 opaque invocation id；每行是 compact、单行的 application envelope（`event | completed | failed`），其中 Agent event/result payload 直接引用 `ora-plugin-protocol` 类型，不是 plugin stdio Frame，字符串中的换行由 JSON escape。fetch body abort 与显式 DELETE 都触发 cancellation，最终结果仍遵守 Written/UnknownOutcome 规则。adapter 只映射 `AgentInvocationHandle`，不接触 plugin framing；invocation id/tombstone 的保留期、单行大小和流的 event/byte buffer 同样有界。

WebView invocation DTO 可以引用 Ora application model 中已有的 project/worktree id，但不能携带 cwd。Application 层在 admission 时验证这些 id 仍属于本 session、解析当前 canonical working directory，并构造 §13.1 的 `AgentScope`；解析失败在写 plugin frame 前返回 application error。这样项目上下文既不是 initialize 全局快照，也不需要 v1 Plugin→Host path API。

本地 HTTP 不是“天然可信”。BackendRuntime 每次启动用 CSPRNG 生成至少 256-bit、仅存内存的 session bearer，通过 Tauri 内存 bootstrap/IPC 交给 WebView；不得出现在 URL、日志、持久 localStorage 或命令行。全部 `/api/plugins*` 与 invocation endpoint（包括 GET）要求 `Authorization: Bearer`，使用常量时间比较，并精确校验 Host 为实际 loopback endpoint、Origin 为运行时 allowlist；不使用 cookie，因此没有 cookie fallback。缺失/错误 Origin、跨站 form/navigation、wildcard CORS 全部拒绝；WebView 还需严格 CSP，避免第三方脚本取得内存 token。Tauri dev 的 Vite host 也改为 loopback并继续通过 Tauri IPC 提供 token；没有安全 bootstrap channel 的普通浏览器开发模式不挂载 plugin/invocation routes，不能关闭认证来求方便。

携带 `Authorization` 的跨-origin fetch 会先发送不含 bearer 的 CORS preflight，因此 `OPTIONS` 只豁免 bearer，绝不豁免安全校验：先验证实际 loopback Host、exact/non-null Origin、`Access-Control-Request-Method` 属于该 route 的固定集合、requested headers 仅为大小写规范化后的 `Authorization`/`Content-Type`；OPTIONS 不进入业务 handler。成功 preflight 只回显该 exact Origin、允许的 method/headers 与 `Vary: Origin`，禁止 wildcard 和 credentials。实际 success/error/stream response 都带同一 exact `Access-Control-Allow-Origin`/`Vary`；错误不能因缺 CORS header 而在前端变成不可诊断的 opaque network failure。

前端永远不能提交任意本地 path。Tauri native picker 在可信 IPC 回调中把 OS 返回路径交给 `CandidateAuthority` 并只把 `SelectionHandle` 给 WebView；scan 只能引用 Host 配置的 `DiscoveryRootId`，返回的每项同样携带 selection handle。`POST /identify` 消费 selection handle，返回可展示的 manifest/diagnostics/digest 与新的 `CandidateHandle`；`POST /install` 只消费 candidate handle，不接受 path、id、version 或客户端回传 digest 作为授权依据。handle 都绑定本次 bearer session、短 TTL、用途和单次消费；过期、跨 session、重放和 staging digest 变化分别稳定返回结构化错误。上述 bearer、Origin、handle 的正反 E2E 是开放插件 API 的前置门禁。

endpoint/token bootstrap 与 native-picker IPC command 只授权 Tauri 标识的主 Workbench window，并绑定本次 readiness/session；未 ready 的 WebView、未来 plugin WebView 或任意新窗口不能取得 token或签发 selection。窗口 identity 不是插件 identity，不能被 HTTP body 冒充。

---

## 16. 错误、事件与可观测性

### 16.1 内部错误与边界错误

内部错误应精确，边界允许稳定归一化，不要求一一双射：

```rust
pub enum PluginError {
    NotFound { plugin_id: PluginId },
    AlreadyInstalled { plugin_id: PluginId },
    InvalidManifest { diagnostics: Vec<PluginDiagnostic> },
    UnsupportedSchemaVersion { manifest_version: u32 },
    UnsupportedPackageLayout { diagnostics: Vec<PluginDiagnostic> },
    Incompatible { reason: CompatibilityReason },
    UnsupportedKind { kind: PluginKind },
    IntegrityMismatch { plugin_id: PluginId },
    MissingInstallFiles { plugin_id: PluginId },
    Disabled { plugin_id: PluginId, reason: EffectiveDisableReason },
    InstallConflict { plugin_id: PluginId },
    SelectionHandleInvalid { reason: SelectionHandleFailure },
    CandidateHandleInvalid { reason: CandidateHandleFailure },
    SourceChanged { reason: SourceChangeReason },
    RecoveryRequired { operation_id: OperationId, diagnostic: PluginDiagnostic },
    RemovalPending { plugin_id: PluginId },
    StateCorrupt { message: String },
    StateVersionUnsupported { schema_version: u32 },
    PersistenceUncertain { operation_id: OperationId, diagnostic: PluginDiagnostic },
    DataDirInUse,
    PluginRuntimeUnavailable { diagnostic: PluginDiagnostic },
    LaunchGrantUnavailable { plugin_id: PluginId, binding: GrantBindingKey },
    ProcessSpawnFailed { plugin_id: PluginId, message: String },
    HandshakeFailed { plugin_id: PluginId, reason: HandshakeFailure },
    ActivationFailed { plugin_id: PluginId, reason: ActivationFailure },
    DeactivationFailed { plugin_id: PluginId, reason: DeactivationFailure },
    ProtocolViolation { plugin_id: PluginId, reason: ProtocolFailure },
    TreeKillUnavailable { plugin_id: PluginId, diagnostic: PluginDiagnostic },
    TreeCleanupTimeout { plugin_id: PluginId, generation: u64 },
    BackpressureExceeded { plugin_id: PluginId, request_id: String },
    AgentContractViolation {
        plugin_id: PluginId,
        request_id: String,
        reason: AgentContractFailure,
    },
    TransportFailed { plugin_id: PluginId, request_id: String, stage: TransportFailureStage },
    RequestTimedOut { plugin_id: PluginId, request_id: String },
    Cancelled { plugin_id: PluginId, request_id: String },
    PluginExited { plugin_id: PluginId, exit_code: Option<i32> },
    UnknownOutcome {
        plugin_id: PluginId,
        request_id: String,
        cause: UnknownOutcomeCause,
    },
}

pub enum SourceChangeReason {
    RootMissing,
    RootIdentityMismatch { expected: SourceRootIdentity, actual: SourceRootIdentity },
    StagingValidationFailed { diagnostics: Vec<PluginDiagnostic> },
    PluginIdentityMismatch { expected: ReviewedPluginIdentity, actual: ReviewedPluginIdentity },
    ContentDigestMismatch { expected: TreeDigest, actual: TreeDigest },
}

pub enum TransportFailureStage {
    RequestWrite,
    TransportCancelWrite,
    ResponseRead,
    SessionDrain,
}

pub enum FatalSettlementCause {
    ConnectionLost { stage: TransportFailureStage },
    ProcessExited { exit_code: Option<i32> },
}

pub enum AgentContractFailure {
    InvalidRequestDto,
    InvalidStreamEvent,
    InvalidTerminalResult,
    InvalidBusinessError,
    ConversationCorrelation,
    ActiveTurnCollision,
    GeneratorProtocol,
}
```

`SelectionHandleFailure`/`CandidateHandleFailure` 内部至少区分 unknown、expired、wrong-session、wrong-purpose 与 already-consumed，便于审计；HTTP 边界把这些统一映射为不泄露 token 存在性的稳定 `InvalidAuthorizationHandle`，但将 reason 以 metadata-only 方式记录。`TransportFailureStage`、`FatalSettlementCause` 与 `AgentContractFailure` 只表达稳定分类；敏感或攻击者可控细节进入有界、脱敏的 `PluginDiagnostic`/tracing，不扩展为自由文本 enum。`UnknownOutcomeCause` 是 closed enum（`DeadlineExceeded`、`CancellationUnconfirmed`、`ConnectionLost`、`ProcessExited`），不得退化为自由文本。

wire v1 的错误处置必须同时满足“JSON-RPC 语义可诊断”和“坏 session fail closed”，不能把所有异常混成断管，也不能对无法关联的垃圾无限回包：

| 条件 | 出站行为 | session 行为 |
|---|---|---|
| 完整、长度合法且 `type=Request`，但 UTF-8/JSON/duplicate-key/depth 无效 | writer 可用时在短 control deadline 内 best-effort `id:null, -32700` | 回包成功与否都 fatal |
| JSON 可解析且 `type=Request`，但 envelope/batch/id/method shape 无效 | best-effort `id:null, -32600` | fatal |
| 完整合法 Request 的未知 method | 同 id `-32601` | 一般继续；v1 收到任何 Plugin→Host Request 时回包后 fatal，因为方向本身违约 |
| 已知 method 的 typed params 无效 | 同 id `-32602` | 继续 |
| bootstrap/router 内部异常 | 同 id `-32603` | 按异常分类继续或 fatal；不得把普通 Agent 业务失败伪装成 internal error |
| Agent provider 的预期业务失败 | 同 id `-32000`，`data.kind` 为生成的 closed enum | 继续 |
| ordinary handler admission 已满 | 同 id `-32010` (`ServerBusy`) | 继续；`agent.cancelConversation` 不走该 lane，若收到 `-32010` 则 Host 立即 terminate Job |
| cancellation 已被 Plugin 接受并成为终态 | 同 id `-32800` (`RequestCancelled`) | 继续 |
| 非法 Response/Notification、未知 frame type、坏 length、partial EOF | 不回 JSON-RPC error | fatal |

error envelope 固定为 `{ code: i32, message: string, data?: object }`。除前两行无法取得合法 id 的错误哨兵外，wire id 仍只允许 non-empty string；`id:null` 不是普通请求能力。`-32700/-32600/-32601/-32602/-32603` 采用 JSON-RPC 2.0 定义；`-32000..-32099` 是 server-reserved 区间；`-32800` 明确借用 LSP 的取消约定，是 Ora 扩展而非 JSON-RPC core。`AgentBusinessFailureKind` v1 **精确**冻结为 `AgentUnavailable`、`AuthenticationRequired`、`InvalidAgentConfiguration`、`InstallationNotFound`、`ConversationNotFound`、`UnsupportedAgentCapability`、`InvalidState`、`PermissionDenied`、`CursorExpired`、`AgentProcessFailed` 与 bootstrap-reserved `ProviderFailure`；JSON discriminant 使用这些 Rust variant 的 lowerCamelCase 拼写，新增成员属于 Agent contract 变更。`-32000` data 精确为 `{ kind, retryable, details? }`，details 受 method byte/depth schema 与脱敏策略约束。

Host 管理错误不必全部伪装成 plugin JSON-RPC error，它们属于 application contract。

### 16.2 事件

```rust
pub enum PluginEvent {
    CatalogChanged { revision: u64 },
    RegistryChanged { revision: u64, added: Vec<PluginId>, removed: Vec<PluginId> },
    EnablementChanged { plugin_id: PluginId, effective: EffectiveEnablement },
    RuntimeChanged { plugin_id: PluginId, generation: u64, state: RuntimeStateView },
    InstallProgress { operation_id: String, phase: InstallPhase },
    Diagnostic { plugin_id: Option<PluginId>, diagnostic: PluginDiagnostic },
}
```

事件通道有界；慢消费者可收到“revision changed，请重新取 snapshot”，不能让消费者阻塞 registry/runtime actor。

### 16.3 日志字段

建议字段：`operation`、`operation_id`、`plugin_id`、`plugin_version`、`kind`、`generation`、`request_id`、`method`、`frame_type`、`payload_bytes`、`queue_bytes`、`duration_ms`、`exit_code`、`reason`。

禁止默认记录：payload、prompt、token、响应正文、完整配置、env value、credential path 内容。stderr 需要脱敏/截断策略并标识来源。

---

## 17. 测试与验收

### 17.1 单元与 property/fuzz

Manifest/路径：

- strict schema、未知字段、任意嵌套 object duplicate key、JSON depth=64/65 边界。
- id/SemVer/engine/kind/contribution 全边界。
- absolute/UNC/盘符/`..`/设备名/尾随点空格；Win32 stream enumeration 能检出 named ADS。
- case collision、symlink/junction/reparse escape、hardlink count≠1、目录深度 64/65 边界。
- entry regular file 与 canonical containment。
- unknown manifestVersion 与 known-v1 invalid 的分类；materialized bundle、unresolved external、node_modules/native addon 拒绝。
- tree digest 排序、长度编码与跨运行稳定性。

Framing：

- 每个字节一个 chunk。
- 对每个 cut position 拆分 header/payload。
- 一个 chunk 多帧与尾部 partial frame。
- UTF-8 多字节跨 chunk。
- max-1、max、max+1、zero、negative。
- boundary EOF 与 header/type/payload partial EOF。
- unknown/negative type、invalid UTF-8、invalid JSON、batch array。
- type/envelope mismatch、result/error XOR、duplicate key、JSON depth。
- 任意 fuzz bytes 不 panic/越界/超预算分配。
- 第 12.9 节单一 golden fixture 的 Rust↔TS 双向互操作。

Runtime：

- N 个 concurrent first invoke 只 spawn 一次。
- start/disable、start/uninstall、invoke/stop 全排序。
- old registry snapshot + disable epoch 被拒绝。
- result/cancel/timeout/exit 所有竞争只有一个终态；NotWritten、WriteStarted→FrameWritten、WriteStarted→WriteFailed(0/partial/unknown bytes)、Written/PossiblyWritten 与 idempotent/non-idempotent 的结果矩阵逐项覆盖。另强制调度 response/stream-before-writer-ack，并分别在 inbound 与 ack 中间插入 explicit cancel、hard deadline；断言按原 actor sequence 重放，WriteFailed 不采纳 deferred inbound，结果不随 task 调度改变。
- Starting→CancellingStart 的迟到 spawn success/failure；late tree 必须进入 CleanupPending、terminate Job、wait 后才允许新 generation。
- exit-first、EOF-first、protocol-error-first 三种 Draining 排列；Initializing/Activating/Running 的 direct-exit 先到时必须在同一 actor turn请求 terminate Job。fixture 让孙进程持有 stdout并尝试继续运行，同时在 direct process退出前写入完整缓冲 response：断言孙进程被立即终止、buffered response 仍在 boundary EOF 前正确完成、tree-empty 不提前且 controller 可并发 terminate。Stopping 的预期 direct-exit则只在 tree grace 到期后升级。
- 无 termination intent 的 `ConnectionLost/ProcessExited × NotWritten/Written/PossiblyWritten × idempotent/non-idempotent` fatal-settlement 矩阵逐项覆盖；EOF 与 direct-exit 的先后锁存对应 cause。另强制 deferred terminal→fatal、fatal→terminal-before-boundary-EOF、fatal→boundary-EOF-no-terminal，证明合法 terminal 与 WriteFailed 的 causal gate、以及 `TransportFailed/PluginExited/UnknownOutcome` 分类不会因 drain task 调度漂移。
- `StopEscalation→bystander D_inv→direct-exit` 与 `StopEscalation→direct-exit→bystander D_inv` 必须给 bystander 相同 `ConnectionLost(SessionDrain)`；normal disable/uninstall/shutdown 先为 active ordinary request登记 HostStop，且 HostStop 与 caller cancel/backpressure/deadline 的先后按 first-intent 测试。
- old generation late response/exit 不影响新 generation。
- writer queue 满、byte budget 满、control reserve 可用；普通 request writer failure 与 transport-cancel writer failure分别携带一个 Written idempotent/non-idempotent bystander，断言当 writer failure 是 primary trigger 时 stage 固定为 `RequestWrite/TransportCancelWrite`，失败 request保留自身矩阵，bystander不因 EOF/direct-exit 后到而改成 `PluginExited`。SessionControl writer failure 是首个 fatal 时必须锁存 `ConnectionLost(SessionDrain)`；`ProtocolFailure` 先成为 primary 并锁存 `ConnectionLost(ResponseRead)`、随后 fatal diagnostic `WriteFailed(SessionControl)` 的反向排列必须保持原 primary trigger/cause，只增加 secondary diagnostic。
- cancel before enqueue/queued/WriteStarted/Written 各路径；WriteStarted 保存 ordered deferred events，partial write 的 non-idempotent outcome 必为 UnknownOutcome。
- 分别强制 `ExplicitCancel/HostStop→HardDeadline`、`Backpressure→HardDeadline`、`HardDeadline→ExplicitCancel/HostStop/Backpressure`，每种再覆盖 cap 前 terminal 与无 terminal。前两类 cleanup 不得越过 `D_inv`、后到 deadline 不改写 first-intent cause；hard-deadline-first 才获得独立 bounded post-outcome cleanup grace。相同 monotonic instant 由 actor sequence 唯一裁决。
- cancel/timeout 已完成 caller 后，wire tombstone 仍能由 terminal Response 回收；普通 Written hard-deadline-first 必须实际写出 `$/cancelRequest`，control-write/post-outcome cleanup 任一超时都终止 Job，不能留下永久 handler/tombstone。
- stream seq gap/duplicate/late/consumer backpressure；backpressure-first 在 cap 前有 terminal 时统一 `BackpressureExceeded`，无 terminal时 idempotent 仍为该错误、Written non-idempotent 只能是 `UnknownOutcome(CancellationUnconfirmed)`，不能被后到 deadline 改写。
- private bootstrap ordinary Agent handler executor 满载返回 `ServerBusy`；同一负载下 `cancelConversation` 仍进入独立 safety executor，safety lane 不足或异常 `-32010` 会终止 Job；safety terminal→deadline 与 deadline→terminal（含 terminal-before-writer-ack）按 sequence 分别验证。safety action 接受后的 zero/partial/unknown write、connection loss 与 grace timeout 均返回 `CancellationUnconfirmed`，不得落入普通 idempotent `TransportFailed`。Bun stdout writer backpressure、data flood 下 cancel/terminal/exit 各非借用 control reserve 仍可达。
- exact initialize DTO 对 `maxFrameBytes` 非 8 MiB、六项动态 limit 的 0/上限/上限+1、以及试图下发固定 leaf/string cap 的额外字段都拒绝；Host/bootstrap validator 使用同一实际六项值。`pluginApi` mismatch、activate provider descriptor 缺失/额外/重复、activate failure 均不能进入 Running；successful activate 后零/一次 deactivate，随后恰好一次 exit notification。
- 任意 Plugin→Host Request、旧 NDJSON 行消息、`plugin.ready`/`$/ready`、activate 前 Agent traffic 均被拒绝且不会污染下一 generation。
- stderr flood 不阻塞 stdout；pipe 读取继续、ring 保留有界且 `dropped_bytes` 递增。

测试遵循仓库规则：`pretty_assertions::assert_eq` 比较整个对象；tracing callsite 全部位于 test-scoped TRACE subscriber 下；不修改进程环境，从上层注入环境策略。

### 17.2 安装 fault injection

在每个阶段模拟错误/进程退出并重启 reconcile：

1. staging 创建后。
2. 复制中。
3. staging 验证前/后。
4. receipt 写入前/后。
5. PendingInstall Prepared 的 temp/replace 前后。
6. final rename 前/后。
7. PendingInstall FilesCommitted 的 temp/replace 前后。
8. installed+disabled/clear-pending state temp/replace 前后。
9. registry event 前。
10. uninstall tombstone 写入前/后。
11. stop 后、trash rename 前/后。
12. PendingRemoval FilesMoved 写入前/后。
13. remove-record+clear-tombstone 写入前/后。
14. trash delete 中。

另覆盖授权链：selection 过期/跨 session/重放、candidate 过期/跨 session/重放、identify 后替换 source root identity、修改源目录、复制时源 file identity 变化以及 staging digest 与 reviewed digest 不同。所有路径都不得安装 bytes；`CandidateHandle` 一经 install 尝试即失效。

状态恢复分别注入 primary 损坏但 `state.previous.json` 有效、两份都损坏、backup 版本不兼容三种情形；只有第一种可自动恢复，且恢复出的全部插件强制 disabled、launch grants 全部清空。第一种还要分别令 invalid primary 带 named ADS、reparse identity 与 extra hard link，证明 special recovery 先 handle-rename 隔离、再 no-replace 首次创建 clean primary，绝不经 `ReplaceFileW` 继承旧 metadata；在“隔离后、MoveFileExW 前”kill 后，下一 bootstrap 必须从 primary-missing + valid-backup 幂等恢复。另以“revision N 有 grant → N+1 revoke → primary N+1 损坏 → backup N 恢复”证明 revoked credential/path reference 不会复活。后两种必须保持 final 为 Untracked/RecoveryRequired，不得由 receipt 猜测用户意图。

断言：staging 永不执行；只有 matching install intent 可收养 final；untracked final 保持隔离；未知状态永不自动 enabled；失败安装不破坏既有安装；repair 幂等并最终收敛；pending removal 的 final/trash 任意组合在重启后不复活且不残留 tombstone。

### 17.3 Windows E2E

- 真实 Bun bootstrap 完成 5-byte framing、initialize、activate；activate success 后才允许首个 Agent Request，stop 完成 deactivate/exit。
- packaged runtime asset 的版本、摘要、部署/恢复、损坏 fail-closed；系统 PATH 中的其他 bun 不会被使用。
- 插件启动孙进程；graceful/forced stop 后整棵树退出。
- 强制终止 Host；Job handle close 后 Bun 与后代退出。
- cwd 为受管插件根，环境只有 allowlist，`--no-install --no-env-file` 生效。
- 插件根和测试用户目录各放置带 `preload` 的 `bunfig.toml`，显式 Host config 必须保证 preload 未执行、stdout 未被提前写入。
- 非 ASCII 路径、entry 与 payload。
- 每字节 pipe writer、coalesced frames、8 MiB 边界、partial EOF。
- disable/uninstall 后无可调用 runtime；文件占用进入可恢复 PendingRemoval。
- source/staging/installed root 的 reparse point、named ADS、hardlink、深度超限与 delete-time path swap 攻击不会越根、保留隐蔽 stream 或误删外部 sentinel；特别在 child no-delete-share handle 打开后、disposition 前尝试 rename 到根外必须失败/使删除 fail closed；staging/installed file link count 始终为 1。
- production/Tauri-dev backend 与 Vite 只监听 loopback；无 bearer、错误 Origin、wildcard Host、CSP 违规、重放/过期/跨 session selection/candidate handle 均被拒绝，合法 Tauri bootstrap 可完成 identify/install/invoke；普通 browser-dev 不挂载高权限 routes。
- Tauri close/崩溃、standalone backend 与同 data dir 竞争时，只有一个 ManagerLease/状态写者。

### 17.4 SDK 质量门禁

先修复当前门禁：

- Taskfile filter 与真实 package name 一致。
- test runner 递归发现 public SDK package 与 private-bootstrap runtime package 中全部 `*.test.ts`/`*.spec.ts`；两个 suite 都打印匹配文件与非零测试数，不继续把旧 `tests/host/*` 目录名当作未来契约。
- typecheck、Bun runtime tests、生成类型 drift check 真实运行。
- public exports 只提供稳定的 `./agent` 与作者可见 `./types`；private bootstrap/transport/lifecycle 不可从作者包导入，MVP 不导出 `./testing` 空壳。
- 删除旧 `getNums/returnNums`、NDJSON reader/writer、`./host` 与旧 manifest 类型，不提供兼容 shim；`defineAgentPlugin` 的手写同形 object、重复 SDK bundle 和 pack 后 import 都通过 structural ABI 测试。
- pack 后 fixture 只用 public import，完成真实 Rust↔Bun pipe E2E。
- 每个 Agent request/result/event/error/`AgentScope` variant 都有 Rust encode→TS decode→TS encode→Rust decode golden；所有 leaf newtype 断言透明 primitive projection。额外覆盖 `claude-code`/首尾连字符/点/Unicode provider id、`AgentPrompt` string 而非 object、configuration-key 512/513-byte 与非法字符、opaque id/cursor 256/257-byte、limit 0/1/100/101、RFC3339、UUID 与 `JsonSafeU64` 的 `2^53-1/2^53` 边界。
- provider/installation/conversation/cursor 不能跨 plugin content owner、provider、generation 或 session 路由；configuration fixture 证明 secret 只能成为 `redacted`，`locationDisplay`/`workingDirectory` 不会被 manager 反向解释为安装或文件授权。
- `startConversation` provisional request-key、首事件原子绑定、重复 conversation-key 冲突、`sendMessage` 禁止 `conversationStarted`、conversation id correlation、AsyncGenerator 唯一 terminal return 与非 streaming method 发 stream的正反测试全部通过。
- per-call transport cancel 只 abort 目标 invocation；generation `shutdownSignal`、business `cancelConversation`、caller detach、同会话并发 send、重复 cancel single-flight、active-turn terminal→cancel Response ordering 与 safety reserve exhaustion 分别测试，不能用一个 cancellation fixture 代替。
- branded business error、普通 Error/Promise reject、generator throw、非法 result/event DTO 分别映射到预期 business failure、reserved `providerFailure` 或 fatal `AgentContractViolation`；不得依赖跨 bundle `instanceof`。
- helper/手写 descriptor/重复 SDK bundle、class/array/function/null/thenable/Proxy/getter、额外字段忽略、activate 后 provider mutation、failed activation cleanup、deactivate throw 后 LIFO dispose 都有 runtime contract test。
- package export negative test 证明 `/host`、`/bootstrap`、`/internal`、transport/lifecycle/reader/writer 均不可导入；pack allowlist、external/dynamic import、native addon、parent `node_modules` sentinel 与 Unicode relocation tests 均执行。

`task test` 必须输出匹配到的 SDK package 和测试数量，避免“命令成功但没有执行测试”。

### 17.5 MVP 签收条件

全部满足才可签收：

1. 扫描候选不会修改或执行候选目录。
2. staging 复验之前没有 bytes 进入最终安装目录。
3. 任一安装崩溃点重启后不存在半安装可执行状态。
4. 无 matching pending intent 的 final 不会被自动收养；卸载在 trash rename 前后崩溃均收敛且不复活。
5. 安装默认 disabled；disabled/invalid/incompatible/unsupported/missing-files/pending removal 均不能 spawn。
6. 一个合法 Agent 与一个合法 Workbench 同时安装：Agent 可按需运行，Workbench enable 返回 `UnsupportedKind`、用户意图不变且不运行。
7. 同一 Agent 的并发首次调用只有一个 Bun PID/generation；迟到 spawn 不泄漏也不并发新 generation。
8. initialize 与 activate success 前零业务请求；不存在第三个 ready 状态，provider descriptor 与 manifest 必须精确一致。
9. frame split/coalescing、UTF-8、padding/BE golden、超限和 EOF 测试全部通过。
10. 所有 buffer、queue、pending、stderr 与 stream 都有可观测上限。
11. cancel/timeout/result/exit 只交付一个终态；Written non-idempotent 无 terminal response 必为 UnknownOutcome，且从不自动重放。
12. plugin crash 精确失败 pending；达到阈值后进入持久 CrashLoop，重启不复位，显式 reset 才恢复。
13. grant secret 不进 state/log；grant revoke 会停止 runtime，卸载/重装不静默继承授权。
14. disable/uninstall 与应用重启后插件不会复活。
15. Host crash 后无 Bun 或 Agent 后代残留。
16. production loopback+bearer+Origin+SelectionHandle/CandidateHandle 防护与 source-changed digest binding 通过正反 E2E。
17. 卸载完成时 registry/runtime/install record 均已移除；trash 可稍后清理。
18. 真实 Rust↔Bun E2E 与 SDK public-package test 在 `task test` 中被执行。
19. `cargo fmt --all`、workspace clippy/test 与文档引用检查通过。
20. per-call transport cancel 只有在目标 handler/generator settled 后才返回 `-32800`；business cancel 只有在 active turn 的 cancelled terminal frame 写成后才返回 `Accepted`，任一 safety/control lane 失败都会回收 Job而不会留下继续计费的 Agent。
21. 每个 Agent DTO/事件/错误/Scope 都通过 Rust↔TS golden、边界与跨 provider/session negative test；SDK public/private export、structural ABI 与 materialized pack 门禁全部通过。
22. Windows Job 在插件代码执行前原子绑定，只有三个 child stdio handle 可继承；Host crash、pipe EOF、tree-empty 与 delete-time path swap E2E 均证明无残留进程和越根删除。
23. primary 损坏时 backup recovery 必须强制全部 disabled、清空 stale launch grants，并先持久化新 primary 才 readiness；未来 schema、双份损坏、replace commit 不确定均 fail closed，不扫描 final 猜测用户意图。
24. CORS preflight 只豁免 bearer且仍执行 exact Host/Origin/method/header 检查；实际 success/error/stream response 均返回 exact Origin 与 `Vary: Origin`。

---

## 18. 实施里程碑

### M0：规范与门禁

- 冻结 manifest v1、Frame v1、错误码与 golden fixtures。
- 修复 SDK package filter、test discovery 和 public exports；删除旧 NDJSON/getNums/returnNums API，不保留兼容层。
- 删除 plugin stdio 对 NDJSON 的未来承诺；应用 HTTP adapter 的流格式是独立 contract，不与 5-byte plugin wire 混用。
- 锁定 Bun/runtime asset manifest、bootstrap bundle 与 Tauri resource 构建校验。

### M1：模型、扫描与状态

- 实现 identity/manifest/validation/catalog/effective enablement 与 selection→candidate digest-bound authorization。
- 实现 state 单写者、进程生命周期 ManagerLease、pending operation/launch grant/crash policy、receipt/tree digest。
- candidate/installed scan 与 invalid diagnostics。
- Workbench kind 进入 schema/catalog，runtime support 明确为 unsupported。

### M2：安装、卸载与恢复

- CandidateHandle→fresh-file staging→digest equality/verify→receipt→pending intent→rename→state。
- disable/uninstall/tombstone/trash/safe delete。
- bootstrap reconcile 与 fault injection。

### M3：进程树与 Frame codec

- `ProcessSpec` 环境清理策略。
- Windows Job Object 的无逃逸 spawn/kill/wait。
- Rust/TS 5-byte frame reader/writer、golden/property/fuzz。
- stderr 有界 drain。
- Host-owned Bun config；验证插件/用户 `bunfig.toml` preload 无法先于 bootstrap 执行。
- versioned Bun/bootstrap asset 的打包、摘要复验、原子部署、升级 lease 和缺失恢复。

### M4：Runtime actor 与握手

- start single-flight、generation 状态机、writer/reader/exit actor。
- initialize/activate、deactivate/exit、pending router、deadline、cancel、stream、backpressure。
- CancellingStart/CleanupPending、持久 crash window/CrashLoop 与 Written-aware UnknownOutcome。

### M5：Agent contract 与应用接入

- Agent Contract v1 DTO、SDK facade 和真实 fixture。
- `ora-contracts` DTO/path/TS export、BackendRuntime、Tauri production composition、loopback bearer/Origin/SelectionHandle/CandidateHandle、`AppState`、readiness、shutdown。
- authenticated HTTP invocation stream、abort/delete cancellation 与前端 consumer。
- 两个 Agent provider 并存与选择 E2E。

每个里程碑合入前必须满足其故障注入与不变量测试；不能等到 M5 再补安装/协议原子性。

---

## 19. VS Code 借鉴、裁剪与强化

| 机制 | 一手证据 | Ora 决策 |
|---|---|---|
| manifest→scanned→installed→runtime description 分层 | `src/vs/platform/extensions/common/extensions.ts`；报告 §3 | 借鉴，增加 validity/compatibility/support/integrity 正交状态 |
| scanner 生成 validations，调用方可选择是否包含 invalid | `extensionsScannerService.ts:674-766`；报告 7.3-7.4 | 借鉴诊断分层，但不声称 VS Code 默认总保留 invalid；Ora catalog 明确始终保留受管坏候选供诊断且绝不运行 |
| engine/manifest 校验 | `extensionValidator.ts:242-343` | 借鉴；VS Code 对部分 entry 越界只 warning，Ora 强化为 hard error |
| 临时目录解压后 rename | `extensionManagementService.ts:620-697`；报告 8.4 | 借鉴同卷 staging；增加 receipt、state commit 与 reconcile |
| profile metadata/manifest metadata | `extensionsScannerService.ts:38-44,706-718` | 不照搬写回 manifest；Ora manifest 不可变，metadata 进 Host receipt/state |
| enablement 是持久原语+派生决策 | `extensionEnablementService.ts:446-513`；报告 §10 | 借鉴用户意图与 effective state 分离；裁剪 14 态 |
| registry delta 与锁 | `extensionDescriptionRegistry.ts:93-119,262-307`；报告 13.11 | 借鉴 immutable snapshot、单写者、revision delta |
| delta、lazy start、activate 是独立机制 | `abstractExtensionService.ts`；报告 13.6 | 借鉴；Ora installed≠registered≠running |
| Ready→InitData→Initialized | `extensionHostProcess.ts:332-395`；报告 14.6 | 只借鉴严格、有确认的阶段；Ora 自定义 initialize/activate，activate success 就是 admission barrier |
| pending reply 在 dispose/exit 时 reject | `rpcProtocol.ts:164-175` | 借鉴；Ora 增加 Draining EOF barrier 和 UnknownOutcome |
| CancellationToken 跨 RPC | `rpcProtocol.ts:465-488,360-401` | 借鉴 cancel 信号，不序列化 Rust token 对象 |
| 响应性/崩溃阈值 | `rpcProtocol.ts:121,184-221`、`abstractExtensionService.ts:1565-1588` | 借鉴有界 crash window；不重放业务 RPC |
| `.obsolete` / `.vsctmp` 延迟删除 | `extensionManagementService.ts:719-858`；报告 7.7/15 | 裁剪为 tombstone + trash + reconcile |
| 多扩展共享 Extension Host | 报告 §13 | 不采用；Ora 一插件一 Bun 进程 |
| Local/Web/Remote、affinity、多 server/profile | 报告 §11-13 | MVP 不采用 |
| 两层扫描缓存 | 报告 7.8 | MVP 不采用；先用正确的直接扫描 |
| Gallery、VSIX、签名、publisher trust | 报告 §8/§17 | 后置；本地 digest 不冒充签名 |
| VS Code 二进制 RPC/PersistentProtocol header | `rpcProtocol.ts:516-561`、`ipc.net.ts:378-381,474-483` | 只借鉴显式宽度/BE/分层；Ora 5-byte frame 是独立格式 |

在线一手资料交叉核对：

- VS Code Extension Host：<https://code.visualstudio.com/api/advanced-topics/extension-host>
- VS Code Extension Manifest：<https://code.visualstudio.com/api/references/extension-manifest>
- JSON-RPC 2.0 Specification：<https://www.jsonrpc.org/specification>
- Language Server Protocol 3.17（`$/cancelRequest` / `-32800` 来源）：<https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/>
- Bun runtime `--no-install`：<https://bun.com/docs/runtime>
- Bun auto-install 行为：<https://bun.sh/docs/runtime/auto-install>
- Bun bundler 与 `--packages=bundle`：<https://bun.com/docs/bundler>
- Bun isolated installs/link 布局：<https://bun.com/docs/pm/isolated-installs>
- Microsoft `FindFirstStreamW`（stream 枚举与错误语义）：<https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-findfirststreamw>
- Microsoft `GetVolumeInformationByHandleW`（从 pinned volume 查询 `FILE_NAMED_STREAMS`）：<https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getvolumeinformationbyhandlew>
- Microsoft `CreateFileW`（reparse/share/handle 打开语义）：<https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew>
- Microsoft `UpdateProcThreadAttribute`（`PROC_THREAD_ATTRIBUTE_JOB_LIST/HANDLE_LIST`）：<https://learn.microsoft.com/en-us/windows/desktop/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute>
- Microsoft Job Objects：<https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects>
- Microsoft `CreateNamedPipeW`（overlapped/local pipe flags）：<https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-createnamedpipew>
- Microsoft `SetFileInformationByHandle`（handle-based disposition）：<https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfileinformationbyhandle>
- Microsoft `ReplaceFileW` / `MoveFileExW`（Windows state commit）：<https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew>、<https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw>

---

## 20. 需求追踪表

| 用户目标 | 设计落点 | 验收证据 |
|---|---|---|
| 扫描本地插件 | §8.1-8.3 | candidate/installed scan tests |
| 识别插件 | §5、§8 | structured identify snapshot |
| 安装 | §6、§9 | fault injection + atomic visibility |
| 验证 | §5.5、§8 | schema/path/engine/integrity tests |
| 执行 Agent 插件 | §11、§13 | Windows real Bun E2E |
| 卸载 | §10.3 | tombstone/trash/restart tests |
| 禁用 | §7.2、§10.2 | stale snapshot/admission tests |
| 注册管理 | §7.3 | revisioned delta tests |
| 生命周期 | §11 | state/race/process-tree tests |
| Workbench 类型 | §5.3-5.4 | valid catalog + no-spawn E2E |
| 本地管理 API 与授权 | §8、§14.3、§15 | bearer/Origin/SelectionHandle/CandidateHandle/grant E2E |
| Windows runtime 供应 | §6.1、§11.3 | packaged asset digest/deploy/restore E2E |
| `[length i32][type i8][payload]` | §12.2 | shared golden fixtures |
| 大端序 | §12.2-12.3 | Rust↔TS golden |
| Rust padding/字节对齐 | §12.3 | fixed 5-byte encoder tests |
| 粘包/分包/EOF | §12.4、§17.1 | arbitrary split/coalesce property tests |
| 必要安全与恢复 | §3、§9、§14 | fault injection/fuzz/Windows E2E |

---

## 21. 最终实现门禁

实现团队在开始编码前必须确认以下裁决没有被重新含混化：

- length 是 **payload bytes**，不是 header/body total。
- type 是 signed `i8`，length 是 signed `i32`，均显式 big-endian/byte 编码。
- header 恰好 5 bytes；Rust struct layout 从不进入 wire。
- manifest、安装记录、用户意图、effective state、registry 与 runtime 是不同概念。
- candidate scan 不授予执行资格；staging 复验才是安装提交前证据。
- final 目录只有 matching persisted install intent 才可恢复收养；receipt 本身不是用户授权。
- workbench 是合法但 MVP 不支持执行的 kind，而不是 invalid agent。
- 安装默认 disabled；缺失/损坏状态 fail closed。
- ManagerLease 由唯一 BackendRuntime 持有到 shutdown；production 只监听 authenticated loopback。
- launch grant 只控制 Host 注入，不宣称 OS sandbox；secret value 不进 state/log。
- 一插件一进程不等于 sandbox；Job Object 是生命周期保证，不是权限隔离。
- 崩溃不重放业务请求；`UnknownOutcome` 是必要语义。
- lifecycle 只有 initialize→activate 与 deactivate→exit 两段；不存在 `plugin.ready`、`$/ready` 或插件入口抢先启动协议。
- v1 Plugin→Host 业务 Request 集合为空；作者 SDK 不导出 raw RPC、Host API facade 或 private bootstrap。
- selection/candidate 两阶段授权必须绑定 session、用途、TTL、单次消费和 reviewed digest；客户端 path/id/version/digest 都不是安装授权。
- 当前代码中不存在 manifest/installer/Workbench/runtime actor/Job Object/new frame codec；本文描述的是待实现目标，不是已有能力。

满足第 17.5 节全部签收条件后，本设计对应的插件管理 MVP 才算完成。

---

## 22. `alignment_plugin_manager_sdk_0715.md` 冲突审核与裁决

### 22.1 输入有效性与审核方法

0715 对齐文档自述比较的是 “PluginManager `design.md` v7，1315 行” 与另一份 SDK v2.0；它同时把 `plugin.json`、换行 JSON-RPC、`plugin.ready`、Host API/Memento 等当作讨论前提。这些前提不是当前 Ora 代码事实，也与本次用户明确指定的 5-byte Frame 及 v3 边界不完全相同。因此该文件是**历史差异清单**，不是可以整体覆盖当前设计的上位规范；其行号和“完全契合”标签不能直接移植。

冲突裁决按以下证据优先级执行：本次明确产品/协议要求 → 当前 Ora commit 可复现源码事实 → JSON-RPC/LSP/Bun 官方规范 → 指定 VS Code commit 的机制事实 → 两侧设计文档中的待实现建议。低优先级材料可以提出风险，不能把尚未实现的接口写成现状，也不能推翻已冻结的上位约束。VS Code 只提供可借鉴机制；拓扑、威胁模型或协议不同的地方不做类比推断。

本轮以三个互相制约的角色审查，并由总体架构视角检查闭环：

1. **协议与互操作审查**：逐字检查 framing、握手、method、JSON-RPC error、取消、deadline、stream 和 Rust/TS golden fixture。
2. **SDK ABI 与包工程审查**：检查 public/private 边界、entry shape、类型真相源、materialized artifact、Bun 启动行为和现有 SDK 迁移。
3. **运行时、安全与恢复审查**：检查授权链、Windows 文件语义、Job Object、状态损坏、生命周期竞态、卸载和 fail-closed 条件。

三方一致的最小交集才写入 normative 主文；不能达成一致的扩展能力一律后置，不以空 interface 或兼容 shim 预占。最终一致结论是：private bootstrap、Host-first initialize/activate、无额外 ready、v1 无 Plugin→Host 业务 Request、无半成品 Memento、两阶段 source authorization、`wireVersion`/`pluginApi`/Agent `contractVersion` 三个独立版本轴以及 Job/tree-empty 门禁必须同时成立。

### 22.2 “完全契合”十项的重新核验

| 0715 项目 | v3 裁决 | 结论与边界 |
|---|---|---|
| 一插件一进程 | 采纳 | 每个 active plugin 一个 Bun/private-bootstrap generation；不是每请求一进程，也不是安全 sandbox。 |
| stdio + 换行 JSON-RPC | 拆分裁决 | 采纳 stdio；拒绝换行 framing，统一为 §12 的 5-byte Frame。HTTP 层 NDJSON 是独立 adapter contract。 |
| Rust `ts-rs` 生成 TS | 采纳并强化 | lifecycle、Agent DTO、error data 和 fixture 只保留一个 Rust/生成真相源；生成 drift 是 CI failure。 |
| console 保护 | 合并后采纳 | 由 Host-owned private bootstrap 在 import entry 前占有 stdout；console 重定向 stderr，仍以恶意插件可污染 fd 1 为威胁模型。 |
| 动态扫描 `plugin.json` | 采纳目标、拒绝载体 | 动态 catalog/registry 是目标；manifest 固定为 `package.json#ora`，候选扫描不直接注册，只有受管 installed+enabled Agent 可注册。 |
| 懒激活 | 采纳 | effective-enabled 只授予 admission，首个 start/invoke single-flight 启动；不预 spawn。 |
| 复用现有进程抽象 | 扩展后采纳 | `ProcessSpec`/DI 思路保留；当前只杀直接 child，不满足 Agent 场景，必须新增无逃逸 `ProcessTreeSpawner`/Job Object。 |
| manager 与管理面同进程 | 采纳 | 唯一 BackendRuntime/ManagerLease 是状态写者；插件仍为独立 child。 |
| `read_to_string` 改有界逐行 | 采纳目标、拒绝机制 | 必须长期、有界读取；实现是 incremental byte Frame decoder，不是 `read_line`。 |
| 换行解决粘包/分包 | 拒绝机制 | 粘包/分包由 length-prefixed codec、partial buffer 与 EOF 状态机解决；不得搜索换行或猜测重同步。 |

### 22.3 红、黄、绿差异的最终协议

| 差异 | 最终统一方案 | 被拒绝方案及原因 |
|---|---|---|
| 握手发起方与阶段 | Rust Host spawn **private bootstrap**；Host Request `$/initialize` → Response；Host Request `$/activate` → success Response；Host 复核 epoch/revision 后进入 Running。停止为 `$/deactivate` → `$/exit`。 | 拒绝 v7 的 initialize+ready：没有可确认 activation barrier。拒绝 SDK 的 `plugin.ready`+init+activate：多一个竞态状态，且若直接执行插件 entry，未可信代码会先于 transport guard 运行。VS Code child-first Ready 属于不同的共享 Extension Host 拓扑，不是 Ora 必须复制的方向。 |
| 初始化字段 | initialize 下发 id/version/kind/pluginApi/contentOwner、Host-derived `extensionPath`/`entryPath`/`storagePath`、manifest agents、session 和 limits；entry 只属于 private Host↔bootstrap DTO。 | 不接受客户端/插件回传的 entry/capability，不把 `entryPath` 暴露进作者 context；不下发 `ORA_DATA_DIR`、credential、globalState 或单一 workspaceState。 |
| Method 命名 | lifecycle/transport 控制统一 `$/...`；Host→Plugin Agent 业务统一 `agent.*`；v1 Plugin→Host 业务 Request 集合为空。 | 不采用 `$host.*` 或 `ora.*` 空 API。当前没有业务需求、权限模型和 handler executor，预留可调用 method 只会形成未实现的安全承诺。未来必须随新 `pluginApi` 冻结 typed namespace。 |
| JSON-RPC 错误码 | 采用 §16.1 的标准码和方向/致命性矩阵；Agent 预期业务失败集中为 `-32000 + closed data.kind`，busy 为 `-32010`；`-32800` 明示为 LSP-derived Ora extension。 | 不把管理面 `not-installed/disabled/unauthorized` 与插件 wire server code 混用，也不复用两份各自增长、会碰撞的 `-32001/-32003` 表。管理错误留在 application contract。 |
| `globalState/workspaceState` | v1 不提供；给每个 content owner 独立 `storagePath`，插件自管文件。项目/工作树使用每次 Agent typed request 的 closed `AgentScope`（opaque identity + Host-resolved cwd），不进入进程全局状态。 | 拒绝只读 initialize snapshot：它没有 update/CAS/revision/quota/损坏恢复/多项目 identity，表面上有 Memento、实际无法正确持久化。完整 state broker 需新 `pluginApi`。 |
| 插件间 broker | 后置，v1 无 dispatcher 扩展点。 | “先留 raw invokeHost/onRequest”会扩大攻击面并冻结错误 ABI；需求、授权和环路/背压语义未定义前不实现。 |
| SDK API 模型 | default export plain structural `AgentPluginDefinition`；`defineAgentPlugin` 是纯 identity helper；所有 provider DTO generated。 | 不采用 `register(manifest)`、raw `onRequest/invokeHost`，也不让 `definePlugin` 隐式启动 transport。class/`instanceof`/global singleton 会被重复 bundle 破坏。 |
| SDK 内外分层 | public 仅 `./agent`、作者可见 `./types`；private bootstrap/Frame/RPC/lifecycle/dispatcher 是 Ora runtime asset。 | 0715 将其视为“manager 无需关心”不够准确：private bootstrap 的版本、加载时序、stdout 所有权和打包边界直接决定 wire 正确性，必须由 Host 设计共同冻结。 |
| 版本字段 | Rust Host↔private bootstrap 使用 runtime asset/receipt 锁定的 `wireVersion`；bootstrap↔plugin module 使用 manifest exact `engines.pluginApi=1`；每个 Agent contribution 再有 `contractVersion`。 | 拒绝一个 `pluginProtocol` 同时代表 framing、SDK ABI 和业务 contract；三个变化速度和所有者不同，混在一起无法做兼容判定。 |

### 22.4 当前 SDK 的无兼容迁移

当前 `packages/plugin-sdk` 只是 `getNums/returnNums` + NDJSON 的最小实验，并非应保留的 v1 API。按仓库“No Backward Compatibility”规则，迁移必须一次完成：

1. 删除 line reader/writer、`getNums/returnNums`、旧 `./host`/raw RPC export；不做 NDJSON auto-detect，也不接受两套握手。
2. `@ora-space/plugin-sdk` 只构建作者 ABI；private bootstrap 作为 Host versioned runtime asset 单独构建、打包和校验，不能被插件依赖或 bundle。
3. manifest/lifecycle/Agent/error 类型从 Rust contract 生成；当前未跟踪、未导出的 `plugin-manifest.ts` 不能升级为平行真相源，应由生成物替换或删除。
4. 修正 Taskfile 中 `@ora/plugin-sdk` 与真实 package name `@ora-space/plugin-sdk` 的错配；public SDK 与 private-bootstrap package 分别递归发现全部 test/spec 文件，并让命令打印实际匹配 package、suite 与非零测试数。
5. SDK pack 固定单 ESM materialized artifact；metafile/parser 证明无 unresolved external，安装器复验无 `node_modules`、links/native addon，private bootstrap 不在 bundle 中。
6. 至少一个 pack 后 fixture 只通过 public subpath import，在 Windows 上完成 Rust spawn→initialize→activate→Agent request/stream→deactivate/exit 的真实 pipe E2E；旧 NDJSON、旧 method、旧 ready 和 private import 都有 negative test。

### 22.5 单一规范与变更控制

实现阶段的单一规范链为：本文 v3 → Rust contract/schema → 生成 TypeScript → 一份机器可读 Frame/lifecycle fixture → Rust/TS/E2E。0715 对齐文档和旧 `design.md` 只保留为历史输入；发生冲突时不得由 SDK 或 Manager 任一侧私自选择旧版本。

任何 wire/ABI 变更必须在同一变更中：

- 分清 `wireVersion`、`pluginApi` 或 Agent `contractVersion` 哪个轴发生变化；
- 更新 closed DTO/error/method registry 和生成物；
- 更新 golden fixture、正反互操作测试、超限/竞态测试；
- 更新本文及一份说明兼容性与迁移方式的 ADR；
- 证明旧 runtime/新 plugin、或新 runtime/旧 plugin 会在执行插件代码前稳定 fail closed，而不是“尽量解析”。

这份裁决消除了 0715 文件中会导致实现分叉的三套核心歧义：行协议与 Frame 协议并存、两种握手同时存在、SDK 自由扩展 Plugin→Host 方法。后续若要改变这些边界，应当作为明确的新版本设计，而不是称为“补字段”或“内部 SDK 决策”。
