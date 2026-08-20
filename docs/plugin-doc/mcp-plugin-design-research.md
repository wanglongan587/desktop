# Ora MCP 插件定义与持久化研究

> 调查日期：2026-08-20
> 范围：只研究 MCP 插件定义、安装描述、项目启用意图及密钥输入边界；不设计 Agent 插件内部如何改写 Claude Code、Codex 等配置。
> 方法：仓库代码和提交历史优先；外部资料只采用 MCP、VS Code、Claude Code 的官方规范或文档。文中明确区分“代码事实”“推断”和“建议”。
> 外部快照：MCPB `main@70fe3b34cd6dff1b3bba046638edc72a6467a4fb`；MCP specification `main@4df2d6b6e3588efb46e7542d98498e5c630a0a86`。链接保留到官方主线便于阅读，兼容测试应 pin 到明确版本/commit。

## 结论摘要

1. **不需要新建另一套数据库。** Ora 已有一份应用级 SQLite 数据库和版本化迁移。若产品以后需要“每个 Ora 项目选择一组 MCP”，可在现有数据库增加 `project_mcp_enablements` 关联表。当前最小闭环可先把选择保存为对话的 desired snapshot，不要求 MCP manifest/安装任务同步实现项目默认表。
2. **两套 manifest 的代码职责确实不同。** `ora-plugin-manifest` 处理发布/市场侧 TOML（下载 URL、SHA-256、宿主版本等）；`ora-plugin-manager` 处理安装后包根目录中的 `package.json`（运行入口、引擎和 Agent contribution）。但发布侧 crate 当前没有生产调用者，二者没有安装管线和交叉校验；它们是“合理的两层意图 + 尚未接通的实现”，不是已经完整落地的两层协议。
3. **Q22 应采用稳定的 Ora ID + SemVer。** 以现有市场设计为基础，规范化为 `PluginId = namespace/name`，例如 `official/user.ora-weather`。ID 不应由 GitHub 仓库名临时推导；市场 manifest 与包内 manifest 必须声明同一个 canonical ID 和精确相同的 SemVer。`id + version` 发布后不得替换资产或摘要。
4. **Q23 选择 B：沿用 MCPB `user_config` 声明输入，但不携带真实 secret。** Ora 严格解析 MCPB 的 string/number/boolean/file/directory、required/default/multiple/sensitive 以及到 args/env 的引用语义，再转换为自己的强类型 DTO。secret 只能保存为安全引用，不能进入插件包、项目启用集合或可提交的 Agent 配置。Ora 当前没有凭据库实现；由 Agent 插件使用目标 Agent 的安全输入机制或系统环境变量，无法安全表达时返回 `SecretInputUnsupported`。
5. **新设计改为“`.orax` 是 MCPB 内容格式的 Ora profile”。** MCP kind 的 archive 继续是 ZIP，保留根 `manifest.json`、MCPB 目录布局、runtime、`mcp_config`、`user_config` 和 platform override 语义，只额外增加根 `orax.toml`。但兼容必须分级：stdio 包可以做到 MCPB schema/启动语义兼容；HTTP 尚不是 MCPB 能力，只能是 Ora 扩展，不能宣传成 MCPB HTTP 兼容。
6. **已创建对话要用新 MCP，必须更新对话 desired selection 并使 Agent session 重新加载。** 第一版不承诺热插拔；磁盘配置已写入、运行 session 已加载和全局包已安装是三个不同状态。卸载应先 PendingRemoval、清理 Agent 配置、等待运行引用释放，再删除 package。

## 1. 每项目启用 MCP 集合：是否需要新数据库

### 1.1 代码事实

- Ora 的 `Project` 当前只有 `id`、`name`、`root_path` 和审计字段，没有项目设置或 MCP 字段（[`crates/domain/src/project.rs`](../../crates/domain/src/project.rs#L4-L11)）。SQLite 的 `projects` 表同样只有这些字段（[`schema_v0001.rs`](../../crates/db/src/migration/schema_v0001.rs#L4-L11)）。
- `tasks` 已通过 `project_id` 归属于项目，因此 Ora 已经有稳定的项目主键，不需要再发明一个项目配置身份（[`schema_v0001.rs`](../../crates/db/src/migration/schema_v0001.rs#L13-L22)）。
- 当前 `plugin_state` 是**全局**的插件生命周期资格状态：主键只有 `plugin_id`，值只有 `enabled` 与时间戳，没有 `project_id`（[`schema_v0007.rs`](../../crates/db/src/migration/schema_v0007.rs#L3-L9)）。它不能表达“项目 A 启用，项目 B 不启用”。
- 当前工作树已有 `user_config(key, value)`，但迁移注释明确它用于“non-sensitive user preferences”（[`schema_v0008.rs`](../../crates/db/src/migration/schema_v0008.rs#L3-L16)），应用端也把它包装成只支持已知用户偏好的强类型 repository，而不是任意项目配置容器（[`crates/application/src/user_config.rs`](../../crates/application/src/user_config.rs#L20-L35)）。
- 数据库已有线性 migration catalog，因此增加一张表属于现有数据库的普通演进方式，不是创建新数据库（[`crates/db/src/migration/README.md`](../../crates/db/src/migration/README.md)）。

### 1.2 可选方案与工作量

| 方案                                               | 数据库迁移 | 优点                                                     | 代价/风险                                                                          | 结论                                 |
| -------------------------------------------------- | ---------- | -------------------------------------------------------- | ---------------------------------------------------------------------------------- | ------------------------------------ |
| 现有 SQLite 新增关联表                             | 需要一次   | 私有于本机；可查询所有引用项目；卸载时容易清理；关系清晰 | 需新增 domain/port/repository/contracts/UI 流程                                    | **推荐**                             |
| `projects` 增加 JSON 列                            | 需要一次   | 表面上少一张表                                           | 每次 Project CRUD 都被迫搬运 JSON；无法做关系约束；局部更新和查询差                | 不推荐                               |
| 复用 `user_config`，用 `project/<id>/mcps` 存 JSON | 不需要     | 最快做出原型                                             | 违反全局非敏感偏好的现有边界；孤儿清理、查询、损坏隔离都差                         | 只适合一次性原型                     |
| 项目根目录新增 `.ora/mcp.json`                     | 不需要     | 可随仓库共享；与 VS Code 的 workspace 配置思路相似       | 会修改用户仓库；必须决定是否提交 Git；多 worktree 一致性、外部编辑和原子写都要处理 | 只有产品明确要求“团队共享声明”时采用 |
| 直接把 Agent 的实际配置当 Ora 期望状态             | 不需要     | 没有新增持久化                                           | 各 Agent 格式不同；换 Agent 后无法知道 Ora 意图；卸载与修复不可可靠 reconcile      | 禁止                                 |

推荐表形状：

```sql
CREATE TABLE project_mcp_enablements (
    project_id TEXT NOT NULL,
    plugin_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (project_id, plugin_id)
);
```

这里只保存“项目希望启用哪个 MCP”的声明，不保存运行配置值，更不能保存 secret。MCP 安装仍是全局事实；创建对话时，Ora 根据项目 ID 读取完整期望集合，再把经过验证的安装描述交给所选 Agent 插件。

### 1.3 工作量判断

**推断：**持久化本身是小到中等改动，不是“大到需要新子系统”：一份 migration、一个关系模型、一个 application repository trait、一个 SQLite adapter 及测试即可。完整产品链路是中等工作量，因为还包括前端项目选择、contracts、卸载清理和对话启动时读取。它应与“MCP manifest 定义/解析”拆成独立任务，避免当前负责人被迫同时改 Agent 启动链路。

**已确认决策：**若以后实现项目默认集合，它是当前 Ora 用户的私有偏好，使用现有 SQLite；不写成团队共享项目文件。VS Code 同时提供 workspace 和 user profile 两个配置范围，并将启用/禁用状态与共享的 `mcp.json` 分开保存，这说明二者是不同产品语义，而不是存储实现细节（[VS Code 官方 MCP 配置](https://code.visualstudio.com/docs/agent-customization/mcp-servers)）。

## 2. 两套 manifest 的真实职责和来源

### 2.1 `crates/plugin-manifest`：发布/市场 release manifest

**代码事实：**

- README 明确称它解析“one published Ora plugin release”的 TOML（[`crates/plugin-manifest/README.md`](../../crates/plugin-manifest/README.md#L1-L5)）。
- 字段包括 `resolver`、`name`、`namespace`、`kind`、SemVer `version`、展示元数据、release `url`、`sha256`、源码 `head` 和 Ora 版本要求（[`manifest.rs`](../../crates/plugin-manifest/src/manifest.rs#L10-L27)）。
- 它严格拒绝未知 TOML 字段，并将 URL、摘要、名称等转换为有不变量的值对象（[`manifest.rs`](../../crates/plugin-manifest/src/manifest.rs#L212-L240)）。
- README 明确把文件系统、下载、安装、发现、执行和与 `ora-plugin-manager` 的集成列为 non-responsibilities（[`crates/plugin-manifest/README.md`](../../crates/plugin-manifest/README.md#L16-L22)）。
- 全仓库除 workspace 声明和 crate 自身外，没有 `ora_plugin_manifest` / `PluginManifest::parse` 的生产调用。因此它目前是可用的解析库，但还没有接入市场同步或安装路径。

### 2.2 `crates/plugin-manager`：安装后 package manifest

**代码事实：**

- 它扫描 `<data-dir>/plugins` 的直接子目录，并读取每个包的 `package.json`（[`discovery.rs`](../../crates/plugin-manager/src/discovery.rs#L15-L33)）。
- `package.json` 模型保留 npm 的 `name`、`version`、`type`，并在 `ora` 下描述 `id`、display name、kind、运行入口、宿主/Plugin API/Bun engine 以及 contribution（[`crates/plugin-manager/src/manifest.rs`](../../crates/plugin-manager/src/manifest.rs)）。
- 校验实际运行所需事实：SemVer、ES module、manifest/API/Agent contract 版本、安全相对入口和贡献类型；当前仅接受 `agent` kind（[`validation.rs`](../../crates/plugin-manager/src/validation.rs#L84-L140)）。
- 这是现有生产链路真正使用的 manifest：`PluginLifecycle::open` 调用 `PluginManager::discover`（[`crates/plugin-lifecycle/src/lib.rs`](../../crates/plugin-lifecycle/src/lib.rs#L149-L160)），Desktop bootstrap 也直接发现它并构造 Agent 插件包（[`apps/desktop/src-tauri/src/lib.rs`](../../apps/desktop/src-tauri/src/lib.rs#L238-L250)）。

### 2.3 `docs/plugin-doc` 的设计意图与矛盾

这些文档目前描述的是一个更大的规划态系统：市场源下的 `registry/.../orax.toml` 带 URL、SHA-256 和 host dependency；`.orax` 包内又包含 `orax.toml`；本地发现段落却写解析 `manifest.toml`（[`plugin-manager.md`](plugin-manager.md)、[`plugin-runtime.md`](plugin-runtime.md#L42-L52)）。这三种名字与当前生产代码固定读取的 `package.json` 不一致。

因此文档能证明的稳定意图是：

- 市场条目负责发现、展示、release 定位与完整性；
- 安装包负责运行入口和 capability；
- 安装需要下载、校验、解压和生命周期管理。

文档**不能**证明已经决定“市场和包内必须共用同一份 manifest”，也不能证明当前两 crate 已有交叉校验。

### 2.4 为什么会出现两套

提交历史提供了直接证据：

- 安装后 `package.json` discovery 先在 2026-08-05 的 [`b32c39aa`](https://github.com/ora-space/desktop/commit/b32c39aaa042c7547ff4b480008ed0aab7193f5e) 引入；
- 发布侧 TOML parser 后在 2026-08-17 的 [`f6cf2212`](https://github.com/ora-space/desktop/commit/f6cf22125892405ab7f384b1b107d2abe7042e79) 独立引入；其 README 明确声明不集成 `ora-plugin-manager`。

**推断：**两套不是同一管线有意识拆分后一起交付，而是两个不同阶段/参与者分别实现了“本地运行发现”和“市场发布解析”。职责上的分层是合理的；协议字段、文件名和安装桥接还没有完成统一。

### 2.5 Q20 的决定

继续选择 **B（两层 manifest）**，但要把当前偶然并存提升成明确契约：

```text
MarketplaceReleaseManifest (TOML)
  展示、namespace/name、kind、version、release assets、SHA-256、Ora compatibility
                   │
                   │ 下载、摘要校验、staging 解压
                   ▼
Package manifests (包内；职责互补)
  orax.toml：Ora canonical id、kind、兼容性、权限及 HTTP 扩展
  manifest.json：MCPB version、server/runtime、stdio config、user_config、展示元数据
```

安装器必须交叉校验至少 `canonical id`、`kind` 和 `version`。市场层不能覆盖包内启动命令，包内层也不能自行改变用户在市场确认的身份和版本。

包内格式现已决定为 `.orax` ZIP 中同时存在 `orax.toml` 与 MCPB `manifest.json`。这不是两个竞争的权威清单：TOML 只拥有 Ora identity/trust/policy 与 HTTP 扩展；JSON 拥有 MCPB server/runtime、stdio config、`user_config` 和通用展示元数据。共享 version 与外层市场 manifest 做 exact-match，冲突即拒绝，不能用读取优先级掩盖。

## 3. 当前 ID、namespace、version 规则与 Q22

### 3.1 代码与文档事实

- 发布 manifest 的 resolver v1 只接受 `namespace = "official"`（[`enums.rs`](../../crates/plugin-manifest/src/enums.rs#L4-L37)）。
- 发布侧 `PluginName` 允许一到两个由点号分隔的 slug，整个名称最多 128 bytes（[`name.rs`](../../crates/plugin-manifest/src/name.rs#L5-L38)）；每个 slug 最多 63 bytes，只允许小写 ASCII、数字和单连字符，不能以连字符开头/结尾（[`crates/utils/src/slug.rs`](../../crates/utils/src/slug.rs#L4-L38)）。
- 发布侧版本由 Rust `semver::Version` 严格解析（[`manifest.rs`](../../crates/plugin-manifest/src/manifest.rs#L43-L55)）。
- 市场规划文档定义展示 ID 为 `namespace + name`，示例是 `official/user.ora-weather`（[`plugin-manager.md`](plugin-manager.md)）。
- 安装后 `package.json` 的 `ora.id` 当前只校验非空；没有 namespace/slug 约束，也不要求等于 npm package `name`（[`validation.rs`](../../crates/plugin-manager/src/validation.rs#L84-L130)）。duplicate 检测也只按这个原始字符串做精确匹配（[`discovery.rs`](../../crates/plugin-manager/src/discovery.rs#L29-L49)）。
- 当前 Agent 示例使用 `ora.claude-code`，与市场侧 `official/user.ora-weather` 的形状不同。这证明当前并不存在跨 manifest 的 canonical ID 规则。

### 3.2 建议：Q22 选择 B，但采用现有 Ora 语义定稿

1. Canonical `PluginId` 定义为 `namespace/name`；resolver v1 暂时只有 `official`。示例：`official/github-mcp` 或保留现有双段 name 时为 `official/acme.github-mcp`。
2. ID 由市场注册流程分配/确认，之后不可因 GitHub repository rename、transfer 或 Release 文件名变化而改变。仓库 URL只是来源元数据，不是身份。
3. 市场 manifest 和包内 manifest 都携带完整 canonical ID；安装时必须精确相等。不要继续让 `ora.id` 使用另一套 `ora.foo` 别名。
4. 版本必须是 SemVer；市场 release、包内 manifest、资产命名中的版本必须一致。
5. 同一个 `id + version` 一旦进入市场，下载 URL 所指内容和 SHA-256 不得被替换。修复发布新版本。
6. `namespace` 将来若开放 community/publisher，应由 marketplace 的发布者身份或审核流程赋予，不能允许包作者随意声称 `official`。

MCP 官方 Registry 使用自己的服务器命名（例如 `io.github.publisher/server`）。如 Ora 后续接入官方 Registry，应把它保存为 `upstream_mcp_name`，不要让外部 registry 名直接替换 Ora 的 canonical plugin ID。

## 4. MCP 配置输入与 secret schema（Q23）

### 4.1 仓库事实

- 当前发布 manifest 没有配置输入字段；包内 `package.json` 也只有 Agent contribution，没有 MCP 或 inputs 模型。
- `docs/plugin-doc/plugin-runtime.md` 规划了 `env` capability 和普通 KV storage，但它描述的是 Deno Ora 插件运行时权限，不是用户凭据的安全存储，也没有加密/系统凭据库保证（[`plugin-runtime.md`](plugin-runtime.md#L60-L70)）。
- 全仓库没有 keyring/credential/secret-storage 实现；现有 `user_config` 明确只用于非敏感偏好。
- 日志层已有常见 token/API key 文本脱敏，但日志脱敏不能替代 secret storage（[`crates/logging/src/error_report.rs`](../../crates/logging/src/error_report.rs)）。

### 4.2 官方一手资料

#### MCP Registry

MCP 官方 Registry 的 `server.json` quickstart 已用 `environmentVariables` 声明 API key，字段包括 `name`、`description`、`isRequired`、`format` 和 `isSecret`，而不在示例中嵌入真实值（[MCP Registry 官方 quickstart](https://modelcontextprotocol.io/registry/quickstart)）。官方 schema 的共享 `Input` 还定义 `string | number | boolean | filepath` 格式、required/secret 标记、placeholder 和固定 value；`KeyValueInput` 加 `name` 后用于 environment variables（[官方 Registry schema](https://github.com/modelcontextprotocol/registry/blob/main/docs/reference/server-json/draft/server.schema.json)）。Registry 仍在演进，因此 Ora 应复用其核心语义，而不是与 draft 的每个字段强耦合。

#### VS Code

VS Code 的 `mcp.json` 把输入声明放在 `inputs`，server 的 `env` 只写 `${input:...}` 引用；首次启动时提示并安全保存实际值。它支持 `promptString`、`pickString`、`command`，并用 `password: true` 隐藏敏感输入（[VS Code 官方 MCP configuration reference](https://code.visualstudio.com/docs/agents/reference/mcp-configuration)）。VS Code Extension API 的 `SecretStorage` 明确承诺敏感值加密、实现随平台变化且不跨机器同步（[VS Code SecretStorage API](https://code.visualstudio.com/api/references/vscode-api#SecretStorage)）。

#### Claude Code

Claude Code 把 project-scope MCP 存为可提交的项目根 `.mcp.json`，把 local/user MCP 存到用户 `~/.claude.json`；官方明确建议 local scope 用于不希望进入版本控制的 credentials。共享 `.mcp.json` 支持 `${VAR}` 与 `${VAR:-default}` 环境变量展开，包括 `command`、`args`、`env`、URL 和 headers（[Claude Code 官方 MCP 文档](https://code.claude.com/docs/en/mcp)）。这支持 Ora 只提供输入语义，由 Claude Agent 插件决定生成 `${VAR}`、local scope 或其他安全引用。

#### MCP runtime elicitation

MCP 的运行时 elicitation 不是安装配置的通用替代品。官方规范要求敏感数据不得经 form mode 请求，应使用不会把凭据暴露给 MCP client/LLM 的 URL mode（[MCP 2025-11-25 Elicitation 规范](https://modelcontextprotocol.io/specification/2025-11-25/client/elicitation)）。对于启动前就需要环境变量的本地 stdio server，仍然必须在进程启动前解决输入。

MCP 官方 TypeScript SDK 的 stdio client 默认使用 `getDefaultEnvironment()`，文档明确说它只包含“deemed safe to inherit”的环境变量，而不是复制整个父进程环境（[MCP TypeScript SDK stdio API](https://ts.sdk.modelcontextprotocol.io/v2/api/%40modelcontextprotocol/client/client/stdio.html)）。Ora 应采用同样的 allowlist 思路，再叠加 manifest 明确声明和用户授权。

### 4.3 Q23 建议（按新 MCPB profile 修订）

选择 **B：沿用 `manifest.json.user_config` 的强类型 input schema，但不保存值**。Ora 不再另造一份 TOML inputs；它严格解析 MCPB 类型、约束及 `${user_config.KEY}` 在 command/args/env 中的引用，转换为：

```text
McpInputSpec {
  id,
  title,
  description,
  value_kind: String | Number | Boolean | Directory { multiple } | File { multiple },
  required,
  sensitivity: Plain | Secret,
  default,
  numeric_bounds,
  placements: [Command | Argument { index } | EnvironmentVariable { name }]
}
```

MCPB schema 允许引用落在 args 或 env；为了凭据安全，Ora profile 应增加更严格的不变量：`sensitive = true` 的 input 只能用于 `env` 或 HTTP header 的 secret reference，禁止用于 command/args，也禁止携带 default。plain path/number/boolean 仍可按 MCPB 语义展开到 args。Agent 插件接收已解析的 placement DTO，不需要猜测 input，也不直接解释模板字符串。

### 4.4 必须写进 validator 的安全不变量

1. `Secret` 输入禁止 manifest `default`、固定 `value` 或示例 secret，并禁止引用到 command/args。
2. 实际值禁止进入包、市场索引、项目启用集合、日志、错误文本和可提交的 Agent 配置。
3. Agent 插件接收的是 input declaration 与 opaque secret reference/目标 Agent placeholder，不是可随意序列化的明文值。
4. 启动 MCP 时只注入该 manifest 明确声明且用户已授权的环境变量；不能继承 Ora 的完整环境。
5. 非 secret default 必须类型匹配；required input 在启动前必须能解析，否则返回“configuration required”，不能启动后静默失败。
6. manifest 合法只代表结构可安装，不代表 token 有效、网络可达或 MCP server 能握手；这与已选择的 Q24.B 一致。

### 4.5 Ora 当前没有 secret store 时的 v1 决策

按安全性排序：

1. **推荐：**Agent 插件把 `Secret` 翻译成目标 Agent 支持的安全输入引用或用户私有 scope；例如 VS Code `${input:...}`、Claude `${VAR}`/local scope。
2. **可接受：**Ora 只声明所需环境变量，让用户在外部环境设置；Agent 插件生成 `${VAR}` 引用，Ora 不读取 secret。
3. **后续：**Ora 增加 OS credential store 后，项目状态只保存 opaque secret reference，运行前按最小权限注入。
4. **禁止退化：**因为当前没有 secret store，就把 token 写入 SQLite `user_config`、项目 `.mcp.json`、`.claude/settings.json` 或 plugin manifest。

如果某 Agent 既没有安全输入引用，又只能把明文写入项目，则该 Agent + MCP 组合应在 v1 报 `SecretInputUnsupported`，而不是牺牲凭据边界。

## 5. 对当前 MCP 插件定义工作的直接实施建议

以下顺序能保持当前工作边界，不提前侵入 Agent 启动实现：

1. 定稿 canonical `PluginId`、SemVer 与两个 manifest 的交叉校验规则。
2. 在发布 manifest 增加 `mcp` kind；不要继续只有 `workbench`。
3. 定义 MCP `.orax` profile：根 `manifest.json` 必须通过锁定的 MCPB 0.3 strict schema，根 `orax.toml` 只增加 Ora identity/trust/policy 与 HTTP 扩展。
4. 将 MCPB `user_config`、模板、platform override 严格解析为 Ora 强类型 DTO；secret 只允许安全引用，禁止固定明文值。
5. 定义安装器输出的强类型 `InstalledMcpDescriptor`，只包含已验证入口、args、工作目录、input declarations、permissions 和包根，不包含 secret values。
6. 定义给外部 Agent 插件消费的只读 DTO；配置时点、兼容性选择和写 Agent 配置仍是外部方案。
7. 把 `project_mcp_enablements` 及 UI 选择拆成后续任务。若产品尚未确认“用户私有 vs 团队共享”，先不要用 `user_config` 临时固化错误语义。

建议的边界测试至少包括：

- 市场与包内 ID/kind/version 不一致时拒绝安装；
- canonical ID、SemVer、未知字段、重复 input ID、非法环境变量名；
- secret 带 default/value 时拒绝；
- 入口绝对路径、父目录穿越、symlink escape；
- stdio、remote Streamable HTTP、bundled Streamable HTTP 的排他组合校验；deprecated SSE 明确返回 unsupported；
- 官方 MCPB 0.3 CLI/schema compatibility fixture，以及 pack/unpack 后 `orax.toml` 保留测试；
- 合法包产生完整且无 secret value 的 `InstalledMcpDescriptor`。

## 6. 已确认的产品决策与剩余边界

截至本次讨论，已经确认：

1. 若未来保存项目默认 MCP 集合，它是当前 Ora 用户的本地偏好，使用现有 SQLite 演进，不写入仓库共享文件。
2. Ora 插件统一使用 `.orax` 包格式；MCP kind 保留 MCPB 根 `manifest.json` 和内容结构，并新增根 `orax.toml`。stdio profile 以锁定的 MCPB 0.3 schema/启动语义为兼容基线；HTTP 是明确的 Ora 扩展。
3. 第一版允许 MCP 声明 secret input，但 Ora 不读取或保存真实 secret；只允许目标 Agent 的安全输入机制或系统环境变量。不能安全表达时返回 `SecretInputUnsupported`。
4. 第一版不承诺运行中热插拔；MCP 配置最迟在新建或重启 Agent session 前物化。

仍需由对话/Agent 集成任务决定的不是 MCP 包格式，而是“选择作用域”：保存项目默认集合，还是保存对话的期望集合。后文给出一个可以覆盖已创建对话增删 MCP、又不要求当前 MCP 包开发立刻实现项目数据库的建议。

## 7. Ora `.orax` 规划的完整语义与现状差异

### 7.1 规划文档表达的两层清单

**文档事实：**`plugin-manager.md` 描述的是市场和安装管理：市场仓库的 `registry/.../orax.toml` 包含 `namespace`、`name`、`kind`、版本、Release `.orax` URL、SHA-256、源码位置及 Ora 版本依赖；同步后生成轻量 `registry_index.json`，安装则经历下载、Hash 校验、解压、启禁、升级和回滚（[`plugin-manager.md`](plugin-manager.md#L1-L136)）。这是**外层发布清单**。

**文档事实：**`plugin-runtime.md` 又把 `.orax` 定义成解压包容器，包根包含 `orax.toml`、`main.js`、`logo.svg`、`README.md` 与可选 assets；这里的 `orax.toml` 承担入口和 Capability 声明，并由 `orax pack` 生成包（[`plugin-runtime.md`](plugin-runtime.md#L1-L52)）。这是**包内运行清单**。

因此，“Ora 统一使用 `.orax`”可以与前面已经选择的“两层 manifest”同时成立：

```text
marketplace/registry/.../orax.toml       # 发布、下载、摘要、Ora 兼容性
                   │
                   │ 下载并校验
                   ▼
plugin-version.orax                      # Ora 定义的压缩包容器
└── orax.toml                            # 包内身份、贡献、入口、输入、权限
```

两层可以都叫 `orax.toml`，因为它们处于不同上下文，但必须有不同的 schema/resolver，并在错误中明确报告“marketplace release manifest”或“package manifest”。安装器仍要交叉校验 canonical ID、kind 和 version，不能把外层字段静默覆盖到内层。

### 7.2 目录、权限与运行时的规划语义

**文档事实：**规划目录是 `~/.ora/plugins/{sources,installed,data,cache}`；安装主体和插件持久数据分开，`.orax` 下载缓存也单独保存（[`plugin-manager.md`](plugin-manager.md#L1-L11)）。`plugin-runtime.md` 的另一段又写数据位于 `~/.ora/data/plugins/<plugin-id>`，和前述 `~/.ora/plugins/data/<plugin-id>` 不一致（[`plugin-runtime.md`](plugin-runtime.md#L467-L489)）。

**文档事实：**通用 JS 插件被规划为 Deno 子进程。`network`、`fs.read`、`fs.write`、`env`、`shell` capability 被翻译为 Deno allow flags；高级 UI/storage/workspace 能力再经 SDK 和宿主二次鉴权（[`plugin-runtime.md`](plugin-runtime.md#L54-L81)、[`plugin-runtime.md`](plugin-runtime.md#L319-L356)）。插件使用 Ora 自定义的 length-prefixed stdio JSON-RPC 协议注册并调用能力，生命周期包括 activate、deactivate、超时关闭与进程强杀（[`plugin-runtime.md`](plugin-runtime.md#L104-L184)、[`plugin-runtime.md`](plugin-runtime.md#L375-L445)）。

**重要边界：**这些是 **Ora JS/Agent 插件运行时** 的语义，不是 MCP Server 的运行协议。MCP stdio server 的 stdout 属于 MCP JSON-RPC，不能套用 Ora 插件的自定义 frame；本方案又规定 MCP 由目标 Agent 启动，因此 Deno flags 也不能自动约束 bundled MCP binary。MCP 的 permissions 在第一版只能是审核和安装页展示的“权限意图”，除非目标 Agent 插件明确提供可执行的沙箱能力。

`plugin-deno.md` 是选型讨论记录而不是稳定规范：文档先推荐 Deno，随后因 `--allow-run` 改推 Fence+Bun，最后又在“JS 胶水 + 宿主托管二进制”前提下改回 Deno（[`plugin-deno.md`](plugin-deno.md#L1-L145)、[`plugin-deno.md`](plugin-deno.md#L260-L332)）。其中未引用来源的启动耗时、内存和发布节奏不能用作 manifest 契约。可稳定吸收的只有一个设计原则：插件若需执行高风险子进程，应由宿主/受控适配器代理，而不是把 unrestricted spawn 当成普通 capability。

`plugin-template.md` 规划用 Deno + esbuild 将 `main.ts` 构建为 `dist/main.js`（[`plugin-template.md`](plugin-template.md#L1-L39)），但 `plugin-runtime.md` 的包布局把入口写成根目录 `main.js`，启动示例又使用 `main.ts`。这说明入口必须由包内清单显式给出，不能靠固定文件名猜测。

### 7.3 与当前生产代码的矛盾

| 主题         | 规划文档                                    | 当前生产代码                                            | 结论                                   |
| ------------ | ------------------------------------------- | ------------------------------------------------------- | -------------------------------------- |
| 已安装目录   | `~/.ora/plugins/installed/<id>`             | 扫描 `<data-dir>/plugins/<direct-child>`                | 以代码为当前事实；目录迁移需单独设计   |
| 包内清单     | `orax.toml`；发现段落还误写 `manifest.toml` | 固定读取 `package.json`                                 | `.orax` 包内 schema 尚未接入生产发现器 |
| contribution | 通用 workbench/JS 插件规划                  | `package.json.ora.kind` 只接受 `agent`                  | MCP kind 目前不存在                    |
| 市场 kind    | 文档示例为 `workbench`                      | `ora-plugin-manifest` resolver v1 只接受 `workbench`    | MCP 发布清单也需要新 resolver 或扩展   |
| 运行时       | 文档主线倾向 Deno                           | `PluginRuntime` 实际执行 `deno run --no-prompt`         | Deno 是当前启动事实                    |
| engine 字段  | 文档为 Deno                                 | `package.json` 却要求 `ora.engines.bun`                 | 现有包协议与实际 launcher 明显不一致   |
| 签名         | 文档提出 SHA-256 + GPG/Ed25519              | 发布 parser 只有 SHA-256，无签名字段                    | 数字签名仍是未实现规划                 |
| 更新/平台    | 文档提出平台矩阵、原子回滚                  | 当前 release manifest 只有单一 URL，manager 不安装/更新 | 需要新的安装桥梁和平台资产模型         |

代码依据：当前发现器拼接 `plugins` 并固定读取 `package.json`（[`crates/plugin-manager/src/discovery.rs`](../../crates/plugin-manager/src/discovery.rs#L15-L33)）；包模型要求 npm `type`、Ora `main`、Bun engine 和 agent contribution（[`crates/plugin-manager/src/manifest.rs`](../../crates/plugin-manager/src/manifest.rs)），验证器只接受 `agent`（[`validation.rs`](../../crates/plugin-manager/src/validation.rs#L190-L223)）。实际 runtime 则调用 Deno（[`crates/plugin-runtime/src/lib.rs`](../../crates/plugin-runtime/src/lib.rs#L28-L39)、[`crates/plugin-runtime/src/lib.rs`](../../crates/plugin-runtime/src/lib.rs#L94-L119)）。发布侧 TOML parser 确实拥有 URL/SHA/Ora dependency 且拒绝未知字段（[`crates/plugin-manifest/src/manifest.rs`](../../crates/plugin-manifest/src/manifest.rs#L10-L27)、[`crates/plugin-manifest/src/manifest.rs`](../../crates/plugin-manifest/src/manifest.rs#L210-L233)），但 resolver v1 的 namespace/kind 只有 `official`/`workbench`（[`crates/plugin-manifest/src/enums.rs`](../../crates/plugin-manifest/src/enums.rs)）。

**推断：**`.orax` 是明确的目标方向，但还不是已落地的统一生产格式。当前 MCP 开发不能声称“Ora 已兼容 `.orax` MCP”；它要补齐 MCPB `manifest.json` 的锁版本 parser/validator、Ora `orax.toml` parser、打包/安装桥梁和 discovery 迁移。生产发现器不应继续猜测 `package.json`/`manifest.toml`；MCP kind 中 JSON 与 TOML 的双文件是有意的互补协议，权威边界必须由 validator 固化。

### 7.4 文档还不能当成生产契约的具体证据

- 市场文档的 `orax.toml` 示例缺少生产 parser 强制要求的 `resolver` 字段；当前 crate 只接受 `resolver = 1`（[`plugin-manager.md`](plugin-manager.md#L18-L40)、[`crates/plugin-manifest/src/manifest.rs`](../../crates/plugin-manifest/src/manifest.rs#L10-L41)）。
- 文档要求根据 capability 构造 Deno permissions，但当前 lifecycle 启动器实际传入空列表（[`crates/plugin-lifecycle/src/runtime.rs`](../../crates/plugin-lifecycle/src/runtime.rs#L49-L73)）。所以 capability enforcement 既没有包内字段，也没有启动接线。
- 文档规划 `ora.http`、`ora.storage`、`ora.system.spawn` 之类插件调用宿主的代理 API，但当前 runtime 明确禁止 reverse request/response：插件只能向宿主发白名单 notification，带 request id 的插件消息会使连接失效（[`crates/plugin-runtime/README.md`](../../crates/plugin-runtime/README.md#L15-L45)、[`crates/plugin-runtime/README.md`](../../crates/plugin-runtime/README.md#L65-L70)）。实现 SDK broker 前不能把这些 API 写进 v1 保证。
- 文档把 frame length 写成 `i32` 并计划保留未知 frame 以兼容未来类型；生产 codec 使用 `u32`，收到非 `0x01` frame 立即报错（[`plugin-runtime.md`](plugin-runtime.md#L104-L184)、[`crates/plugin-runtime/src/codec.rs`](../../crates/plugin-runtime/src/codec.rs#L5-L38)）。
- 文档规划指数退避、崩溃熔断、`ora/activate`/`ora/deactivate` 和结构化 stderr 日志；当前生产协议以 `ora/register`/`ora/shutdown` 为主，生命周期将异常置为 Failed，没有文档所述自动重启策略。它们应被视为后续目标，不应影响 MCP `.orax` v1 的安装成功定义。
- 仓库没有 `orax pack`、`.orax` download/extract 或 `@ora-space/create-plugin` 的生产实现。当前 release crate 只验证 SHA-256 字符串形状，不计算下载文件摘要；`.orax` 安装器仍需从零补齐。

## 8. 新设计：`.orax` 作为 MCPB 内容格式的 Ora profile

本节替代此前“只参考 MCPB、由 Ora 重新定义全部 MCP 元数据”的建议。已确认的新方向是：MCP kind 的 `.orax` 保留 MCPB 的 ZIP 内容结构和根 `manifest.json`，只新增根 `orax.toml`；Ora 同时支持 stdio 与 Streamable HTTP。这里的“基本兼容”必须写成可验证的兼容等级，不能只凭两个 ZIP 看起来相似就宣称兼容。

### 8.1 MCPB 当前权威事实

#### 包结构

**官方事实：**MCPB README 将 `.mcpb` 定义为包含本地 MCP Server 和根 `manifest.json` 的 ZIP；`manifest.json` 是唯一普遍必需的文件。`server/`、`node_modules/`、`lib/`、`package.json`、图标和 assets 都是按 runtime 给出的示例，并不是 archive 文件白名单（[MCPB README 的目录结构](https://github.com/modelcontextprotocol/mcpb#directory-structures)）。因此 Ora 的 MCP 包可以采用：

```text
plugin.orax                         # ZIP archive
├── manifest.json                  # MCPB manifest，保留原语义
├── orax.toml                      # Ora identity/trust/policy 扩展
├── server/                        # 与 MCPB 相同
├── node_modules/ | lib/ | ...     # 按 runtime 可选
├── package.json                   # 可选
├── icon.png | assets/             # 可选
└── 其他 MCPB bundle 文件
```

**源码事实与推断：**MCPB pack 会收入未被固定规则或 `.mcpbignore` 排除的文件，`orax.toml` 不在排除列表；unpack 会写出 ZIP 中任意通过 zip-slip 检查的条目；validator 校验 `manifest.json`、入口和图标等，不维护 archive 顶层文件白名单（[files.ts](https://github.com/modelcontextprotocol/mcpb/blob/main/src/node/files.ts)、[pack.ts](https://github.com/modelcontextprotocol/mcpb/blob/main/src/cli/pack.ts)、[unpack.ts](https://github.com/modelcontextprotocol/mcpb/blob/main/src/cli/unpack.ts)）。所以官方 MCPB CLI **按当前源码应当会保留并忽略额外的 `orax.toml`**；这是对工具源码的推断，不是所有第三方 MCPB host 的正式兼容承诺。

#### manifest 版本与严格 schema

**官方事实：**上游当前存在一个必须显式处理的不一致：

- `MANIFEST.md` 仍声明 current version 为 `0.3`，示例和 `DEFAULT_MANIFEST_VERSION` 也使用 `0.3`；
- 当前 source/schema 同时接受 `0.1`、`0.2`、`0.3`、`0.4`，`LATEST_MANIFEST_VERSION = "0.4"`；
- `0.4` 在 `server.type` 中新增 `uv`；
- 严格 v0.4 schema 仍要求 `server.entry_point` 和 `server.mcp_config`，但 `MANIFEST.md` 的 UV 文本称 `mcp_config` 可选。

依据：[MANIFEST.md](https://github.com/modelcontextprotocol/mcpb/blob/main/MANIFEST.md)、[constants.ts](https://github.com/modelcontextprotocol/mcpb/blob/main/src/shared/constants.ts)、[v0.3 schema](https://github.com/modelcontextprotocol/mcpb/blob/main/src/schemas/0.3.ts)、[v0.4 schema](https://github.com/modelcontextprotocol/mcpb/blob/main/src/schemas/0.4.ts)。

**建议：**Ora v1 的强兼容 baseline 固定为 MCPB `manifest_version = "0.3"`，不要把 upstream `latest` 当作浮动依赖。可以另行实现经测试的 v0.4 feature，但必须按 schema/source 的更严格交集处理，不依赖 UV 文档与 schema 的矛盾行为。每个支持版本都要有固定 fixture 和独立 parser；未知版本 fail closed。

#### `server`、`mcp_config`、platform override 与 `user_config`

MCPB 0.3 的严格核心是：

- 根必需字段：`name`、SemVer `version`、`description`、`author.name`、`server`；`manifest_version` 是版本判定字段，遗留 `dxt_version` 仍被 source resolver 接受。
- `server.type` 为 `node | python | binary`；0.4 才增加 `uv`。
- `server.entry_point` 与 `server.mcp_config` 必需。
- `mcp_config.command` 必需；`args`、`env`、`platform_overrides` 可选。
- 每个 platform override 只能覆盖 `command`、`args`、`env`。当前 config source 按 `process.platform` 选 override：command/args 使用 override 值替换，env 与基础 env 合并（[config.ts](https://github.com/modelcontextprotocol/mcpb/blob/main/src/shared/config.ts)）。
- `mcp_config` 支持 `${__dirname}`、`${pathSeparator}`/`${/}` 和 `${user_config.<key>}` 等替换；Ora 若宣称 MCPB 运行兼容，就必须按锁定版本复刻并测试这些语义，不能定义一个同名但行为不同的模板语言。
- `user_config` 是 keyed object；每项类型为 `string | number | boolean | directory | file`，需要 `title` 与 `description`，可声明 `required`、`default`、`multiple`、`sensitive`、`min`、`max`。值可以代入 args/env，多选值在 args 中展开；sensitive 输入由实现方遮蔽并安全保存（[MCPB user configuration](https://github.com/modelcontextprotocol/mcpb/blob/main/MANIFEST.md#user-configuration)）。

严格 schema 的 `additionalProperties: false` 作用于 `manifest.json` 对象，而不是 ZIP 的其他文件。因此 `orax.toml` 作为另一个 archive entry 不会使 `manifest.json` schema 失败；把 `transport`、`url` 或 Ora 字段直接塞进当前 MCPB JSON 则会失败。

### 8.2 MCPB 当前没有 HTTP transport

**官方事实：**MCPB README 的目标是“一键安装本地 MCP Server”，示例要求 stdio；当前严格 schema 的 `mcp_config` 只有 `command`、`args`、`env`、`platform_overrides`，没有 `transport`、`url`、`headers`。因此 MCPB 当前不原生支持：

- 远程 Streamable HTTP；
- bundle 内启动本地 Server 后再连接其 HTTP endpoint；
- 远程或本地旧版 HTTP+SSE；
- 一个 package 声明多个 transport/mode。

[MCPB issue #176](https://github.com/modelcontextprotocol/mcpb/issues/176) 正在提议 `transport + url + headers`，并区分 bundled/reference 与 stdio/http 四种组合；截至调查日仍是 open enhancement，且没有关联 PR。它是设计输入，不是规范。[Issue #20](https://github.com/modelcontextprotocol/mcpb/issues/20) 曾提议同时声明 SSE、Streamable HTTP 与本地 fallback，已 closed as not planned。Ora 不能把 issue 中的示例当成已发布的 MCPB 字段。

**MCP 官方事实：**当前标准 transport 是 stdio 与 Streamable HTTP。Streamable HTTP 使用一个 MCP endpoint 接受 POST，响应可以是 JSON 或该请求范围内的 SSE stream；旧的 HTTP+SSE 是 deprecated，新实现不应采用（[stdio 规范](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio)、[Streamable HTTP 规范](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http)、[deprecated features](https://modelcontextprotocol.io/specification/2026-07-28/deprecated)）。因此 Ora 字段应命名为 `streamable-http`，不要使用含义模糊的 `http`，也不要把“响应可以是 SSE stream”误解成遗留 `sse` transport。

### 8.3 可测试的兼容等级与方向

建议在设计、validator 和市场 UI 中明确以下等级：

| 等级                      | 可自动验证的含义                                                             | stdio `.orax`          | HTTP `.orax`                                        |
| ------------------------- | ---------------------------------------------------------------------------- | ---------------------- | --------------------------------------------------- |
| C0：archive-compatible    | 是安全 ZIP；根有 `manifest.json`；MCPB 目录/变量没有被 Ora 改名              | 可以                   | 可以                                                |
| C1：manifest-compatible   | `manifest.json` 通过 Ora 锁定的官方 MCPB strict schema                       | 可以                   | 只有保留一个真实合法的 MCPB local-server 描述才可以 |
| C2：launch-compatible     | MCPB host 与 Ora 从 `manifest.json` 得到等价的进程、参数、env 与 user config | 推荐目标               | 纯 HTTP 不可以；当前 MCPB 不理解 HTTP               |
| C3：install-UX-compatible | 文件能被既有 MCPB host 的文件关联/安装入口直接识别                           | `.orax` 后缀通常不保证 | 不保证                                              |

由此得到两个方向：

1. **MCPB → Ora：**已有 `.mcpb` 不能只改扩展名成为有效 `.orax`，因为它缺少必需的 `orax.toml`。必须通过受控 import/pack 步骤添加 Ora identity/policy，重新计算 archive digest，并通过两份 manifest 的交叉校验。
2. **Ora → MCPB：**一个 stdio `.orax` 若包含严格有效的 MCPB `manifest.json`，其 archive 内容可被当前 MCPB CLI pack/unpack 保留；但 `.orax` 文件关联不是 `.mcpb`，不能保证 Claude 或其他 host 的双击安装。若需要分发给 MCPB host，应从相同内容产出/复制正式 `.mcpb` 资产并运行 MCPB CLI compatibility test，而不是口头承诺改后缀即可。
3. **HTTP profile：**它最多天然拥有 C0；纯远程 HTTP 无法满足 MCPB 当前必需的 local `server/entry_point/command`。为了“通过 schema”而伪造 command 或 entrypoint 会制造可安装但不可运行的包，应禁止。若某个 bundled server 确实同时支持 stdio fallback，则 `manifest.json` 可以真实描述 stdio，`orax.toml` 另选 Ora 的 HTTP 模式；此时 C1 成立，但 C2 仅针对 fallback stdio，不代表 MCPB 支持 HTTP。

建议 CI compatibility matrix 至少包括：

```text
fixture package
  → Ora archive/path safety validator
  → Ora orax.toml strict parser
  → 官方 MCPB 0.3 strict schema
  → 官方 mcpb validate
  → 官方 mcpb pack/unpack 后校验 orax.toml 未丢失
  → Ora 再解析并比较规范化 InstalledMcpDescriptor
```

MCPB CLI/source version必须 pin；上游 `main` 与浮动 `latest` 只用于定期兼容性监测，不能直接改变 production acceptance。

### 8.4 两份 manifest 的权威边界

两份文件不是两个都能自由描述启动方式。最小重复和冲突规则建议如下：

| 信息                                                                          | 权威来源                                            | 交叉校验/理由                                                                                               |
| ----------------------------------------------------------------------------- | --------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| Ora canonical ID、namespace、kind、Ora resolver、最低 Ora 版本、Ora 权限意图  | `orax.toml`                                         | MCPB 没有 Ora identity/trust 语义                                                                           |
| MCPB `manifest_version`、server name、展示元数据、author、tools/prompts hints | `manifest.json`                                     | 保持生态内容兼容，不在 TOML 复制                                                                            |
| package version                                                               | `manifest.json`；外层市场 release manifest 必须相等 | 包内只保留一个版本来源；若现有 Ora schema 暂时要求 TOML version，则过渡期必须 exact-equal，随后删除重复字段 |
| stdio runtime、entry point、command、args、env、platform override             | `manifest.json`                                     | C2 的关键；`orax.toml` 禁止重复这些字段                                                                     |
| `user_config`、模板替换和 sensitive 标记                                      | `manifest.json`                                     | Ora 严格解析成自己的 input DTO；不修改原语义                                                                |
| Streamable HTTP endpoint、headers references、本地/远程 ownership、readiness  | `orax.toml`                                         | 当前 MCPB 无对应字段；必须显式标注 `ora-extension`                                                          |
| GitHub release URL、archive SHA-256、publisher ownership                      | 外层 marketplace release manifest                   | 包内不能自证自己的 archive digest或 publisher 权限                                                          |

安装时必须形成一个不可分割的验证事务：外层 `kind == mcp`；外层 Ora ID 等于 `orax.toml` ID；外层 version 等于 `manifest.json.version`；archive SHA-256 等于外层声明；`manifest.json` 通过锁定 MCPB schema；`orax.toml` 通过对应 Ora resolver；所有包内路径 containment 有效。任一冲突都拒绝安装，不能使用“优先读取某一份”静默修复。

`orax.toml` 可以额外保存 `mcpb_manifest_sha256`，把 Ora 扩展语义绑定到包内 JSON 的规范字节；但这只是包内防止两文件被意外错配，archive 自身可信度仍来自市场层摘要/签名。

### 8.5 stdio 与 HTTP 的排他领域模型

“支持 stdio 与 HTTP”不能建模成 `transport` 字符串再搭配一堆 optional 字段。transport 只有两种协议，但 HTTP 又有两个生命周期不同的部署形态：远程 endpoint 不启动进程；bundled local HTTP 必须启动、等待 readiness、连接 loopback 并在 session 结束时回收。建议领域模型：

```text
McpConnection
├── BundledStdio {
│     launch: McpbProcessConfig
│   }
├── RemoteStreamableHttp {
│     endpoint: HttpsUrl,
│     headers: HeaderInputReferences
│   }
└── BundledStreamableHttp {
      launch: McpbProcessConfig,
      endpoint: LoopbackEndpointTemplate,
      port_injection: NamedEnvironmentVariable,
      readiness: ReadinessPolicy,
      shutdown: ShutdownPolicy
    }
```

这三个 variant 恰好排除以下非法状态：remote URL 同时携带 command、stdio 携带 headers、local HTTP 没有进程或 readiness、一个 MCP 同时声明两种 active transport。若以后确需 fallback/multiple modes，应新增显式 `ConnectionAlternatives` 和选择规则，不要把 Issue #20 的未采纳提案偷渡进 v1。

对 HTTP 的额外不变量：

1. `RemoteStreamableHttp.endpoint` 默认必须是 HTTPS；开发/企业例外需要显式受信策略，不能由包绕过。
2. `BundledStreamableHttp.endpoint` 必须是 loopback；禁止 `0.0.0.0`。官方规范要求本地 HTTP Server 只绑定 localhost 并验证 `Origin`，以避免 DNS rebinding。
3. 本地 HTTP 不允许固定公共端口。Ora 分配空闲端口并通过声明的环境变量注入，endpoint 只能引用该受控端口。
4. headers 只能是 plain 值或 `user_config` secret reference；禁止在两个 manifest 中携带真实 token。
5. bundled HTTP 安装成功仍只表示 package/manifest 合法；首次启动的 bind、readiness、initialize 和协议版本协商才决定是否 Healthy。
6. Agent 插件接收 Ora 已解析的排他 DTO，不直接解释 `manifest.json`、TOML、模板或自由 URL。

### 8.6 必须显式决定的 HTTP 兼容范围

**建议：**第一版若优先兑现“基本兼容 MCPB”，应先交付 `BundledStdio` 的 C2 完整闭环，再把 HTTP 作为 Ora-native profile 分开验收。HTTP 中又建议先支持 `RemoteStreamableHttp`，因为它不涉及 Ora 启动本地 daemon、端口竞争、readiness 和回收；但它不是“一键安装本地 MCP 包”，产品 UI 应称为“安装连接描述”或“添加远程 MCP”。

若产品坚持第一版同时支持 bundled local HTTP，则必须把端口分配、readiness、崩溃清理、每个 Agent/session 的进程 ownership 和 loopback 安全纳入范围。只增加一个 URL 字段远远不够。

最终对外表述建议是：

> Ora MCP `.orax` 的 stdio profile 与 MCPB 0.3 内容/schema/启动语义兼容，并额外包含 `orax.toml`；Streamable HTTP 是 Ora 扩展 profile，不属于当前 MCPB 标准。Ora 不支持 deprecated HTTP+SSE。

### 8.7 HTTP 可以复用的官方严格 schema

MCP 核心 specification 的 schema 约束 initialize、tools、resources 等协议消息，并由 Streamable HTTP binding 约束 POST、响应类型、版本 header 和安全行为；它不定义一个可以直接放进插件 archive 的发布清单。因此“HTTP 直接使用严格 MCP schema”需要进一步区分：不能拿 core protocol message schema 代替 package manifest，但可以直接采用官方 MCP Registry 的版本化 `server.json` schema。

Registry `server.json` 已定义 `remotes[]`，其中 `type = "streamable-http"` 的项要求 `url`，并可声明 `headers` 与 `variables`；官方远程 Server 发布文档也使用相同结构。它比 Ora 自造一个相似的 HTTP manifest 更有互操作价值。不过 Registry 当前仍处于 preview，因此 production acceptance 必须 vendor/pin 到明确日期版本（例如 `2025-12-11`），不能在安装时联网获取 draft/latest schema。

这使 `.orax` 最干净的两个 profile 变为：

```text
McpbStdioPackage
  ├── orax.toml
  ├── manifest.json      # strict MCPB 0.3
  └── bundled server assets

RegistryRemoteHttpPackage
  ├── orax.toml
  └── server.json        # pinned official MCP Registry schema
```

若强制 HTTP profile 也保留一个 MCPB `manifest.json`，则它要么缺失必需的本地 server 而不是严格 MCPB，要么伪造 local entry point；这两种都不如直接承认两个官方 descriptor profile。Ora 应把二者解析成统一且排他的 `InstalledMcpDescriptor`，Agent 插件仍只消费 DTO，不感知 JSON 文件差异。

依据：[MCP Registry remote server 文档](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/registry/remote-servers.mdx)、[版本化 server.json schema](https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json)、[Registry draft schema](https://github.com/modelcontextprotocol/registry/blob/main/docs/reference/server-json/draft/server.schema.json)、[Streamable HTTP specification](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http)。

## 9. 已创建对话中的新增、禁用和卸载生命周期

### 9.1 必须分开的三种状态

```text
Desired selection
  “这个对话下一次运行希望使用 A、B”
            │ Agent 插件 reconcile/materialize
            ▼
Materialized agent config
  “工作区的 Claude/Codex 配置已写入 A、B”
            │ Agent session start/reload
            ▼
Running session loaded state
  “当前正在运行的 Agent 实际连接的是 A”
```

**建议：**这三层必须有各自 revision/status。写完 `.mcp.json` 或其他 Agent 配置只能报告 `Materialized`，不能报告 `Loaded`。安装新的 `.orax` 也只改变全局 inventory，不应自动改变任意对话的 desired selection。

MCP 的 `notifications/tools/list_changed` 只表示**一个已经连接的 Server**改变了自己的 tools 列表；它不是“给运行中的 Agent 增加/移除另一个 MCP Server”的通用机制。MCP 连接本身需要 initialize、能力协商和 shutdown，因此 server 集合变化仍由 MCP client/Agent 的生命周期能力决定（[MCP 生命周期规范](https://modelcontextprotocol.io/specification/2025-06-18/basic/lifecycle)、[MCP tools 规范](https://modelcontextprotocol.io/specification/2025-11-25/server/tools)）。

### 9.2 已有对话安装新 MCP 后如何使用

第一版建议流程：

1. 安装成功只将 package 状态置为 `Installed(Unverified)`，不改已有对话。
2. 已创建对话提供“管理 MCP”入口；用户把新 MCP 加入该对话的 desired selection，产生新 revision。
3. 若该对话没有运行中的 Agent session，下一次启动前由 Agent 插件物化完整 desired set。
4. 若 session 正在运行，第一版显示 `Pending agent restart`。用户选择“重新加载 Agent”后，Ora 停止/重建或恢复 Agent session，再在 `session/new`/resume 前物化；若目标 Agent 插件没有安全的恢复能力，则明确提示“仅新对话可用”，不能假装当前对话已经加载。
5. 只有新 session 启动且 Agent 插件确认连接后，loaded revision 才追上 desired revision。

这使 Q31 的答案变成：**创建对话时选择 MCP，同时把选择保存为该对话的期望快照；已创建对话可以修改快照，但 v1 通过 Agent session 重启/重建生效，不承诺无重启热插拔。**它不要求当前 MCP 包定义任务实现项目级关联表，但对话集成任务仍需要持久化 selection revision，不能只把选择留在 React 内存中。

### 9.3 为什么不能假定所有 Agent 都能热重载

**官方事实：**VS Code 在新增或修改 MCP 配置后需要重新启动 MCP server 才能发现工具；它提供 start/stop/restart 操作，配置变更自动 restart 仍是 experimental（[VS Code 官方 MCP 管理文档](https://code.visualstudio.com/docs/agent-customization/mcp-servers)、[VS Code MCP 配置参考](https://code.visualstudio.com/docs/agents/reference/mcp-configuration)）。其 enable/disable 状态与 `mcp.json` 分开保存，也支持 workspace-specific disable。

**官方事实：**Claude Code 对“Claude plugin 自带 MCP”的场景支持在当前 session 执行 `/reload-plugins` 来连接或断开，配置未变化的连接会被保留；但这是 Claude Code 自己的 plugin lifecycle，不等于所有手写 `.mcp.json`、所有 Agent 或 Ora 的 Agent 插件自动具备同样能力（[Claude Code 官方 MCP 文档](https://code.claude.com/docs/en/mcp)）。Claude 也支持 `/mcp` 查看状态和 reconnect，说明连接状态仍独立于磁盘配置。

**推断：**Ora 的外部 Agent 插件协议应该报告 `AppliesOnNextSession | RestartRequired | Reloaded`，而不是由 Ora 根据 Agent 名称猜测。已选择的第一版策略是只依赖 `AppliesOnNextSession/RestartRequired`。

### 9.4 禁用与卸载的安全顺序

“从对话移除 MCP”和“从机器卸载包”不是同一个操作：

- 对话移除：更新 desired selection；Agent 插件删除自己管理的配置项；运行 session 重启后才不再 loaded。
- 全局卸载：先阻止任何新 selection/session 使用，再清理所有已知 materialization，等待运行引用释放，最后删除 package。

建议的卸载顺序，即使第一版暂不实现也要预留状态：

```text
Installed
  │ uninstall requested
  ▼
PendingRemoval           # 立即从可选择 inventory 隐藏，阻止新 session
  │ remove from desired selections / reconcile agent configs
  ▼
Draining                 # 已运行 session 仍可能持有该 executable
  │ active usage leases = 0
  ▼
Removing                 # 原子/可恢复地删除 package 和安装记录
  ▼
Absent
```

不能先删除 binary 再清 Agent 配置：新 Agent 可能读到指向不存在入口的配置；Windows 也可能因运行进程持有文件而删除失败。若工作区暂时不可访问，保存 cleanup tombstone，下一次打开并在启动 Agent 前调用 adapter 删除 managed entry。安全撤销/恶意版本属于例外：可以强制终止受影响 session 后立即清理，但这是单独的高优先级策略。

当前 `PluginLifecycle::uninstall_plugin` 只会停止它自己拥有的 Ora plugin runtime，然后删除 package 和全局 durable state（[`crates/plugin-lifecycle/src/lib.rs`](../../crates/plugin-lifecycle/src/lib.rs#L434-L507)）。而且它先删除 durable state、再递归删除目录；文件删除失败时状态已经消失，并不是文档规划的事务回滚。MCP 进程由 Agent 拥有、配置又由 Agent 插件写入，因此现有卸载逻辑既无法知道 active MCP connection 或项目残留，也不满足上述 staged removal，不能原样复用。

## 10. 第一版状态机与接口建议

### 10.1 当前 MCP 插件定义/安装任务负责

```text
PackageInstallState
├── Absent
├── Staging { source, expected_digest }
├── Installed { id, version, digest, verification: Unverified }
├── PendingRemoval { reason }
└── Failed { phase, reason }       # staging 失败不得破坏旧 Installed 版本
```

当前任务应交付：

1. Ora market release manifest 的 `mcp` kind、平台资产选择与 `.orax` SHA-256 校验。
2. 包内 Ora `orax.toml` schema、严格 parser、kind-specific validator 和安全解压。
3. MCPB 0.3 manifest/变量/platform override 到 Ora DTO 的严格适配，以及 `BundledStdio | RemoteStreamableHttp | BundledStreamableHttp` 的排他领域模型。
4. 安装器输出 `InstalledMcpDescriptor`，包含 canonical ID、精确版本、排他的 connection variant、已验证的 process 或 endpoint、input declarations、permission intent 和适用时的 package root；绝不包含 secret value。remote HTTP variant 不得伪造 executable，stdio variant 不得携带 URL/headers。
5. inventory 的 `Installed/PendingRemoval` 可用性状态，以及为未来 session usage lease 保留稳定 package identity/version。

### 10.2 外部 Agent 插件/对话任务负责

建议协议轮廓：

```text
prepareSessionMcps {
  conversation_id,
  workspace_root,
  desired_revision,
  desired: [InstalledMcpDescriptor]
}

=> MaterializationReceipt {
  desired_revision,
  agent_plugin_id,
  workspace_root,
  managed_entry_ids,
  effect: AppliesOnNextSession | RestartRequired | Reloaded
}
```

为删除/卸载预留：

```text
reconcileConversationMcps {
  conversation_id,
  workspace_root,
  desired_revision,
  desired: [] | [...]
}

releaseSessionMcpUsage {
  conversation_id,
  loaded_revision
}
```

外部任务的必要不变量：

1. Agent 插件拥有 Agent-specific 配置语义并直接写文件，但只获得权威 workspace 边界内的受限写权限；用户级写入按已选 Q15.C 需要显式授权。
2. reconcile 接收**完整期望集合**并且幂等，不依赖可能丢失的 enable/disable 事件序列。
3. Agent 插件只能更新带稳定 Ora managed identity 的条目，不能覆盖同名的用户自建 MCP；`MaterializationReceipt` 为卸载清理提供反向索引。
4. `Loaded` 必须由 Agent session/connection 事实驱动；`Materialized` 不能冒充 `Loaded`。
5. 运行中 MCP 版本应形成 usage lease。升级或卸载不得删除仍被 session 使用的 package version，除非用户选择强制终止。
6. secret input 只转成目标 Agent 的安全 placeholder 或系统环境变量引用；无法安全表达时返回 `SecretInputUnsupported`。

### 10.3 第一版最小可实现闭环

```text
安装 .orax
  → Installed(Unverified)
创建/打开对话并选择 MCP
  → 保存 conversation desired revision
启动 Agent 前
  → agent plugin reconcile + MaterializationReceipt
启动/恢复 Agent
  → loaded revision（成功）或 configuration/start failure
对话修改选择
  → PendingRestart
重新加载 Agent
  → reconcile 新 revision → loaded
```

这个闭环回答了“安装新 MCP 后，已有对话怎么用”：修改该对话的 desired selection，再重启/重建 Agent session。也回答了未来卸载：先将 package 置为不可新用，reconcile 删除 managed config，等 loaded usage 清零后再物理删除。当前 MCP 包开发只需要把身份、版本、路径、输入和权限定义得足够稳定；对话选择、Agent 配置写入、session restart 和 usage lease 都明确属于外部方案。

## 11. 修订：安装期编译，Session 期物化

新的生命周期决定是：Ora 在安装 `.orax` 时完成 schema 校验、交叉校验、模板/输入声明解析和规范化持久化；创建 Session 时再让 Agent 插件把 Ora 的规范描述转换为目标 Agent 的项目配置。这个方向避免每次 Session 都重新解释不可信 archive，也让安装失败在写入任何 Agent 项目配置之前暴露。

建议把流程精确定义为：

```text
Install-time compile
  archive + market metadata
    -> validate pinned official schemas
    -> cross-check identity/version/digests/paths
    -> normalize to immutable InstalledMcpDescriptor
    -> persist descriptor + package inventory transactionally

Session-time materialize
  session desired MCP IDs/versions + authoritative workspace
    -> Ora queries its repository
    -> Ora resolves install-root/platform/input references
    -> Ora passes typed SessionMcpDescriptor[] to Agent plugin
    -> Agent plugin renders target-Agent config and writes only its managed entries
```

关键 seam 是 Ora host 与 Agent 插件的 typed interface，而不是 SQLite。Agent 插件若直接打开 Ora 数据库，会把表名、列、migration 顺序和 JSON 存储形状变成插件协议；还需要授予任意插件读取整个应用数据库的文件权限，难以限制它只能读取当前 Session 选择的 MCP。更深且更安全的模块应让数据库留在 Ora repository implementation 内，Agent 插件只学习稳定 DTO。若交互形式必须由插件“获取”，也应调用受限 host capability（按当前 Session 授权并返回 DTO），绝不能接收数据库路径或执行 SQL。

安装期可以保存三类信息，但不能混成一份“最终 Agent 配置”：

1. **Package inventory：**canonical ID、exact version、archive/manifest digest、安装根、descriptor profile、安装状态。
2. **Compiled descriptor：**排他的 `BundledStdio | RemoteStreamableHttp`、已校验的包内相对路径、command/args/env/header 模板、input declarations、permission intent；按 package version 不可变。
3. **Input bindings：**用户实际填写的非敏感值或 secret reference，必须与 descriptor 分开并有明确作用域。真实 secret 不得进入普通 SQLite、descriptor JSON、项目文件或日志。

安装期只能完全解析 package-intrinsic 信息。authoritative workspace、项目路径、Session 选择、目标 Agent 占位符和 Agent 配置文件位置在 Session 创建前不存在，因此必须延迟到 materialization。`${__dirname}` 可以在 Ora 读取已安装版本后解析；workspace/input/secret 引用必须在已知 Session 和 Agent 能力时解析或转换。安装成功仍然只证明静态合法，不证明远程 HTTP endpoint 可达或本地 Server 可以成功 initialize；沿用此前决定，liveness 属于首次使用/显式检查。

版本记录应不可变：升级插入新 `(mcp_id, version)` descriptor，不覆盖运行 Session 正在引用的旧记录。Session materialization 应明确使用的 exact version，并返回 receipt；这给未来 restart、rollback、draining uninstall 和 usage lease 留下稳定依据。

### 11.1 已确认的持久化与调用 seam

本轮选择已经确认：

- Session 创建时由 Ora 查询 repository，并把完整 typed DTO 推送给 Agent 插件；插件不能直接访问 SQLite，也不需要反向调用 host 查询。
- 安装时保存规范化 `InstalledMcpDescriptor`；archive 内继续保留原始 `manifest.json`/`server.json` 作为可审计来源，Session 不重复解析原始清单。
- 安装流程可以收集 global input；缺少必填值时包进入 `InstalledNeedsInput`，而不是伪装 Ready 或直接丢弃一个结构合法的安装。
- input binding 支持 global default 与 project/workspace override；真实 secret 进入 OS credential store，SQLite 只保存 opaque reference。
- 每个 `(mcp_id, version)` descriptor 不可变；升级插入新版本，Session/materialization 引用 exact version。

这不要求另建数据库：应通过 Ora 现有 `ora.sqlite3` migration 增加 MCP 专用 repository/table。当前 `plugin_state` 只表达全局 enablement，`user_config` 只表达非敏感全局偏好，二者都不能承载 descriptor、workspace override 或 secret value。

### 11.2 当前 Workspace 事实带来的下一项约束

Ora 当前一个 Session 的 Task 是不可变的，因为 Task 决定 Agent 的 authoritative cwd；同一 Task 可以拥有多个 Session。Task 可能对应独立 Git worktree，也可能使用 project root，而 warm chat 还可以直接以 project root 为 target。Agent 配置若写入 cwd 下的 `.claude`/其他共享文件，作用域实际上是 **workspace target/cwd**，不是单个 Session。

因此，若两个 Session 共享同一 Task/cwd 却选择不同 MCP 集合，二者会竞争同一配置文件。单纯在数据库中把 selection 存成 per-Session 并不能隔离磁盘状态。v1 要么把 desired MCP set 定义在 workspace target 上，并让 Session 只快照该 revision；要么要求 Agent adapter 提供真实的 per-Session overlay/独立配置入口。不能让“最后创建的 Session 覆盖配置文件”成为隐含语义。

### 11.3 已确认的 Workspace 与 materialization 语义

MCP desired selection 的作用域已经确定为配置目的地，而不是 Session：

```text
McpWorkspaceScope
├── ProjectRoot { project_id }
└── Worktree { worktree_id }
```

同一项目根目录下的对话共享一份 desired MCP revision；同一 worktree 内的对话共享另一份；不同 worktree 彼此隔离。input binding 采用相同 scope，解析优先级为 package non-secret default < global binding < workspace binding。这样既不以可移动的绝对路径作为身份，也不会让两个指向同一 cwd 的 Session 互相覆盖。

Ora 每次向 Agent 插件传完整 desired set 与 revision，Agent adapter 必须做幂等 reconcile，并且只修改带稳定 Ora identity 的 managed entries，保留用户自建条目。Ora 负责 MCPB/Registry 模板、平台、install root 和 authoritative workspace 解析；Agent adapter 只负责目标 Agent 的配置形状、转义和安全变量引用。缺少 required input/secret 在调用 Agent adapter 之前返回 `McpConfigurationRequired`。

已选择允许单个 MCP materialization 失败后用成功子集启动 Session。这个策略必须产生显式的 `Degraded` 状态，而不能把 desired revision 冒充为完全 applied。建议 receipt 分别携带 `applied` 与 `failed`：

```text
MaterializationReceipt {
  desired_revision,
  applied: [{ mcp_id, version, managed_entry_id }],
  failed: [{ mcp_id, version, phase, reason }],
  configuration_fingerprint,
  effect
}
```

只有 item-local 错误（例如目标 Agent 不支持该 transport、单项安全引用无法表达）可以降级；无法读取/解析现有目标配置、无法原子写入配置文件或 receipt/fingerprint 不可信是 workspace-global 错误，必须阻止 Session 启动。adapter 应先为每个 item 生成 plan，再一次性合并并原子替换成功集合，避免逐项写入留下半更新文件。

### 11.4 Degraded、跨 Agent 与持久化 receipt

Workspace desired MCP set 已确认跨 Agent 共享；每个 Agent adapter 独立产生 applied/failed 结果。item-local 错误允许 Session 以显式 `Degraded` 状态启动，持续显示失败项并提供修复与 reload；workspace/config-file/revision 等 global 错误必须 `Blocked`。新 revision 的 item 失败时删除该 item 的旧 Ora-managed entry，不允许把旧版本静默当作新 desired version 运行。

更正后的决定是把 materialization receipt 持久化到 Ora SQLite。receipt 的 identity 是 `(workspace_scope, agent_plugin_id)`，不是 Session：同一项目根/worktree 中使用同一 Agent 的多个 Session 共享同一份目标配置和 receipt。Session 可以记录自己加载的 receipt revision，但不能创建另一份竞争的 workspace 配置状态。

建议的 current-state 表：

```text
agent_mcp_materializations
├── scope_kind
├── scope_id
├── agent_plugin_id
├── agent_plugin_version
├── desired_revision
├── applied_descriptor
├── failed_descriptor
├── managed_entries
├── configuration_fingerprint
├── status
└── materialized_at
```

receipt 是最近一次可证实的 materialization observation，不替代 desired selection。desired revision 改变、Agent 插件版本/receipt schema 改变、配置 fingerprint 漂移或 workspace 暂时不可验证时，receipt 必须转为 `Outdated | Drifted | Unknown` 并在启动 Agent 前 reconcile。

文件系统与 SQLite 无法共享一个原子事务，因此顺序必须是：Agent adapter 先规划并原子替换配置文件，再返回 receipt，Ora 最后提交数据库。若进程在文件写入后、数据库提交前崩溃，旧 receipt 保持 stale；下次通过读取配置、确定性 managed identity 与 fingerprint reconcile 恢复。绝不能在文件写入成功前持久化“已应用”。

持久 receipt 仍不得成为跳过磁盘验证的理由：用户或 Git 可能修改配置文件。它提供跨重启的 applied/failed 状态、UI 诊断、managed-entry 反向索引和卸载/清理依据；Agent adapter 仍需幂等，并能从配置文件重新识别稳定 Ora-managed entries。receipt 只保存 MCP identity/version、错误结构、managed identity 和 fingerprint，禁止保存真实 secret 或解析后的 secret value。

已确认 v1 只保留每个 `(workspace_scope, agent_plugin_id)` 的 current receipt，不建立永久 materialization 历史。数据库使用父 `agent_mcp_materializations` 记录 revision/fingerprint/status，并用逐 MCP 子表记录 `Applied | Failed`、exact version、managed identity 和结构化错误，避免一个不可查询的 JSON blob。Session 另外记录自己实际加载的 desired/materialization revision 和 applied exact versions。

每次 Session create/load/resume 前仍调用 Agent adapter 的轻量 inspect/reconcile；fingerprint 与规范化配置一致时返回 `AlreadyMaterialized`，不重写文件。相同 `(workspace_scope, agent_plugin_id)` 的配置读取、plan、原子替换和 receipt 数据库提交在一个应用级串行临界区内，避免并发 Session 产生“文件来自 A、receipt 来自 B”的丢失更新。永久 receipt 不能让文件系统与 SQLite 共享事务：顺序仍是先原子替换配置、adapter 返回 receipt，再提交数据库；两者之间崩溃由下一次 reconcile 修复。

### 11.5 已确认的版本与配置 revision 语义

Workspace selection 默认使用 `FollowActive` 版本策略，并为未来显式 `Pinned(exact_version)` 留出排他 variant。每次 materialization 把策略解析为 exact version；receipt 与运行 Session 永远记录实际使用的 exact version。安装的新版本先进入 `Available`，v1 需要用户显式激活，不能因为静态校验通过就自动影响所有 Workspace。

激活新版本会推进所有选择该 MCP 且使用 `FollowActive` 的 Workspace desired revision，把相关 current receipt 标记为 `Outdated`。已运行 Session 继续持有旧 exact version 和 usage lease，并显示 `PendingRestart`；重新加载后才 reconcile 新版本。新版本 item-local materialization 失败进入 `Degraded`，不静默使用旧版本；回滚是用户显式选择 active version 的另一次 revision 变化。

旧版本只有在不再是 active、没有 pinned Workspace、没有 applied receipt、没有 running Session usage lease 且已超过 rollback retention 时才可回收。修改 global/workspace input 或 secret binding 同样推进受影响 Workspace configuration revision、使 receipt `Outdated`，并让已运行 Session `PendingRestart`，因为现有 MCP 进程可能仍持有旧环境或认证状态。

## 12. 安装事务、布局与 runtime resolution

已确认安装目录使用 `<app-data>/plugins/installed/<namespace>/<name>/<version>/`，canonical ID 在 Agent/MCP kind 之间全局唯一。相同 ID/version/digest 的重复安装幂等成功；相同 ID/version 但 digest 不同是 immutable-version conflict。安装在同文件系统 staging 中完成 digest、安全解压、schema/交叉校验和 descriptor 编译，再原子提升到不可变版本目录并提交 SQLite；启动 reconciliation 处理 staging、orphan directory 和 missing artifact。

Archive 必须严格拒绝 absolute/drive/UNC path、traversal、symlink/junction/reparse point、特殊文件、规范化重复路径及数量/单文件/总展开大小超限，并优先复用或扩展 `ora-utils::path` 与 `ora-utils::archive`。结构合法但缺 required input/runtime 的版本可以 Installed，状态为 `NeedsInput | NeedsRuntime`，但不能 Active/materialize。

### 12.1 Ora resolver 与 Agent adapter 不是重复职责

“Agent 插件负责 MCP 配置”指的是它拥有目标 Agent 配置格式和安全写入语义；不意味着每个 Agent 插件都要重新实现 MCPB platform override、变量模板、包路径 containment、runtime 搜索和版本要求。建议 seam：

```text
Ora package/runtime resolution
  portable InstalledMcpDescriptor
    -> apply current platform override
    -> resolve validated package-relative paths
    -> locate/check Node/Python/bundled binary runtime
    -> produce ResolvedMcpForAgent

Agent adapter materialization
  ResolvedMcpForAgent
    -> map to target Agent field names/placeholders
    -> merge only Ora-managed entries
    -> atomically write target config
    -> return receipt
```

这里的 resolve 不读取 executable 内部、不执行安装脚本、不启动 Server，也不证明 initialize 可用。对 stdio 它只把例如 `command = "node"`、`${__dirname}/server/index.js` 和 platform override 转成已经过存在性、版本和 containment 校验的具体 executable/script/args 描述；对 bundled binary 选择对应平台文件并处理 Windows executable 规则；对 HTTP 不需要 runtime resolution。实际 MCP 进程仍由目标 Agent 按其配置启动和连接。

由 Ora 集中 resolution 的理由是所有 Agent 获得相同的 MCPB 0.3 语义、安全校验和 runtime 选择；传递 absolute validated executable/path 也避免 Ora 与 Agent 进程 PATH 不一致。Agent adapter 若无法表达该 resolved transport/secret reference，返回 item-local unsupported failure，按已选策略进入 Degraded，而不是自行换一个 runtime 或解释模板。

职责划分已确认：Ora compile/resolve，Agent adapter render/merge/write。v1 支持 MCPB 0.3 的 `node | python | binary`，但不执行依赖安装；包作者必须按 MCPB 语义提供依赖内容。runtime resolution 顺序为 Ora bundled runtime、用户显式配置、受控 system PATH，输出 absolute executable。若 runtime 消失或版本不再满足要求，安装版本转为 `NeedsRuntime`、相关 receipt `Outdated`，阻止新的 materialization；已运行 Session 不被强制终止。

## 13. 最终 `.orax` descriptor profile 与权威性

包内 `orax.toml` 保持 Ora-specific 的薄清单，不重复 descriptor 已拥有的 version、stdio command/args/env、HTTP URL/headers 或 input declarations。建议稳定形状：

```toml
schema_version = 1
id = "official/github-mcp"
kind = "mcp"
requires_ora = ">=1.0.0"

[mcp]
profile = "mcpb-stdio"
descriptor = "manifest.json"
descriptor_schema = "mcpb:0.3"
```

或：

```toml
schema_version = 1
id = "official/github-remote"
kind = "mcp"
requires_ora = ">=1.0.0"

[mcp]
profile = "registry-remote"
descriptor = "server.json"
descriptor_schema = "mcp-registry:2025-12-11"
```

Ora ID 与 upstream descriptor name 是不同 identity：市场 Ora ID 必须等于 `orax.toml.id`；市场 release version 必须等于 `manifest.json/server.json.version`；MCPB name 或 Registry reverse-DNS name 作为 upstream identity 保留，不强制伪装成 Ora ID。包内 `orax.toml` 不保存 version，避免第三个重复来源。

HTTP v1 对严格 Registry schema 增加 Ora profile constraints：必须恰好一个 `streamable-http` remote，拒绝 deprecated SSE、multiple remotes 和 packages+remotes fallback。认证支持 none、header/secret variables，以及目标 Agent 明确声明时的 Agent-managed OAuth；Ora v1 不拥有 OAuth client/token refresh。endpoint 默认 HTTPS，只在 developer/explicit-local-trust 下允许 localhost、127.0.0.1 或 `::1` 的 HTTP。

`descriptor_schema` 解析为内置排他枚举 `McpbV0_3 | McpRegistry2025_12_11`，文件内 `manifest_version/$schema` 必须一致；安装不联网下载 validator。市场 release metadata 与包内 `orax.toml` 是不同 schema：前者拥有 GitHub asset URL、archive digest、platform/publisher，后者拥有 Ora identity/kind/host compatibility/profile/permission intent。两者交叉校验但市场 entry 不打入 archive，也不能用包内文件自证 archive digest。
