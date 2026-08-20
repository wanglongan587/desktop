# Ora MCP 插件安装、编译与会话物化 Spec

## 问题陈述

Ora 支持多种 Agent，而不同 Agent 对 MCP 的配置格式、配置文件位置、变量引用方式和热重载能力并不相同。当前系统已经具备 Agent 插件发现、运行和基础插件生命周期，但还没有一条完整、安全、可审计的 MCP 插件链路。用户无法从插件市场安装一个 MCP 包，并在创建或恢复对话时，由所选 Agent 插件把该 MCP 可靠地配置到项目根目录或 worktree 对应的 Agent 配置中。

如果把 MCP 原始清单直接交给每个 Agent 插件解析，会让 MCPB、Registry、归档路径、运行时选择和 secret 规则在多个插件中重复实现，产生不一致的兼容性与安全行为。如果安装时立即写入所有 Agent 配置，又无法知道会话最终选择的 Agent、workspace 和能力。因此，需要把“安装期可信编译”和“会话期 Agent 物化”分开，同时用稳定的类型化契约连接两者。

## 解决方案

Ora 定义统一的 `.orax` MCP 包。包必须包含 Ora 自有的 `orax.toml`，并选择且仅选择一个兼容 profile：本地 stdio 使用严格 MCPB 0.3 `manifest.json`，远程 Streamable HTTP 使用严格 MCP Registry 2025-12-11 `server.json` 子集。

安装时，Ora 下载并校验市场摘要，在受控 staging 中安全解压，严格校验 Ora 清单和选定 descriptor，交叉核对身份、版本、profile 与路径，编译出不可变的统一 `InstalledMcpDescriptor`，并把安装记录、readiness 和输入绑定元数据保存到现有 Ora SQLite 数据库。安装成功只证明静态合法，不代表 MCP 服务可连接或可完成协议初始化。

会话创建、加载或恢复时，Ora 根据项目根目录或 worktree 的 desired MCP selection 查询 repository，把精确版本的已编译 descriptor 解析成当前机器可执行的 `ResolvedMcpForAgent` 完整集合，再调用所选 Agent 插件的幂等 reconcile 接口。Agent 插件只负责目标 Agent 的配置语法、用户配置保护、安全变量引用和原子文件更新，并返回 materialization receipt。Ora 随后持久化 receipt 和本次 Session 实际加载的精确版本。

## 用户故事

1. 作为 Ora 用户，我希望从插件市场安装 MCP 插件，从而无需手工下载和配置服务。
2. 作为 Ora 用户，我希望安装失败时看到明确的失败阶段和原因，从而知道应修复包、运行时还是输入配置。
3. 作为 Ora 用户，我希望安装一个结构合法但缺少输入的 MCP，从而可以稍后补充配置而不必重新下载。
4. 作为 Ora 用户，我希望缺少 Node 或 Python 运行时时仍能保留已安装包，从而可以安装运行时后再启用。
5. 作为 Ora 用户，我希望安装不会执行包内脚本，从而避免安装阶段运行不受信任代码。
6. 作为 Ora 用户，我希望 Ora 在启用前展示 MCP 声明的网络和文件系统权限意图，从而能做出知情选择。
7. 作为 Ora 用户，我希望项目根目录下的对话共享一份 MCP 选择，从而避免同一配置文件被多个会话相互覆盖。
8. 作为 Ora 用户，我希望同一 worktree 下的对话共享一份 MCP 选择，从而让该工作树内行为一致。
9. 作为 Ora 用户，我希望不同 worktree 可以选择不同 MCP 集合，从而隔离并行开发任务。
10. 作为 Ora 用户，我希望创建会话时由当前选择的 Agent 插件配置 MCP，从而适配 Claude Code 等不同 Agent 的专有配置格式。
11. 作为 Ora 用户，我希望 Agent 配置中的手工条目被保留，从而不会因 Ora 物化而丢失自定义设置。
12. 作为 Ora 用户，我希望 Ora 只更新自己管理的配置条目，从而清楚区分系统配置与用户配置。
13. 作为 Ora 用户，我希望同一 desired MCP 集合的重复会话创建不会反复改写配置文件，从而减少无意义变更。
14. 作为 Ora 用户，我希望某个 MCP 不被当前 Agent 支持时仍可用其余成功的 MCP 启动会话，从而避免单项能力阻塞全部工作。
15. 作为 Ora 用户，我希望降级启动时明确看到失败的 MCP 和原因，从而不会误以为全部 MCP 已生效。
16. 作为 Ora 用户，我希望配置文件整体不可安全读取或写入时阻止会话启动，从而避免使用不可验证的半配置状态。
17. 作为 Ora 用户，我希望安装新的 MCP 后，由我明确把它加入 workspace 选择并重启或重建 Agent Session，从而控制已有对话何时采用新能力。
18. 作为 Ora 用户，我希望已运行会话继续使用启动时的精确 MCP 版本，从而避免运行中被静默切换。
19. 作为 Ora 用户，我希望激活 MCP 新版本后看到已有会话需要重启，从而理解新版本何时生效。
20. 作为 Ora 用户，我希望新版本物化失败时不会静默回退旧版本，从而避免实际运行状态与界面不一致。
21. 作为 Ora 用户，我希望可以显式回滚活动版本，从而从失败升级中恢复。
22. 作为 Ora 用户，我希望修改 MCP 输入或 secret 后相关会话被标记为需要重启，从而不继续误用旧环境或认证状态。
23. 作为 Ora 用户，我希望 secret 不进入普通数据库、项目文件、日志或 receipt，从而降低凭据泄露风险。
24. 作为 Ora 用户，我希望 Ora 使用操作系统凭据存储保存真实 secret，从而复用平台安全能力。
25. 作为 Ora 用户，我希望全局输入可以被项目根目录或 worktree 输入覆盖，从而在共享默认值上保留局部差异。
26. 作为 Ora 用户，我希望远程 MCP 默认只接受 HTTPS，从而避免认证信息和流量被明文传输。
27. 作为本地开发者，我希望在显式本地信任策略下使用 loopback HTTP MCP，从而方便调试本机服务。
28. 作为 Ora 用户，我希望远程 MCP 的 endpoint 在变量解析后仍接受安全策略检查，从而不能通过模板绕过 HTTPS 限制。
29. 作为 Ora 用户，我希望配置被手工或 Git 修改后 Ora 能识别 drift，从而在启动前重新协调真实状态。
30. 作为 Ora 用户，我希望崩溃发生在配置文件写入与 receipt 提交之间时能够自动恢复，从而不必手工清理不一致状态。
31. 作为 Ora 用户，我希望并发创建相同 workspace 和 Agent 的会话时配置写入被串行化，从而避免丢失更新。
32. 作为 Ora 用户，我希望不同 workspace 或不同 Agent 的物化可以并发，从而不产生不必要的全局阻塞。
33. 作为 MCP 包作者，我希望 stdio `.orax` 与 MCPB 0.3 的 archive、manifest 和启动语义兼容，从而复用既有 MCPB 工具和包结构。
34. 作为 MCP 包作者，我希望 HTTP `.orax` 采用固定版本的官方 Registry schema，从而基于公开、可验证的描述格式发布远程服务。
35. 作为 MCP 包作者，我希望 Ora 对兼容 profile 和 schema 版本给出封闭、明确的声明，从而不会因上游 `latest` 变化导致旧包行为漂移。
36. 作为 MCP 包作者，我希望 `orax.toml` 只声明 Ora 身份、宿主要求、profile 和权限意图，从而不重复 descriptor 已有的版本和运行配置。
37. 作为 MCP 包作者，我希望 descriptor 中的上游名称被保留，从而不必把 MCPB 或 Registry 身份伪装成 Ora ID。
38. 作为 MCP 包作者，我希望有统一的 validate、pack 和 inspect 工具，从而在发布前发现与生产安装器相同的问题。
39. 作为 MCP 包作者，我希望相同输入产生可重复摘要的包，从而可靠发布不可变版本。
40. 作为插件市场维护者，我希望市场 Ora ID、kind、version 和 SHA-256 与包内信息严格交叉校验，从而阻止被替换或错配的发布资产。
41. 作为插件市场维护者，我希望相同 Ora ID 和版本只能对应同一摘要，从而保证版本不可变。
42. 作为插件市场维护者，我希望 GitHub 仓库重命名或转移不会自动改变 Ora ID，从而保持安装身份稳定。
43. 作为 Agent 插件作者，我希望收到已经校验并解析运行时的类型化 MCP DTO，从而不用重新实现 MCPB、Registry 和归档安全逻辑。
44. 作为 Agent 插件作者，我希望收到完整 desired set 与 revision，而不是增量启用事件，从而可以实现幂等 reconcile。
45. 作为 Agent 插件作者，我希望 Ora 明确传入 authoritative workspace root，从而把配置写入正确的项目根目录或 worktree。
46. 作为 Agent 插件作者，我希望 Ora 传入 stdio 的绝对可执行路径和已验证参数，从而避免 Host 与 Agent 的 PATH 解析不一致。
47. 作为 Agent 插件作者，我希望可以针对不支持的 transport 或安全引用返回 item-local failure，从而让其他 MCP 继续工作。
48. 作为 Agent 插件作者，我希望由插件决定目标配置格式、转义和安全变量引用，从而保留各 Agent 的专有语义。
49. 作为 Agent 插件作者，我希望 receipt 只包含稳定 managed identity、版本、错误和 fingerprint，从而不必处理或回传明文 secret。
50. 作为 Ora 维护者，我希望 MCP 状态使用现有 SQLite 的版本化 migration，从而不引入第二个数据库和额外部署复杂度。
51. 作为 Ora 维护者，我希望 MCP 专用状态不复用全局插件启用表或普通用户配置表，从而保持作用域和安全语义清晰。
52. 作为 Ora 维护者，我希望非法 transport 字段组合无法构造成领域对象，从而减少下游重复校验。
53. 作为 Ora 维护者，我希望归档路径和解压逻辑由共享通用模块实现，从而让 CLI 与安装器使用相同安全规则。
54. 作为 Ora 维护者，我希望 receipt 子项可以按 MCP 查询，从而支持诊断和未来卸载清理，而不是依赖不透明 JSON。
55. 作为 Ora 维护者，我希望 Session 记录实际加载的 revision 与精确版本，从而支持重启提示、旧版本租约和未来卸载。
56. 作为 Ora 维护者，我希望安装目录和数据库不一致时启动协调可以确定性处理，从而不猜测或静默修改不可变包内容。
57. 作为安全审计人员，我希望所有校验错误都有稳定类别、阶段和字段或路径，从而便于审计与自动化分析。
58. 作为安全审计人员，我希望归档拒绝 traversal、绝对路径、链接、特殊文件、重复规范化路径和解压炸弹，从而避免越界写入。
59. 作为安全审计人员，我希望日志默认脱敏 secret 并减少绝对 workspace 路径，从而降低诊断数据泄露风险。
60. 作为后续卸载功能开发者，我希望当前模型记录 managed entry、使用中的精确版本和清理状态，从而未来可以先移除配置、等待租约，再删除二进制。

## 实现决策

### 1. 领域边界与职责

- MCP 包的可信解释权属于 Ora Host。Ora 负责严格 schema 校验、身份与版本交叉校验、归档和路径安全、MCPB platform override、输入解析、运行时发现和 workspace 解析。
- Agent 插件负责目标 Agent 的配置语法、字段映射、转义、安全变量引用、用户配置保护、Ora-managed entry 识别和原子文件更新。
- Agent 插件不得直接读取 Ora SQLite、重新解析 archive，或自行选择另一个运行时。Ora 通过稳定的类型化 DTO 推送当前 Session 获准使用的数据。
- 市场发布元数据、包内 Ora manifest、上游 descriptor、数据库活动状态和 materialization receipt 是不同权威来源；冲突时拒绝，不进行静默优先级修复。

### 2. `.orax` profile

- `.orax` 是 ZIP 容器，根目录必须包含 `orax.toml`。
- v1 只支持两个封闭 profile：`mcpb-stdio` 和 `registry-remote`，包必须选择且仅选择一个。
- `mcpb-stdio` 必须携带根 `manifest.json`，严格兼容 MCPB 0.3，支持 `node`、`python`、`binary`，并保留该版本的启动、变量、输入和 platform override 语义。
- `registry-remote` 必须携带根 `server.json`，严格验证 MCP Registry 2025-12-11 schema，并进一步限制为恰好一个 `streamable-http` remote；拒绝 SSE、多 remote 和 package/remote fallback。
- 固定 schema 随 Ora 发布一同 vendoring；安装时不得联网获取 `latest` schema。
- `orax.toml` 是薄 Ora manifest：拥有 canonical ID、`mcp` kind、Ora 版本要求、profile、descriptor schema 和权限意图；不得重复 descriptor version、stdio 启动字段、HTTP endpoint/header 或输入声明。

### 3. 身份、版本与摘要

- canonical Ora ID 采用 `namespace/name`，v1 只接受 `official` namespace；ID 与 GitHub 仓库名解耦。
- 市场 Ora ID 必须与 `orax.toml` 完全一致，市场 kind 与包内 kind 都必须是 `mcp`。
- 版本只由选定 descriptor 在包内声明，必须是精确 SemVer；市场 release version 必须与其完全一致。
- 市场 SHA-256 在解压前验证。`(Ora ID, exact version)` 是不可变身份；相同摘要重复安装幂等成功，不同摘要构成 immutable-version conflict。

### 4. 安装事务与编译

- 安装顺序固定为：同文件系统 staging 下载、摘要验证、安全解压、严格解析 Ora manifest、严格验证 descriptor、交叉校验、编译 descriptor、计算 readiness、原子提升到不可变版本目录、单个 SQLite 事务提交记录。
- 安装器不得执行 install、post-install、activation 或 migration 脚本，也不得在静态校验前解析或执行包代码。
- 归档验证必须拒绝绝对路径、盘符和 UNC、`..` 越界、符号或硬链接、junction/reparse point、特殊文件、规范化重复路径，以及超过有限上限的条目、文件、路径、深度或展开体积。
- 文件系统 rename 与 SQLite 不能共享事务，因此启动时必须协调 abandoned staging、无记录的最终目录、缺失目录的数据库记录和摘要不一致。不可恢复的版本不得物化。
- 成功安装生成排他的 `BundledStdio` 或 `RemoteStreamableHttp` 编译结果；领域类型不得允许本地 command 与远程 URL 混合。
- 静态安装不执行 MCP initialize 或健康检查。远端可达性、凭据有效性、进程启动和工具可用性在首次使用或显式健康检查中确定。

### 5. Readiness 与输入安全

- 已安装版本 readiness 是 `Ready`、`NeedsInput` 或 `NeedsRuntime` 的封闭状态。后两者允许保留安装，但不得激活或物化。
- descriptor 是输入声明权威：stdio 使用 MCPB `user_config`，HTTP 使用 Registry variables 和 header inputs。Ora 编译为统一类型模型。
- 非 secret descriptor default、全局 binding、workspace binding 按从低到高优先级解析。workspace 使用与 selection 相同的项目根目录或 worktree scope；v1 不提供 Session-local binding。
- secret 不得有包默认值，不得进入 command 或 args，只能通过环境变量或 HTTP header 的安全引用表达。
- 真实 secret 存入操作系统 credential store；SQLite 只保存 opaque reference。archive、普通数据库字段、项目文件、receipt 和日志不得包含明文 secret。
- 必填输入缺失时，在调用 Agent 插件前返回结构化 `McpConfigurationRequired` 并阻止 Session 创建；这不是可降级的 item-local Agent 错误。
- 远程 endpoint 默认要求 HTTPS。明文 HTTP 仅在宿主显式开发/本地信任策略下对 loopback 地址开放，包自身无权打开例外。
- OAuth 仅在目标 Agent 插件声明 Agent-managed OAuth 能力时支持；v1 Ora 不实现 OAuth client 或 token refresh。

### 6. 持久化模型

- 使用现有应用 SQLite 和版本化 migration，不创建新数据库，不复用只表达全局启用状态的插件表，也不复用普通非敏感用户配置表。
- 持久化分别表达不可变安装版本、活动版本、输入 binding/secret reference、workspace desired selection 与 revision、每个 Agent 的当前 materialization receipt、逐 MCP applied/failed item，以及 Session 实际加载记录。
- 编译 descriptor 按 `(MCP ID, exact version)` 不可变；升级插入新版本，不覆盖运行中 Session 引用的记录。
- materialization receipt 的 identity 是 `(workspace scope, agent plugin ID)`，不是 Session。v1 只保留当前 receipt，但逐 MCP item 必须可查询，不能只存在不透明 JSON 中。
- receipt 至少保存 desired/materialization revision、Agent 插件身份和版本、精确 MCP 版本、逐项结果、稳定 managed identity、fingerprint、状态和本地时间；不得保存明文 secret 或包含 secret 的渲染配置。
- Session 单独快照它实际加载的 desired revision、materialization revision 和 applied exact versions。

### 7. Workspace 选择与版本策略

- desired MCP selection 作用域是配置目的地：项目根目录或 worktree。同一 scope 下所有会话共享选择；不同 worktree 相互隔离。
- 选择默认使用 `FollowActive`，领域模型为未来显式 `Pinned(exact version)` 保留排他 variant。每次物化必须把策略解析为精确版本。
- 安装新版本只产生 `Available`；v1 需要用户显式激活，不因静态校验成功自动改变 workspace。
- 激活新版本会推进所有受影响 `FollowActive` workspace revision，标记 receipt 为 `Outdated`，并把运行中 Session 标记为 `PendingRestart`；运行中 Session 保持旧精确版本。
- 输入或 secret binding 变化使用相同 revision 机制，不对已运行进程做隐式热替换。

### 8. Session 期解析与 Agent 物化

- Session create/load/resume 前，Ora 查询 repository，并按当前机器和 workspace 把 portable descriptor 解析为 `ResolvedMcpForAgent`。
- stdio 解析包括当前平台 override、已验证包内路径、Node/Python/binary runtime 搜索和绝对可执行路径输出。查找顺序是 Ora bundled runtime、用户显式 runtime、受控 system PATH。
- HTTP profile 不进行 executable resolution，但 endpoint 模板解析后必须再次通过网络安全策略。
- Ora 向 Agent 插件传完整 desired set、revision、Session ID 和 authoritative workspace，而不是发送增量 enable/disable 事件。
- Agent 插件把调用视为幂等 reconcile：读取并验证目标配置，区分用户与 Ora-managed entries，为各 MCP 独立规划，合并成功项，在目标目录写入并验证临时文件，再原子替换并返回 receipt/fingerprint。
- managed identity 必须由 Agent 插件身份和 Ora MCP canonical ID 等稳定输入确定。若用户条目与保留身份冲突且无法安全区分，必须失败而不是覆盖。
- normalized 配置、fingerprint、desired revision 与当前 receipt 一致时，插件应返回 `AlreadyMaterialized`，不改写文件。

### 9. 失败、并发与恢复

- Agent 不支持 transport、安全 secret reference 无法表达或单项无法渲染属于 item-local failure；会话可用成功子集启动，但整体状态必须是 `Degraded` 并持续暴露失败项。
- 目标配置不可安全读取/解析、managed ownership 不可判定、无法原子更新、revision 变化或 receipt/fingerprint 不可信属于 global failure，状态为 `Blocked` 并阻止 Session 启动。
- 新 revision 的某项失败时，必须移除该项旧 Ora-managed entry，不得把旧版本冒充成新 desired version。
- materialization 状态是 `Unknown`、`Ready`、`Degraded`、`Outdated`、`Drifted` 或 `Blocked` 的封闭状态机。
- 相同 `(workspace scope, agent plugin ID)` 的 inspect、plan、原子替换和 receipt 提交由应用级临界区串行化；其他 key 可以并发。
- 顺序必须是 Agent 插件先更新文件并返回 receipt，Ora 再持久化。文件成功但数据库提交前崩溃时，下一次 inspect/reconcile 依靠确定性 managed identity 和 fingerprint 恢复。

### 10. 生命周期与未来卸载兼容性

- 旧版本在仍为 active、被 pinned workspace 引用、存在于 applied receipt、被运行中 Session usage lease 持有或处于 rollback retention window 时不得删除。
- 新版本物化失败不得自动回退。回滚是显式活动版本变更，并推进新的 desired revision。
- v1 不实现完整卸载，但记录必须支持未来 `PendingRemoval → Draining → Removing → Absent` 流程。
- PendingRemoval 应立即阻止新选择和物化；未来卸载必须先从 desired selection 删除并 reconcile 掉 Agent 配置，等待 usage lease，再删除不可变包。不可访问 workspace 使用持久 cleanup tombstone 延后清理。

### 11. 模块与接口

- 扩展发布 manifest 解析能力，使市场元数据可表达和校验 MCP release，但保持其不负责网络、文件系统和运行时。
- 新增 MCP package/descriptor 领域模型与严格编译器，以 enum 表达 profile、readiness、版本角色和 materialization 状态。
- 扩展通用 path/archive 模块，作为 CLI packer 与生产安装器唯一的归档安全实现。
- 新增 MCP 安装应用服务、repository ports 和 SQLite adapters，安装事务与数据库表保持在应用边界内。
- 新增 workspace selection、input binding、receipt 和 Session load 的 repository 接口。
- 在 Agent 插件 contract 中新增 capabilities、inspect/reconcile request、resolved MCP DTO、逐项结果、global failure 和 receipt contract。
- 会话编排在启动 Agent 前调用 MCP reconcile，并把成功加载快照写入 Session 记录。
- 所有错误返回稳定 category、phase 和 field/path；日志通过 Ora logging wrapper 输出并执行 secret 脱敏。

### 12. 开发者工具

- 提供 `validate`、`pack`、`inspect` 三个命令，并与生产安装器共享严格 schema、身份、输入、路径和 archive 校验实现。
- `pack` 在生成最终包前拒绝不安全或不支持的文件，并提供足够确定性的归档顺序与元数据以生成可重复摘要。
- `inspect` 展示 Ora/upstream identity、精确版本、profile/schema、runtime requirements、无值输入声明、权限意图、平台和结构化诊断。

## 测试决策

### 主测试 seam

- 采用一个最高层应用编排 seam 覆盖核心闭环：市场 release 与 `.orax` 输入经过安装，落入真实临时文件系统和临时 SQLite；设置 workspace desired selection 后创建或恢复 Session；伪造 Agent adapter 接收完整 resolved DTO 并返回 receipt；最终断言完整安装对象、完整 reconcile request、完整 receipt、Session load 和目标文件结果。
- 外部网络下载、OS credential store 和真实 Agent 进程使用 ports/fakes。文件系统与 SQLite 保持真实，以覆盖原子提升、migration、revision、receipt ordering 和崩溃协调等外部可观察行为。
- 测试只断言公共 API、持久状态、文件结果和结构化错误，不断言私有函数调用顺序或内部 SQL 细节。优先对完整领域对象、request 和 receipt 做 deep equality。

### 格式与安全 conformance

- 严格验证器使用固定的官方 MCPB 0.3 node、python、binary fixtures，以及 Registry 2025-12-11 Streamable HTTP fixtures。
- 反例覆盖未知 Ora 字段/schema、profile 与 descriptor 不匹配、市场身份/版本/摘要冲突、相同版本不同摘要、多 remote、SSE、fallback、HTTPS/loopback 策略和 secret 禁止位置。
- archive conformance 覆盖 traversal、绝对路径、Windows drive/UNC、链接/reparse point、规范化重复路径、单文件/总展开体积/条目数/深度限制，以及 Windows executable 解析。
- CLI validator 与生产安装器对同一 fixture 必须返回同一核心 validation result；只允许市场元数据或机器 runtime 相关检查存在上下文差异。

### 持久化与状态机

- repository contract 测试覆盖 immutable version、active version、workspace scope/revision、binding precedence、逐 MCP receipt item 和 Session exact-version load。
- 测试 secret 绝不出现在 SQLite、日志、receipt、共享项目配置或错误消息中。
- 测试 `Ready`、`NeedsInput`、`NeedsRuntime`，以及 `Unknown`、`Ready`、`Degraded`、`Outdated`、`Drifted`、`Blocked` 的合法转换和非法状态拒绝。
- 测试 filesystem promotion 成功但数据库提交失败、数据库记录存在但目录缺失、孤立最终目录和摘要漂移的启动协调。

### Agent materialization contract

- contract 测试覆盖完整集合 reconcile、`AlreadyMaterialized` no-op、用户条目保留、稳定 managed identity、冲突阻断、单项降级、global failure 和新 revision 失败时删除旧 managed entry。
- 并发测试证明相同 workspace/Agent key 串行化且不会产生“文件来自一个请求、receipt 来自另一个请求”的组合；不同 key 不被全局锁阻塞。
- 测试文件写入成功而 receipt 提交前崩溃后，下一次 inspect/reconcile 能通过配置内容和 fingerprint 恢复。
- 能力矩阵覆盖 stdio、Streamable HTTP、header secret reference 和 Agent-managed OAuth 的 supported/unsupported 行为。

### 既有测试先例

- 纯 manifest parser 已使用稳定首错、严格字段校验和完整领域对象比较，可作为 Ora manifest 与 descriptor 编译器测试先例。
- 已安装插件发现模块已有临时目录、路径 containment、无效包隔离和确定性 snapshot 测试，可作为 archive/path 测试先例。
- 插件 lifecycle 已使用真实临时 SQLite、临时文件系统、fake runtime、显式 scan/reconcile 和完整 response 比较，可作为 MCP 高层编排 seam 的直接先例。
- 数据库 repository 已有 migration 与 repository contract 测试模式，可扩展到 MCP 专用表，不复用现有插件状态语义。

## 范围外

- 插件市场 UI、GitHub catalog 同步、下载进度和发布审核流程的完整实现。
- 完整 MCP 插件卸载 UI 和物理删除流程；本期只保证数据模型和 receipt 能支持未来安全卸载。
- 所有 Agent 插件的具体配置适配器实现；本期定义并验证 Host/Agent contract，可用一个参考或 fake adapter 完成闭环。
- Agent 配置格式本身、Agent 是否支持热重载，以及每种 Agent 的重启交互。
- Ora 自有 MCP runtime、MCP server 进程托管或统一 MCP initialize/health-check 服务。
- Ora 自有 OAuth client、浏览器授权和 token refresh 生命周期。
- remote SSE、多个 remote、package/remote fallback、本地打包 HTTP daemon 和 MCPB 0.3 之外的版本。
- Session-local MCP selection 或 Session-local input override。
- 团队共享的项目内 `.ora` 配置文件；v1 desired selection 和 receipt 存在 Ora SQLite。
- 自动激活新安装版本、静默回滚、静默使用旧配置或强制终止已运行 Session。
- 跨设备同步、企业策略分发、签名信任链和沙箱权限强制执行；v1 只声明并展示权限意图。

## 补充说明

- 线格式、字段级约束、兼容声明和 conformance 清单以配套的《Ora MCP `.orax` Package Specification》为规范来源；本 Spec 描述产品闭环与实现验收边界。
- v1 固定兼容 MCPB 0.3 和 MCP Registry 2025-12-11 子集。未来支持新版本时新增显式 profile/schema variant，不改变旧 profile 的解释结果。
- 当前仓库存在“发布/市场 manifest”与“安装后插件 package manifest”两类清单，它们职责不同。MCP `.orax` 应接通市场发布元数据与新的 MCP package 编译链路，不把普通 Agent 插件的 `package.json` 变成 MCP descriptor。
- MCP 插件是否对 workspace 生效由 workspace desired selection 决定，不简单等同于 Agent 插件的全局 enabled 状态。只有启用并被选择的 Agent 插件在 Session 生命周期中执行物化。
- 安装、配置和运行必须保持三个不同状态：installed/readiness、workspace desired/materialization、Session loaded。界面和错误不得把三者折叠成一个模糊的“已启用”布尔值。
