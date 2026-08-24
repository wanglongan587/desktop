# Ora 插件系统全景研究：为 MCP 插件设计做准备

> 本文档综合 6 路并行研究 agent 的发现，交叉验证、去重并解决矛盾后写成。所有代码标识符、路径、`file:line` 引用、trait/type/fn 名称保留英文原文。每一项断言均标注 `path:line` 引用。

---

## 1. 概述与 crate/package 地图

Ora 是一个面向 AI Agent 的 IDE。其插件系统由五个 Rust crate、一个 TypeScript SDK 包、以及一组设计规格文档构成。插件系统的核心边界是：插件以 `orax.toml` 清单描述（`crates/plugin-manifest/src/manifest.rs:13-27`），通过 git 同步的 marketplace 仓库发现（`crates/plugin-registry/src/source.rs:9-14`），以 SHA-256 校验的 `.orax` 压缩包安装（`crates/plugin-manager/src/install.rs:64-86`），由 Deno 子进程承载运行（`crates/plugin-runtime/src/lib.rs:100-211`），并在后端通过生命周期编排管理启停与持久化状态（`crates/plugin-lifecycle/src/lib.rs`）。

**重要纠正**：系统不使用 `package.json`。任务简报中关于 `ora.kind`/`ora.contributes` 存在于 `package.json` 的假设与实际不符——插件清单是 TOML 格式的 `orax.toml`，由独立的 `ora-plugin-manifest` crate 解析（`crates/plugin-manager/src/validation.rs:20`、`:29` 中的 `ora.kind` 仅为文档注释描述概念字段，而非 JSON 解析）。

### 1.1 Rust workspace 布局

顶层 `Cargo.toml` 声明 23 个 workspace member（`Cargo.toml:2-27`），其中 `default-members` 排除 `apps/desktop/src-tauri`（`Cargo.toml:28-51`），故裸 `cargo build`/`cargo test` 仅编译 crate。Edition 2024（`Cargo.toml:60`），共享版本 `0.0.0`（`Cargo.toml:55`），共享 lints 在 `[workspace.lints.clippy]`（`Cargo.toml:133-171`）。

插件系统涉及的全部 Rust crate：

| Crate                  | 职责                                                         | 关键文件                                                          |
| ---------------------- | ------------------------------------------------------------ | ----------------------------------------------------------------- |
| `ora-plugin-manifest`  | 解析与校验 `orax.toml`，提供只读的 `PluginManifest` 值类型   | `crates/plugin-manifest/src/manifest.rs`                          |
| `ora-plugin-registry`  | git 同步 marketplace 源、扫描构建索引、按需解析安装清单      | `crates/plugin-registry/src/{lib,index,source,entry}.rs`          |
| `ora-plugin-manager`   | 发现已安装插件、校验贡献类型、编排下载安装                   | `crates/plugin-manager/src/{lib,discovery,validation,install}.rs` |
| `ora-plugin-runtime`   | 启动 Deno 子进程、二进制帧编解码、JSON-RPC 握手与流量        | `crates/plugin-runtime/src/{lib,codec,protocol,tasks}.rs`         |
| `ora-plugin-lifecycle` | 后端编排：持久化启用状态、扫描对账、运行时状态机、操作串行化 | `crates/plugin-lifecycle/src/{lib,scan,state,runtime}.rs`         |

此外，后端 `ora-backend` 中的 `agent_runtime/plugin_agent/` 模块（`crates/backend/src/agent_runtime/plugin_agent/`）是 agent 插件的完整桥接层。桌面应用 `apps/desktop/src-tauri/src/lib.rs` 负责引导发现并将插件包喂给后端。

### 1.2 TypeScript SDK 包

`packages/plugin-sdk` 是 JSR 包 `@ora-space/plugin-sdk`，版本 `0.1.3`，许可证 Apache-2.0（`packages/plugin-sdk/deno.json:2-5`）。唯一导出入口为 `./src/mod.ts`（`packages/plugin-sdk/deno.json:6-8`）。SDK 导出三类符号：来自 `agent.ts` 的 agent 契约辅助函数、来自 `plugin.ts` 的底层 `Plugin` 类与原语、来自 `protocol.ts` 的 `JsonValue` 类型（`packages/plugin-sdk/src/mod.ts:1-16`）。

### 1.3 规格文档

设计规格位于 `specs/plugin/`，含五个文件：`0-overview.md`（总览）、`1-capability.md`（能力与权限）、`2-settings.md`（设置）、`3-registry.md`（注册表与安装）、`4-agent.md`（agent 协议）。此外仓库根目录有 `plugin-agent-runtime.md`（agent 插件桥接设计文档）。注意：部分规格描述的是**目标设计**而非已实现状态——本文在每个相关处标注规格与实现的差异。

---

## 2. 插件身份与清单（Plugin Identity & Manifest）

### 2.1 `PluginManifest` 结构与字段

`ora-plugin-manifest` crate 的核心类型 `PluginManifest` 持有以下字段（`crates/plugin-manifest/src/manifest.rs:13-27`）：

- `resolver: u64` — 清单版本号；`SUPPORTED_RESOLVER = 1`（`manifest.rs:10`），任何其他值产生 `ManifestError::UnsupportedResolver`（`manifest.rs:64-66`）
- `name: PluginName` — 插件标识符
- `namespace: PluginNamespace` — 命名空间，当前闭合枚举仅 `Official`（`crates/plugin-manifest/src/enums.rs:6-8`）
- `kind: PluginKind` — 插件类型，闭合枚举 `Workbench` 或 `Agent`（`crates/plugin-manifest/src/enums.rs:49-52`）
- `version: semver::Version` — 完整 SemVer，含预发布与构建元数据
- `description: String` — 最多 1000 字节 UTF-8，不允许前后空白和控制字符（`manifest.rs:356-388`）
- `homepage: Option<HomepageUrl>` — HTTPS，无 query，无 fragment（`crates/plugin-manifest/src/urls.rs:56-58`）
- `license: Option<String>` — 最多 256 字节 ASCII，不校验为 SPDX（`manifest.rs:169-172`）
- `url: Option<ReleaseUrl>` — HTTPS 下载地址，**允许** query（用于签名参数）（`urls.rs:19-21`）
- `sha256: Option<Sha256Digest>` — 64 位十六进制字符解码为 `[u8;32]`（`crates/plugin-manifest/src/sha256.rs:12-34`）
- `head: Option<PluginHead>` — 源码仓库元数据，用于 `--head` 安装（`manifest.rs:204-208`）
- `dependencies: Option<PluginDependencies>` — 仅接受 `ora` 键作为 `VersionReq`（`manifest.rs:237-240`）

README 明确：唯一构造入口为 `PluginManifest::parse(&str)`，所有字段私有，仅通过只读 accessor 暴露（`crates/plugin-manifest/README.md:26-28`）。

### 2.2 两个解析入口：release 形态 vs installed 形态

- `PluginManifest::parse(source)`（`manifest.rs:34`）——marketplace/release 形态，使用 `RawPluginManifest`（`manifest.rs:249-264`），其中 `resolver`/`url`/`sha256` 为**必需**
- `PluginManifest::parse_installed(source)`（`manifest.rs:46`）——包内 `orax.toml` 的 installed 形态，使用 `RawInstalledManifest`（`manifest.rs:266-281`），其中 `resolver` 为 `Option<u64>`（缺失默认 `SUPPORTED_RESOLVER = 1`，`manifest.rs:52`），`url`/`sha256` 为 `Option<String>`

两个 raw 结构体均使用 `#[serde(deny_unknown_fields)]`（`manifest.rs:250`、`manifest.rs:267`），故**任何未知 TOML 键（包括 `[permissions]`）在解析阶段即被拒绝**。校验顺序确定：`resolver` → `name` → `namespace` → `kind` → `version` → `description` → `homepage` → `license` → `url` → `sha256` → `head` → `dependencies`（`manifest.rs:64-116`）。

### 2.3 `PluginName` 校验规则

`PluginName(String)` 由一或两个点分隔的 slug 段构成（`crates/plugin-manifest/src/name.rs:9-44`）。`MAX_PLUGIN_NAME_BYTES = 128`（`name.rs:5`），`MAX_PLUGIN_NAME_SEGMENTS = 2`（`name.rs:6`）。超过两段产生 `PluginNameError::TooManySegments`（`name.rs:23-28`）。每段通过 `ora_utils::Slug::parse` 校验（`name.rs:30-35`）。

`Slug` 规则（`crates/utils/src/slug.rs:12-39`，`MAX_SLUG_BYTES = 63`）：非空、≤63 字节、无前导/尾随连字符、无连续连字符、仅 `[a-z0-9-]`（ASCII 小写+数字+连字符）。大写、下划线、非 ASCII 均被拒绝。

### 2.4 `PluginNamespace` 与 `PluginKind`：闭合枚举

`PluginNamespace` 当前仅有 `Official`（`crates/plugin-manifest/src/enums.rs:6-8`）。`FromStr` 仅接受字面量 `"official"`，其他值产生 `PluginNamespaceError::Unsupported`（`enums.rs:30-37`）。**没有 `community`/`third-party` 变体**。

`PluginKind` 有且仅有 `Workbench` 和 `Agent`（`crates/plugin-manifest/src/enums.rs:49-52`）。`FromStr` 接受 `"workbench"` 和 `"agent"`（`enums.rs:71-84`）。**没有 `Mcp` 变体**。`as_str()` 回转为 `"workbench"`/`"agent"`（`enums.rs:55-62`）。

### 2.5 `Sha256Digest`：清单 crate 不计算摘要

`Sha256Digest([u8; 32])` 持有**已解码**的摘要（`crates/plugin-manifest/src/sha256.rs:12-34`）。README 明确列出非职责："No network access, download, repository probing, or release checksum calculation"（`crates/plugin-manifest/README.md:20`）。摘要作为字段携带，由 **installer** 在别处验证。规范确认该字段的用途：校验 `.orax` 文件哈希（`specs/plugin/3-registry.md:100`）。

### 2.6 URL 字段语义

三个 HTTPS newtype 共享 `parse_https_url`（`crates/plugin-manifest/src/urls.rs:145-168`），`MAX_URL_BYTES = 2048`（`urls.rs:5`）：scheme 必须为 `https`（`urls.rs:154-156`），无用户名/密码（`urls.rs:157-159`），无 fragment（`urls.rs:160-162`）。query 策略因类型而异：`ReleaseUrl` 允许 query（`QueryPolicy::Allow`，`urls.rs:19-21`）；`HomepageUrl` 拒绝 query（`urls.rs:56-58`）；`RepositoryUrl` 拒绝 query 且不施加 host 或 `.git` 后缀限制（`urls.rs:92-95`）。

---

## 3. 注册表与 Marketplace（Registry & Marketplace）

### 3.1 registry 的本质

注册表是一个**派生的轻量 JSON 索引**，从一个 **git 同步的 marketplace 仓库检出版** 构建。它不是数据库，不是远程 API——而是一个本地文件 `registry_index.json`，缓存仅用于显示的元数据，使 UI 无需重新扫描成千上万个 `orax.toml` 文件（`crates/plugin-registry/README.md:8-16`）。

存在两个物理上独立的存储：

1. **源检出**——`<data_dir>/plugins/sources/github.com/ora-space/marketplace`，`https://github.com/ora-space/marketplace` 的真实 git 克隆（`crates/backend/src/plugin.rs:27-63`）
2. **缓存索引**——`<data_dir>/plugins/cache/registry_index.json`（`crates/backend/src/plugin.rs:64`）

源检出持有完整的 `orax.toml` 清单（含 `url`/`sha256`）；缓存索引故意省略这些下载字段——安装时重新读取源清单获取它们。

### 3.2 `RegistryEntry` 与 `RegistryIndex`

`RegistryEntry`（`crates/plugin-registry/src/entry.rs:9-16`）仅持有五个显示字段：`id`（`"namespace/name"`）、`name`、`namespace`、`version`、`description`。`id` 通过 `format!("{}/{}", manifest.namespace(), manifest.name())` 合成（`entry.rs:21`）。条目**仅**通过 `pub(crate) from_manifest` 从已校验的 `PluginManifest` 构造（`entry.rs:20-29`），无公开构造函数。

`RegistryIndex`（`crates/plugin-registry/src/index.rs:18-23`）持有 `updated_at: i64`（由调用者注入的 Unix 秒）、`version: String`（恒为 `"1.0"`，`INDEX_VERSION`，`index.rs:11`）、`plugins: Vec<RegistryEntry>`。

**规格与实现差异**：规格（`specs/plugin/3-registry.md:139-154`）描述索引应携带 `release` 字段（`{ "kind": "universal" }` 或 `{ "kind": "targets", "targets": [...] }`）。已实现的 `RegistryEntry` **没有 `release` 字段**（`entry.rs:9-16`）。

### 3.3 索引构建与持久化

`RegistryIndex::build(dir, updated_at)`（`index.rs:31`）递归扫描 `dir` 寻找名为 `orax.toml` 的文件（`collect_orax_manifests`，`index.rs:158`，**不跟随符号链接**，静默跳过缺失根）。每个有效清单 → `RegistryEntry::from_manifest`；每个失败 → `SkippedManifest` 并通过 `ora_warn!` 记录（`index.rs:38`）。条目按 `id()` 排序确保确定性（`index.rs:46`）。扫描是**单线程**的（`index.rs:150-173`），使用 `std::fs::read_dir` 递归——规格（`3-registry.md:162-163`）要求 `rayon`/`ignore::WalkBuilder` 并行解析，**未实现**。

`resolve_manifest(registry_dir, id)`（`index.rs:63-80`）是安装时的配套方法：重新读取源 `registry/` 目录，解析每个 `orax.toml`，返回合成 `id` 匹配的第一个 `PluginManifest`。这是安装获取缓存索引故意省略的 release `url`/`sha256` 的途径。

`write(path)`（`index.rs:89`）序列化为 JSON 并通过 `ora_utils::atomic::write` 原子替换（同目录临时文件 + rename），确保并发读者永远不会观察到部分写入。

### 3.4 marketplace 同步流程

`RegistrySync::sync`（`crates/plugin-registry/src/source.rs:54-87`）是唯一获取路径，由注入的 `gitlancer::Git<R: GitRunner>` 驱动：

- 若 `checkout_dir/.git` 存在：`git.fetch(origin)` → `git.checkout(branch)` → `git.pull(--ff-only origin branch)`（`source.rs:58-86`）
- 否则（缺失）：要求父目录非空（否则 `MissingCloneParent` 错误），`create_dir_all(parent)`，然后 `git.clone(--branch <branch> <url> <dest>)`（`source.rs:74-78`）

生产接线使用真实 `gitlancer::CliGitRunner`，源 URL 与分支硬编码为 `https://github.com/ora-space/marketplace` / `main`（`crates/backend/src/plugin.rs:27-29`）。完整更新管线：**git 同步 → 递归扫描 `registry/` → 解析每个 `orax.toml` → 按 id 排序 → 原子 JSON 写入**（`crates/backend/src/plugin.rs:107-130`）。

无后台/定期同步——同步仅按需通过 `SyncAvailablePluginsRequest` 合约触发（`crates/contracts/src/plugin.rs:75-88`）。

### 3.5 错误分类

`RegistryError`（`crates/plugin-registry/src/error.rs:8-28`）有五个变体：`Git`、`Manifest`、`Io`、`Json`、`MissingCloneParent`。安装错误是 `ora-plugin-manager` 中的**独立**枚举 `InstallError`（`crates/plugin-manager/src/install.rs:17-39`）：`Download`、`MissingRelease`、`Extract`、`Io`。

---

## 4. 发现、安装与校验（Discovery, Install & Validation）

### 4.1 `PluginManager` 公共 API

`ora-plugin-manager` 声明四个私有模块并重导出窄公共表面（`crates/plugin-manager/src/lib.rs:3-15`）。`PluginManager` 结构持有不可变快照（`lib.rs:23-27`）。公共 API 仅三个方法：`discover(data_dir)` 一次性引导扫描（`lib.rs:31`）、`installed_plugins()` 返回已校验插件（`lib.rs:44`）、`discovery_issues()` 返回非致命问题（`lib.rs:49`）。**没有** `install`/`enable`/`disable`/`uninstall`/`refresh` 方法——安装属于独立的 `Installer` 结构，生命周期操作属于 `ora-plugin-lifecycle`（`crates/plugin-manager/README.md:20-22`）。

一个公共常量：`MAX_MANIFEST_BYTES = 1024 * 1024`（1 MiB，`lib.rs:20`）。

### 4.2 发现流程（discovery）

`discover(data_dir)`（`crates/plugin-manager/src/discovery.rs:17`）扫描 `<data_dir>/plugins/installed` 的**直接子目录**（`discovery.rs:18`）。每个包的清单路径为 `package_root.join("orax.toml")`（`discovery.rs:33`）。

`sorted_package_directories`（`discovery.rs:64-106`）：`fs::read_dir(installed_root)`；`NotFound` → 返回 `None`（空安装，**非**错误）（`discovery.rs:70`）；仅保留目录（`discovery.rs:86`）；排序确保确定性（`discovery.rs:103`）。

`read_and_validate_manifest`（`discovery.rs:109-179`）：使用 `fs::symlink_metadata` 检测符号链接（`discovery.rs:113`）；通过 `read_bounded` 读取最多 `MAX_MANIFEST_BYTES + 1` 字节（`discovery.rs:182-212`）；UTF-8 检查（`discovery.rs:142-149`）；调用 `PluginManifest::parse_installed(source)`（`discovery.rs:150`）；调用 `validation::validate`（`discovery.rs:171`）。

重复 id 检测：`HashMap<String, PathBuf>` 跟踪每个 plugin id 的首个包根（`discovery.rs:31-54`）。后续同 id 的包被推为 `DuplicatePluginId` 且**不**加入 `installed_plugins`（`discovery.rs:36-46`）。首个胜出。

### 4.3 校验（validation）

常量（`crates/plugin-manager/src/validation.rs:7-10`）：`SUPPORTED_PLUGIN_API_VERSION = 1`、`SUPPORTED_AGENT_CONTRACT_VERSION = 1`、`INSTALLED_ENTRYPOINT = "main.js"`。

**`PluginPackageType`**（`validation.rs:13-16`）——唯一变体 `Module`。

**`PluginContribution`**（`validation.rs:23-26`）——闭合枚举，唯一变体 `Agent(InstalledPluginAgent)`。这是最接近 `ora.contributes` 类比物的类型，但它是从 `ora.kind` **推导**的，而非从 `contributes` 字段读取。`kind()` 返回 `"agent"`（`validation.rs:30-34`）。

**`PluginEngines`**（`validation.rs:41-46`）——持有但不解释：文档注释（`validation.rs:38-40`）明确 "no consumer currently interprets these values"。`bun` 字段尤其过时（Ora 选择 Deno 而非 Bun）。

**`InstalledPluginAgent`**（`validation.rs:52-56`）——仅 `display_name: String` 和 `contract_version: u32`。文档注释（`validation.rs:49-51`）："The agent has no identifier of its own: one package provides exactly one agent."

**`InstalledPlugin`**（`validation.rs:59-71`）——完全校验后的快照记录，包含 `package_root`、`package_name`、`version`、`package_type`、`manifest_version`、`id`、`display_name`（设为 `package_name`）、`main: PortableRelativePath`（解析后的包含式入口）、`engines`、`contributes`。

`validate(package_root, manifest)`（`validation.rs:94-123`）：合成 `id = format!("{}/{}", namespace, name)`（`validation.rs:99-103`，`/` 分隔符硬编码）；调用 `validate_contribution`（`validation.rs:104`）；调用 `validate_main_path(package_root, INSTALLED_ENTRYPOINT)`（`validation.rs:105`）——入口名**固定**为 `main.js`，清单不能声明不同入口。

`validate_contribution`（`validation.rs:169-183`）：`PluginKind::Agent` → `Ok(PluginContribution::Agent(...))`；`PluginKind::Workbench` → `Err(invalid("kind", "unsupported plugin kind `workbench`; expected `agent`"))`。**`workbench` 在 manifest crate 中可解析，但在此处被拒绝**——当前唯一可安装的 kind 是 `agent`。

`validate_main_path`（`validation.rs:126-166`）：`PortableRelativePath::parse` 拒绝不安全相对路径（`validation.rs:131`）；`CanonicalPathRoot::new` + `root.resolve_existing(&relative)` 检查目标保持在包内（`validation.rs:140-146`）——这是符号链接逃逸防护；`resolved.is_file()` 检查（`validation.rs:152`）。

校验**不做**的事：不检查 `dependencies.ora`（README `:26` 明确列为非职责）；不读取或校验任何 `permissions` 块——manifest raw 结构体使用 `deny_unknown_fields`（`manifest.rs:250,267`），含 `[permissions]` 表的清单会在解析阶段被**拒绝**。

### 4.4 安装流程（install）

`Installer<D>`（`crates/plugin-manager/src/install.rs:45-47`）泛型于 `D: HttpDownload`（`install.rs:50-52`），下载器注入，传输无关（`crates/plugin-manager/README.md:23-25`）。

`install(manifest, source, data_dir)`（`install.rs:64-86`）：

1. `download_package(manifest, source, data_dir)` 获取并校验到缓存（`install.rs:68`）
2. `package_dir = data_dir.join("plugins").join(INSTALLED_ROOT).join(manifest.name().as_str())`（`install.rs:71-74`）——安装目标键是 `manifest.name()` **而非** `namespace/name`
3. `extract_archive(ArchiveFormat::Zip, &archive_path, &package_dir, &ExtractLimits::default())`（`install.rs:75-80`）——始终 Zip 格式
4. 返回 `Ok(package_dir)`（`install.rs:85`）

`download_package`（`install.rs:89-118`）：`digest = manifest.sha256().ok_or(InstallError::MissingRelease)?`（`install.rs:95`）——清单**必须**声明 sha256；缓存目录 `data_dir/plugins/cache`（`CACHE_ROOT = "cache"`，`install.rs:10`）；压缩包名 `format!("{}-{}{}", name, version, ".orax")`（`RELEASE_EXTENSION = ".orax"`，`install.rs:14,101-106`）；构建 `DownloadRequest` 含 `Checksum::sha256`（`install.rs:108-115`）；`self.downloader.download(request).await?`（`install.rs:116`）。

SHA-256 校验在下载器**内部**强制执行，而非 Installer 中（`crates/utils/src/http/types.rs:20-30`）。本地后端在复制时计算 `Sha256`，若 `checksum.digest() != digest` 则 `DownloadError::ChecksumMismatch`（`crates/utils/src/http/local.rs:107-116`）。不匹配在 `.tmp` 被 rename 到目标之前中止——测试 `rejects_checksum_mismatch_and_installs_nothing` 证实（`install.rs:223-251`）。

提取是面向不可信压缩包内容的安全边界。`extract_archive`（`crates/utils/src/archive/extract.rs:18-46`）委托给 `TreeWriter`，每个条目路径在写入前通过 `StrictRelativePath` 校验（`crates/utils/src/archive/mod.rs:5-7`）。`ExtractLimits::default()`（`crates/utils/src/archive/limits.rs:20-29`）：`max_archive_bytes: 50 MiB`、`max_total_bytes: 200 MiB`、`max_entries: 5000`。加密压缩包、符号链接和特殊条目被拒绝（`archive/mod.rs:5-7`）。

### 4.5 生产接线

`PluginApi` 持有 `installer: Installer<ReqwestDownloader>`（`crates/backend/src/plugin.rs:42`）。`PluginApi::install`（`plugin.rs:192-235`）是唯一生产调用者：通过 `RegistryIndex::resolve_manifest` 解析 release 清单（`plugin.rs:197`）；提取 release URL（`plugin.rs:208-216`）；调用 `self.installer.install`（`plugin.rs:218-225`）；调用 `self.lifecycle.scan_plugins` 使新包无需重启即出现在内存快照中（`plugin.rs:228-230`）。

### 4.6 文件落地位置

| 产物                  | 位置                                                                               |
| --------------------- | ---------------------------------------------------------------------------------- |
| marketplace git 检出  | `<data_dir>/plugins/sources/github.com/ora-space/marketplace`                      |
| 缓存索引              | `<data_dir>/plugins/cache/registry_index.json`                                     |
| 下载的 `.orax` 压缩包 | `<data_dir>/plugins/cache/<name>-<version>.orax`                                   |
| 提取的包              | `<data_dir>/plugins/installed/<name>/`（含 `orax.toml`、`main.js`、`logo.svg` 等） |

---

## 5. 生命周期与运行时（Lifecycle & Runtime）

### 5.1 生命周期状态机

`ora-plugin-lifecycle` crate 拥有**仅后端**的编排，连接文件系统发现、持久化启用状态、进程级运行时状态和应用失效（`crates/plugin-lifecycle/README.md:3-5`）。

内部状态机（`crates/plugin-lifecycle/src/state.rs:16-27`）：

```rust
pub(super) enum ManagedPluginState<Runtime> {
    Disabled,
    Enabled(EnabledRuntime<Runtime>),
}

pub(super) enum EnabledRuntime<Runtime> {
    Stopped,
    Starting { attempt: u64 },
    Running { attempt: u64, runtime: Runtime },
    Failed { reason: String },
}
```

此设计使"一个禁用的插件拥有活跃运行时"**不可表示**——`Disabled` 变体不携带 `Runtime`（`state.rs:15`）。

线缆合约枚举 `PluginRuntimeStatus`（`crates/contracts/src/plugin.rs:22-28`）：`Stopped`/`Starting`/`Running`/`Failed { failure_reason }`。此枚举通过 `#[serde(flatten)]` 内联到 `InstalledPlugin` JSON 对象中（`plugin.rs:43-45`）。

关键状态转换：`enable_plugin` 持久化 `PluginEnabledState::Enabled`（`lib.rs:202-235`）；`activate_plugin` 递增 `next_attempt` 并 spawn `complete_launch`（`lib.rs:310-357`）；`complete_launch` 成功 → `Enabled(Running{attempt, runtime})`（`lib.rs:580-596`），失败 → `Enabled(Failed{reason})`（`lib.rs:618`）；`stop_plugin` 调用 `runtime.stop()`（`lib.rs:360-432`）；`disable_plugin` 持久化 `Disabled` 并停止运行时（`lib.rs:238-307`）；`uninstall_plugin` 停止运行时、删除持久化状态、从磁盘删除包、从快照移除（`lib.rs:435-508`）。

每次转换由 attempt 号守卫：`transition_to_stopped` 和 `transition_to_failed` 均检查 `owns_attempt`——`attempt: u64` 必须匹配当前状态的 attempt 才能应用转换（`lib.rs:584-589,634-640,667-675`）。这防止过期启动覆盖新转换。

### 5.2 扫描对账（scan reconciliation）

`scan_plugins` 是**对账屏障**（`crates/plugin-lifecycle/src/scan.rs:20-180`）：获取 `scan_lock` 串行化扫描（`scan.rs:24`）；获取**所有**缓存插件操作锁按稳定 id 顺序（`scan.rs:31-36`）；通过 `PluginManager::discover` 重新发现已安装包（`scan.rs:38-40`）；计算 `removed_ids` 并停止已移除包的运行时（`scan.rs:45-73`）；加载所有持久化状态，分区为 `persisted_by_id` 和 `orphaned_persisted_ids`，删除孤儿持久化行（`scan.rs:76-131`）；重建 `managed_by_id`（`scan.rs:133-172`）；为所有 `changed_ids` 发布状态变更（`scan.rs:142-175`）。

**关键不变量**：缺失持久化状态意味着禁用（`lib.rs:275` 注释，`state.rs:60-61`）。仅 `enable_plugin` 创建首条持久化行（`lib.rs:223-227`）。

### 5.3 操作串行化

每插件操作锁：`acquire_operation(&plugin_id)` 从 `BTreeMap<PluginId, Arc<AsyncMutex<()>>>` 返回 `OwnedMutexGuard`（`lib.rs:522-538`）。这串行化**同一插件**的操作，同时允许无关插件独立进展。扫描获取所有操作锁，使其成为完整屏障。

### 5.4 持久化状态端口

`PluginStateRepository` trait（`crates/application/src/plugin/ports.rs:8-23`）提供 `find_plugin_state`、`list_plugin_states`、`set_plugin_enabled`、`delete_plugin_state`。

### 5.5 `PluginRuntime` 与 `PluginRuntimeLauncher` trait

定义在 `lib.rs:70-87`：

```rust
pub trait PluginRuntime: Clone + Send + Sync + 'static {
    fn stop(&self) -> impl Future<Output = Result<(), PluginRuntimeFailure>> + Send;
    fn wait_for_exit(&self) -> impl Future<Output = PluginRuntimeExit> + Send + 'static;
}

pub trait PluginRuntimeLauncher: Clone + Send + Sync + 'static {
    type Runtime: PluginRuntime;
    fn launch(&self, request: PluginLaunchRequest)
        -> impl Future<Output = Result<Self::Runtime, PluginRuntimeFailure>> + Send;
}
```

### 5.6 生产适配器：`DenoPluginRuntimeLauncher`

`crates/plugin-lifecycle/src/runtime.rs:32-100`。委托给 `ora_plugin_runtime::PluginRuntime::launch`，使用 `TokioProcessSpawner`。返回 `(runtime, _notifications)` 但**丢弃 notifications 接收器**（`runtime.rs:72`）——生命周期 crate 只关心 stop/exit，不关心协议流量。超时：ready=10s、call=30s、shutdown=5s（`runtime.rs:22-27`）。权限硬编码为 `Vec::new()`（`runtime.rs:65`）——通用生命周期路径使用**零权限**。

### 5.7 Deno 子进程运行时

`ora-plugin-runtime` crate 拥有每个沙箱化插件进程的生命周期与双向 stdio 协议（`crates/plugin-runtime/README.md:3-4`）。该 crate **不**发现、安装、选择或配置插件——调用者提供 plugin id、入口点、Deno 可执行文件、权限标志和要调用的方法（`crates/plugin-runtime/README.md:9-12`）。

`PluginRuntime::launch`（`crates/plugin-runtime/src/lib.rs:100-211`）：检查 `config.entrypoint.is_file()`（`lib.rs:108-110`）；构建 `ProcessSpec` 为 `deno run --no-prompt <permissions> <entrypoint>`（`lib.rs:112-116`）；通过 `ProcessSpawner` spawn 进程（`lib.rs:117-119`）；取 stdin/stdout/stderr（`lib.rs:120-129`）；创建通道（`lib.rs:131-136`）；spawn 四个任务（`lib.rs:157-171`）；在 `ready_timeout` 内等待 `Ready` 状态（`lib.rs:173-204`）。

### 5.8 二进制帧编解码器

帧格式（`crates/plugin-runtime/src/codec.rs`）：

```
[4 字节大端 u32 长度] [1 字节帧类型] [长度-1 字节 UTF-8 JSON 负载]
```

常量：`JSON_RPC_FRAME_TYPE: u8 = 0x01`（`codec.rs:5`）、`MAX_FRAME_LENGTH: usize = 16 * 1024 * 1024`（16 MiB）（`codec.rs:6`）。SDK 侧常量一致（`packages/plugin-sdk/src/protocol.ts:1-2`）。

### 5.9 `ora/register` 握手

常量（`crates/plugin-runtime/src/protocol.rs:8-10`）：`JSON_RPC_VERSION = "2.0"`、`REGISTER_METHOD = "ora/register"`、`SHUTDOWN_METHOD = "ora/shutdown"`。

`PluginRegistration`（`protocol.rs:18-22`）：

```rust
pub struct PluginRegistration {
    pub methods: HashSet<String>,
    pub emits: HashSet<String>,
}
```

- `methods`：host 可 `invoke` 的方法。调用不在集合中的方法在本地失败（`crates/plugin-runtime/README.md:30-31`）
- `emits`：插件可主动发送的通知。不在白名单中的通知是协议错误（`crates/plugin-runtime/README.md:32-35`）

两个集合在进程生命周期内**固定**；二次注册是协议错误（`protocol.rs:101-103`）。

握手处理 `handle_plugin_originated`（`protocol.rs:89-129`）：**拒绝任何携带 `id` 的消息**——插件只能发送通知（`protocol.rs:94-97`）。这是"反向请求/响应被故意移除"的边界（`crates/plugin-runtime/README.md:68-70`）。若 method 为 `ora/register` 且状态为 `Starting`：解析 `methods`（必需）和 `emits`（可选，默认空），存储 registration，设状态为 `Ready`（`protocol.rs:100-113`）。若 method 在 `emits` 中：转发为 `PluginNotification` 到 inbound 通道（`protocol.rs:115-128`）。

### 5.10 请求关联机制

`PluginRuntime::invoke(method, params)`（`crates/plugin-runtime/src/lib.rs:219-260`）：`ensure_ready()` 拒绝非 `Ready` 状态（`lib.rs:282-293`）；检查方法在 `registration.methods` 中（`lib.rs:221-229`）；通过 `AtomicU64::fetch_add(1, Relaxed)` 从 1 开始分配 `request_id`（`lib.rs:232`）；创建 oneshot channel，插入 `(request_id, sender)` 到 `PendingRequests.active`（`lib.rs:233-238`）；发送 JSON-RPC 请求（`lib.rs:239-248`）；在 `call_timeout` 内等待 oneshot（`lib.rs:250-259`）。

`PendingRequests`（`crates/plugin-runtime/src/state.rs:25-69`）有三种响应匹配状态：`Pending(sender)`（活跃请求，投递响应）、`Abandoned`（请求超时，迟到响应被静默丢弃，`state.rs:79-81`）、`Unmatched`（真正未知 id → 协议失败，`state.rs:82-84`）。Abandoned 队列上限 256 条目（`ABANDONED_REQUEST_CAPACITY`，`state.rs:15`）。

### 5.11 协议方法

**Host → 插件**：`invoke(method, params)` → 请求/响应，受 `call_timeout` 约束（`lib.rs:219-260`）；`notify(method, params)` → 通知（无 id），**不受** `call_timeout` 约束（`lib.rs:262-279`）；`ora/shutdown`——在 `RuntimeLease::drop` 和 `request_shutdown` 时发送（`lib.rs:79-84,327-332`）。

**插件 → Host**：`ora/register`——一次性握手通知（`protocol.rs:100-113`）；`emits` 白名单中的任何方法——转发为 `PluginNotification`（`protocol.rs:115-128`）；其他任何方法（含携带 `id` 的消息）→ 协议错误，杀死进程（`protocol.rs:94-97,116-119`）。

### 5.12 四个 spawn 任务

`run_writer`（`crates/plugin-runtime/src/tasks.rs:19-47`）：串行化所有出站帧；`run_reader`（`tasks.rs:50-78`）：从 stdout 读取帧、解析 JSON、调用 `handle_message`，干净 EOF → `fail_runtime`（`tasks.rs:57-58`）；`run_stderr`（`tasks.rs:81-107`）：持续将 stderr 排入 `ora_info!` 日志，8KB 缓冲区；`run_supervisor`（`tasks.rs:110-161`）：监督进程退出并保证有界优雅关闭。

`fail_runtime`（`state.rs:106-113`）：设状态为 `Failed(reason)`，以 `Unavailable` 错误失败所有 pending 请求，关闭 inbound 通道，向 supervisor 发送 `ProtocolFailure`。此后连接不可用，所有等待的调用者被唤醒。

### 5.13 `PluginLifecycleConfig` 无权限字段

`crates/plugin-lifecycle/src/lib.rs:28-31`：

```rust
pub struct PluginLifecycleConfig {
    pub data_directory: PathBuf,
    pub deno_path: PathBuf,
}
```

无法通过生命周期层传递每插件权限授予。`DenoPluginRuntimeLauncher` 硬编码 `permissions: Vec::new()`（`runtime.rs:66`），agent 插件路径完全绕过生命周期 launcher，使用自己的 `AGENT_PLUGIN_PERMISSIONS` 常量。

---

## 6. 能力与设置（Capabilities & Settings）

### 6.1 插件能贡献什么

manifest 声明每个插件恰好一个 `kind`。`PluginKind`（`crates/plugin-manifest/src/enums.rs:49-51`）有两个变体 `Workbench` 和 `Agent`，但在运行时**仅 `Agent` 被接受**。`validate_contribution`（`crates/plugin-manager/src/validation.rs:169-183`）显式拒绝 `Workbench`。`PluginContribution` 枚举（`validation.rs:24-26`）仅有一个变体 `Agent(InstalledPluginAgent)`。

线缆合约类型 `InstalledPlugin`（`crates/contracts/src/plugin.rs:34-46`）硬编码 `agent` 字段，确认一包一 agent：

```rust
pub struct InstalledPlugin {
    pub id: String,
    pub package_name: String,
    pub display_name: String,
    pub version: String,
    pub kind: String,
    pub main: String,
    pub agent: InstalledPluginAgent,
    pub enabled: bool,
    #[serde(flatten)]
    pub runtime: PluginRuntimeStatus,
}
```

**今天，插件只能贡献一个 agent。** 没有 command-contribution、tool-contribution 或 MCP-contribution 类型。

### 6.2 运行时能力声明：握手而非清单

插件的**运行时能力**不在清单中声明，而在**线缆协议握手**（`ora/register`）中声明两个集合：`methods`（host 可调用）和 `emits`（插件可主动发送）。这是实际的"contributes"等价物（`crates/plugin-runtime/src/protocol.rs:18-22`）。

### 6.3 agent 契约校验

host 启动 agent 插件时，`verify_agent_contract`（`crates/backend/src/agent_runtime/plugin_agent/control.rs:109-132`）检查 registration 包含全部四个必需方法：`methods` 含 `agent/start`、`agent/stop`、`agent/listModels`，`emits` 含 `agent/acp`（`control.rs:12-18`）。

**规格与实现矛盾**：规格（`specs/plugin/4-agent.md:35-74`）命名方法为 `ora/agent/start_agent`、`ora/agent/stop_agent`、`ora/agent/list_models`，参数含 `agent_instance_id`。实现（SDK `packages/plugin-sdk/src/agent.ts:4-7` 与 host `control.rs:12-18`）使用无 `ora/` 前缀、无 `_agent`/`_models` 后缀的 `agent/start`、`agent/stop`、`agent/listModels`、`agent/acp`，且 `agent/start` 的 `AgentStartContext` 为 `{ cwd, hostVersion }`（`agent.ts:25-30`），**无 `agent_instance_id`**。引用规格设计 MCP 插件时必须以 SDK + host 代码为权威线缆表面。

### 6.4 Deno 权限运行时行为

规格称插件以零 Deno 权限运行（`specs/plugin/0-overview.md:7-14`）。通用生命周期 launcher 确认这一点——`crates/plugin-lifecycle/src/runtime.rs:66` 硬编码 `permissions: Vec::new()`。

但 **agent 插件 launcher 授予宽泛 Deno 权限**（`crates/backend/src/agent_runtime/plugin_agent/mod.rs:34-35`）：

```rust
const AGENT_PLUGIN_PERMISSIONS: [&str; 4] =
    ["--allow-run", "--allow-read", "--allow-env", "--allow-net"];
```

代码明确文档化这是**有意的、延迟的缺口**（`mod.rs:27-33`）：agent 插件需要 `--allow-run` 及其 CLI 所需的一切，使其与 host 一样特权。这直接矛盾规格的零权限模型（`specs/plugin/0-overview.md:7`、`specs/plugin/1-capability.md:86`）。

### 6.5 清单中的权限模型：仅规格，未实现

规格 `specs/plugin/1-capability.md:82-193` 描述 `orax.toml` 中的 `[permissions]` 表，含 `permissions.plugin_data`（none/read/read-write）、`permissions.process`（executables/stdio/signals）、`permissions.process.sandbox`（workspace/network/environment）。规格明确这些**不**翻译为 `deno run --allow-*`；Deno 进程保持零权限，清单权限仅 gate 插件可调用的 Ora SDK 方法（`specs/plugin/1-capability.md:84-86`）。

**关键缺口**：这一切**未**在 `ora-plugin-manifest` 中建模。`RawPluginManifest`/`RawInstalledManifest` 没有 `permissions` 字段（`manifest.rs:251-264,267-281`），`deny_unknown_fields` 会拒绝含 `[permissions]` 表的清单。全仓库无 Rust 代码解析、存储或强制执行规格中的清单 `[permissions]` 声明（Deno 进程级权限标志确实存在——见 §6.4 的 `AGENT_PLUGIN_PERMISSIONS`（`crates/backend/src/agent_runtime/plugin_agent/mod.rs:34`）与 `crates/plugin-runtime/src/lib.rs:34` 的 `PluginRuntimeConfig.permissions`——但它们按 `kind` 硬编码或由调用方注入，并非派生自任何清单权限声明）。

### 6.6 `[[targets]]` 平台特定资产：仅规格，未实现

规格 `specs/plugin/3-registry.md:52-88` 描述平台特定 release 资产通过 `[[targets]]` 数组，与顶层 `url`/`sha256` 互斥。这也**不存在**于 manifest 结构体中。今天 manifest 仅建模**通用** release 形态（单一 `url` + `sha256`）。

### 6.7 设置模型

规格 `specs/plugin/2-settings.md` 描述双文件模型：`assets/config.json`（不可变，随包发布，声明设置 schema）和 `store.json`（可变，Ora 管理，位于 `data/<namespace>/<plugin_name>/`，仅持有用户值）。设置类型：`string`、`number`、`boolean`、`secret`（OS 凭据存储后端的不透明 `secretRef`）、`file`、`directory`。解析顺序：`store.json` 用户值 → `config.json` 默认 → `NeedsConfiguration`。配置状态：`Unconfigured` | `Ready` | `NeedsConfiguration` | `InvalidDeclaration`。作用域第一版为**插件全局**（跨所有工作区相同值），工作区覆盖明确延迟（`specs/plugin/2-settings.md:171-173`）。

**实现状态**：整个设置管线——从 `assets/config.json` 解析、`compile`/`resolve`、`store.json` 持久化到 OS 凭据存储集成——**仅有规格，零实现**。grep `CompiledPluginConfig|ResolvedPluginConfig|NeedsConfiguration|plugin_data|secret_ref|secretRef|store.json|SettingDecl|PluginSetting` 仅在两个规格 markdown 文件中找到匹配。`apps/desktop/src-tauri/src/config.rs` 是 host Desktop 配置（worktree root、dashboard host/port），非插件设置。`DesktopConfig` 结构体无插件设置字段。

### 6.8 规格管理

规格管理（`docs/spec-management.md`）是已存在于磁盘上的 Markdown 规格文档的**只读审阅面**，与插件系统完全无关。它索引来自自动发现源目录的文档（`docs/spec-management.md:15-25`）。`specs/` 目录（含 `specs/plugin/*.md`）是一个这样的规格源。MCP 插件设计不会与规格管理交互，除非 MCP 插件本身想将规格文档暴露为工具。

---

## 7. Agent 插件桥接设计：当前状态 vs 目标

> 这是本文档最核心的章节。MCP 插件设计必须以此为参照系，理解 agent 插件如何完全连接、哪些是已实现的、哪些是延迟的。

### 7.1 权威设计文档

仓库根目录的 `plugin-agent-runtime.md` 是 agent 插件的权威设计文档，描述目标架构和 6 阶段落地计划。

### 7.2 已实现的状态（与设计文档对比）

代码库**已实现设计文档大部分阶段**。设计文档 §0 描述的是撰写时的状态，非当前代码。

#### `AgentCli` 枚举——4 变体，非 5

`crates/domain/src/agent_cli.rs:14-19`：

```rust
pub enum AgentCli {
    Nga,
    CodeAgentCli,
    Claude,
    Codex,
}
```

`AgentCli::ALL` 有 4 个元素（`agent_cli.rs:22`）。文档的"5 个硬编码 CLI"现为 4——**OpenCode（`ora-space.opencode`）已从枚举移除**，改由已安装的 agent 插件供应（`agent_cli.rs:8-12` 注释）。phase 5（身份泛化）和 phase 6（内置 CLI 插件化）**部分完成**——五个 CLI 中一个已被插件化。

#### `AgentRef`——已实现为开放身份

`crates/domain/src/agent_ref.rs:12`：

```rust
pub struct AgentRef(String);
```

含 `parse`、`as_str`、`Display`、`TryFrom<String>`。`Session.agent_ref` 已是 `AgentRef` 而非 `AgentCli`（`crates/domain/src/session.rs:78`）。DB 列仍存储相同的 `"ora-space.claude"` 风格字符串——无需迁移（`docs/agent-runtime.md:7`、`agent_cli.rs:26-28`）。

#### `ConnectionSupervisors`——已是按 `AgentRef` 键控的 map

`crates/backend/src/agent_runtime/connection.rs:149-151`：

```rust
pub(super) struct ConnectionSupervisors {
    supervisors: Arc<BTreeMap<AgentRef, ConnectionSupervisor>>,
}
```

"5 个命名字段 → HashMap"转换**已完成**。`start()`（`connection.rs:158-184`）链接 `AgentCli::ALL`（作为 `AgentSource::Cli`）与 `agent_plugins`（作为 `AgentSource::Plugin`），通过 `resolve_supervised_agents` 按 identity 去重（`connection.rs:331-354`），为每个 agent 创建一个 supervisor。内置 CLI 优先提供；声明已被监督 identity 的插件被**丢弃，不允许替换**（`connection.rs:326-330` 注释，测试 `connection.rs:841-863`）。

#### `AgentSource` 枚举——CLI 与 Plugin 之间的桥

`crates/backend/src/agent_runtime/connection.rs:54-59`：

```rust
pub(super) enum AgentSource {
    Cli(AgentCli),
    Plugin(PluginAgentSpec),
}
```

两个变体都产生调用者无法区分的 `RuntimeConnection`——"这正是让插件提供和内置 agent 共存而无需在别处分支的原因"（`connection.rs:50-52` 注释）。

#### plugin-runtime——已双向

设计文档 §7 描述使 plugin-runtime 双向所需的四项更改，**全部已实现**：

1. **Inbound 通道**：`launch` 返回 `(Self, mpsc::UnboundedReceiver<PluginNotification>)`（`crates/plugin-runtime/src/lib.rs:100-103`），接收器无界（`lib.rs:97-99` 注释）
2. **`emits` 白名单**：`handle_plugin_originated` 检查 `registration.emits.contains(method)` 并转发白名单通知（`protocol.rs:115-128`），未列出的方法是协议错误
3. **`notify(method, params)`**：存在并发送 host→插件通知，不占用 pending 表（`lib.rs:262-279`），`call_timeout` 不适用于 `notify`（`lib.rs:264-266` 注释）
4. **控制请求超时墓碑**：`PendingRequests.abandon` 将超时请求 id 移入有界 256 条目 `VecDeque`（`state.rs:43-51`），迟到响应被静默丢弃（`state.rs:79-81`），真正未知 id 保持协议失败（`state.rs:82-84`）

反向请求/响应（插件调用 host）**故意不存在**（`crates/plugin-runtime/README.md:68-70`、`protocol.rs:94-97`）。

#### ora-acp——传输抽象已存在

`crates/acp/src/transport.rs:25-27`：

```rust
pub trait AcpTransport: Send + Sync + 'static {
    fn send(&self, message: Value) -> impl Future<Output = Result<(), AcpError>> + Send;
}
```

两个实现：`NdjsonTransport<Writer>`（行分隔 JSON over stdio，`MAX_FRAME_BYTES = 8 MiB`，`transport.rs:11`）和 `PluginAcpTransport`（在后端，`crates/backend/src/agent_runtime/plugin_agent/transport.rs:16-33`）：`send` 调用 `runtime.notify("agent/acp", message)`。

`AgentTransport` 枚举（`transport.rs:43-55`）：

```rust
pub(crate) enum AgentTransport {
    Stdio(NdjsonTransport<ChildStdin>),
    Plugin(PluginAcpTransport),
}
```

`Stdio` 变体仍为 4 个未插件化的 CLI 保留（`transport.rs:41-42` 注释）。

`AcpPeer<Transport>`（`crates/acp/src/peer.rs:22-71`）泛型于 `AcpTransport`，通过 `route_frame`（`peer.rs:118-216`）将入站 `AcpMessages` 解复用为 `AcpInboundEvent`（session 更新、权限请求、session 响应、致命错误）。ACP 有自己的 `PendingRequests`（`crates/acp/src/pending.rs:27-78`），含相同的 active/abandoned/unmatched 三态关联、自己的 256 条目墓碑队列、两种 pending 类型：`Direct`（oneshot）和 `Session { session_id }`（通过有序事件流路由）。

#### `plugin_agent` 模块——完全连接的 agent 插件运行时

`crates/backend/src/agent_runtime/plugin_agent/`（`mod.rs`、`control.rs`、`transport.rs`、`inbound.rs`、`README.md`）。

`launch(spec, home_directory, host_version)`（`mod.rs:66-96`）：

1. 配置 `PluginRuntimeConfig`，agent 插件权限为 `["--allow-run", "--allow-read", "--allow-env", "--allow-net"]`（`mod.rs:34-35,75`）——使 agent 插件"与 host 大致同特权"（`crates/backend/src/agent_runtime/plugin_agent/README.md:70-73`）
2. 调用 `PluginRuntime::launch`（`mod.rs:80-81`）
3. `verify_agent_contract` 检查 `methods` 含 `agent/start`、`agent/stop`、`agent/listModels` **且** `emits` 含 `agent/acp`（`control.rs:109-132`）——失败 → `ContractIncomplete` → 终端 `Failing`
4. `start_agent` 调用 `agent/start`，参数 `{cwd, hostVersion}`（`control.rs:135-154`），校验结果含 `protocol: "acp"` 和 `acpVersion: 1`（`control.rs:146-152`）；错误码 `-32001` → `AgentNotInstalled`（`control.rs:31,48-50`）
5. `discard_frames_before_start` 排空 `agent/start` 返回前到达的任何通知（`inbound.rs:15-30`）
6. `spawn_frame_forwarding` spawn 任务将 `agent/acp` 通知转发到 `AcpMessages` 通道（`inbound.rs:37-65`）——非 `agent/acp` 通知被 warn 并丢弃；非 object `params` 被 warn 并丢弃；**单个格式错误帧不会失败连接**（`crates/backend/src/agent_runtime/plugin_agent/README.md:52-54`、`inbound.rs:42-43` 注释）

`stop_agent`（`control.rs:162-180`）：调用 `agent/stop`，2 秒超时（`AGENT_STOP_TIMEOUT`，`control.rs:25`）。失败被记录而非传播——"plugin cleanup is best effort"（`control.rs:159-161` 注释）。

`list_models`（`control.rs:183-193`）：调用 `agent/listModels`，返回 `Vec<PluginAgentModel>`。

拆卸顺序（`connection.rs:404-426`、`crates/backend/src/agent_runtime/plugin_agent/README.md:57-63`）：`agent/stop`（2s 超时）→ `runtime.shutdown_and_wait()`（发送 `ora/shutdown`，`PLUGIN_SHUTDOWN_TIMEOUT`=3s 后杀进程树，`mod.rs:25`）→ supervisor 回收完整进程树。匹配设计文档 §9 的关闭顺序。

#### 桌面引导——已喂插件给后端

`apps/desktop/src-tauri/src/lib.rs:319-335`：`agent_plugin_packages` 将每个发现的 agent 插件映射为 `AgentPluginPackage { id, deno_path, entrypoint }` 并交给 `Backend::open`。桌面**确实**将发现的插件包作为 `AgentPluginPackage` 喂给后端（`bootstrap.rs:39-46`），通过 `From` 转为 `PluginAgentSpec`（`mod.rs:46-54`）。设计文档称"Desktop only feeds settings list"已**不再准确**。

#### 连接 supervisor 失败与重启

`run_supervisor`（`connection.rs:465-566`）：设 `Starting` → 调用 `spawn_initialized_process`（对插件调用 `plugin_agent::launch` 再 ACP `initialize`，`connection.rs:619-688`）→ 成功则递增 generation、设 `Ready`、运行 `run_process_generation` → 退出则设 `Unavailable`、`routes.fail_generation`、`mark_running_sessions_stopped`、检查 `RestartCircuit`。`StartFailure::Terminal`（契约不完整）→ `Failing`，不重试（`connection.rs:527-535`）。`StartFailure::Retryable` → `Unavailable`，退避重试；`AgentCliNotFound` 可重试且不记录（`connection.rs:543-557`）。

`RestartCircuit`（`restart_circuit.rs:1-38`）：60 秒内超过 3 次失败 → `Stop`（断路器打开）。`AGENT_NOT_INSTALLED` **不计入**断路器（`connection.rs:543`）。

### 7.3 两类流量

设计文档（`plugin-agent-runtime.md:30-65`）和实现代码定义恰好两类流量：

**类别 1：控制方法——JSON-RPC 请求/响应**

| 属性 | 值                                                             |
| ---- | -------------------------------------------------------------- |
| 方向 | Host → 插件                                                    |
| 形态 | `invoke(method, params)` → 响应                                |
| 关联 | `PluginRuntime` 的 `AtomicU64` 请求 id（`lib.rs:232`）         |
| 超时 | `call_timeout`（agent 插件 30s，`mod.rs:23`）                  |
| 方法 | `agent/start`、`agent/stop`、`agent/listModels`                |
| 约束 | 每个请求在 `PendingRequests.active` 中有一个 `oneshot::Sender` |

**类别 2：ACP 透传——JSON-RPC 通知**

| 属性 | 值                                                                               |
| ---- | -------------------------------------------------------------------------------- |
| 方向 | 双向（host→插件和插件→host）                                                     |
| 形态 | `notify("agent/acp", params)` / `PluginNotification{method:"agent/acp", params}` |
| 关联 | **plugin-runtime 层无关联**——ACP 自己的 `id` 处理关联                            |
| 超时 | **无**——由 ACP 层和 session 取消约束，非 `call_timeout`                          |
| 方法 | `agent/acp`（必须在 `emits` 白名单中）                                           |

host 对 ACP 是**纯管道**（`plugin-agent-runtime.md:206-217`、`crates/backend/src/agent_runtime/plugin_agent/README.md:25-27`）：不解析、不校验、不重写 `params`。非 object `params` 被 warn 丢弃而非连接失败（`inbound.rs:52-57`）。`agent/start` 完成前的帧被丢弃（`inbound.rs:15-30`）。未知 ACP 方法原样透传——"这让插件支持 host 从未听过的 ACP 方法"（`plugin-agent-runtime.md:216-217`）。

**为何用通知而非 invoke**（`plugin-agent-runtime.md:221-229`）：

1. ACP 帧已携带自己的 `id`；`AcpClient` 已实现 session 排序、`PendingSessionRequest`、取消后墓碑。第二个 `PluginRuntime` id 层意味着两个超时、两个取消路径、两个状态不匹配的地方
2. `call_timeout`（30s）会切断合法运行数分钟的 `session/prompt` 请求
3. `session/update`（流式更新）是无请求/响应形态的通知——在 invoke 模型中无处安放

### 7.4 进程死亡关闭两类流量

`run_reader` 检测 stdout EOF → `fail_runtime`（`tasks.rs:57-58`）→ 设 `Failed`，以 `Unavailable` 失败所有 pending invoke 调用者，关闭 inbound 通道（`state.rs:106-113`）→ inbound 通道关闭使 `spawn_frame_forwarding` 的 `notifications.recv()` 返回 `None`，结束转发任务（`inbound.rs:59-60`）→ `AcpMessages` sender 被丢弃，`route_messages` 检测为 `AcpInboundEvent::Fatal(AcpError::StreamClosed)`（`peer.rs:112`）→ 连接 supervisor 的 `run_process_generation` 见 `None`/`Fatal` 入站事件返回 `false`（`connection.rs:599-607`）→ `run_supervisor` 设 `Unavailable`、失败该 generation 的 routes、标记运行中的 session 已停止、终止+回收进程树、检查断路器（`connection.rs:501-526`）。

**崩溃半径 = 一个 agent**：一插件一 agent 一进程模型（`plugin-agent-runtime.md:69-82`）意味着无扇出——死掉的插件进程只影响它自己 agent 的 session。

### 7.5 设计文档描述为目标但尚未完成的部分

1. **Phase 6——内置 CLI 插件化**：5 个 CLI 中 4 个仍作为直接 CLI spawn 运行（`spawn_cli_connection`，`connection.rs:691-728`），仅 OpenCode 被插件化。`AgentCli`、`cli_path.rs`、`launch_arguments()` 仍存在
2. **`Stdio` 变体**仍有人使用——不能删除（`transport.rs:41-42` 注释）
3. **沙箱化**——agent 插件获得 `--allow-run` 和完整 read/env/net 访问，使其与 host 同特权。这是"有意的、已文档化的缺口"（`mod.rs:30-35`、`crates/backend/src/agent_runtime/plugin_agent/README.md:70-73`、`plugin-agent-runtime.md:296-313`）
4. **前端模型选择器**仍仅提供内置 CLI——"插件提供的 agent 被监督、可按 id 绑定、由运行时状态端点报告，但选择器不枚举它"（`docs/agent-runtime.md:155-159`）
5. **`agent/modelsChanged` 通知**——设计文档 §12.2 提议用于模型列表刷新，但代码中不存在。`listModels` 每连接 generation 调用一次（`connection.rs:739`、`docs/agent-runtime.md:99-101`）
6. **`agent/exited` 通知**——设计文档 §12.5 提议用于区分 CLI 崩溃与挂起，但代码中不存在

---

## 8. 项目规范检查清单：新插件代码须满足

### 8.1 Crate 身份与布局

- Crate 名前缀 `ora-`；目录名为后缀。如 `crates/mcp` → crate `ora-mcp`（`AGENTS.md:12`）
- `[lib]` 块含 `name = "ora_<crate>"` 和 `path = "src/lib.rs"`（如 `crates/plugin-manager/Cargo.toml:6-8`）
- 继承 workspace 包字段：`version.workspace = true`、`edition.workspace = true`、`[lints] workspace = true`（如 `crates/plugin-manager/Cargo.toml:3-4,10-11`）
- 依赖通过 workspace 目录引用（`{ workspace = true }`），在线内 pin feature（如 `crates/plugin-manager/Cargo.toml:15`）
- 将 crate 添加到 `members` 和 `default-members`，并在 `[workspace.dependencies]` 添加 `ora-* = { path = "crates/..." }` 别名（`Cargo.toml:2-51,63-85`）
- 重型可选依赖 gate 在 Cargo feature 后，使轻量消费者可选。模式：`default = ["validation"]`；`archive = ["dep:zip","dep:tar","dep:flate2"]`；`http-reqwest = ["http","dep:reqwest","dep:tokio"]`（`crates/utils/Cargo.toml:9-15`）。需要网络传输的 MCP crate 应遵循 `ora-utils::http` / `http-reqwest` 拆分，而非无条件拉 reqwest
- 优先私有模块，显式导出公共 API（`AGENTS.md:27`）
- 不创建仅引用一次的小辅助方法（`AGENTS.md:28`）

### 8.2 Rust 编码标准

- 每个非平凡 fn 签名上方有文档注释描述用途；复杂逻辑/分支有行内注释。`new()` 等自明方法豁免。注释用英文（`AGENTS.md:3`）
- 解释"Why"而非"What"：注释覆盖设计理由、业务约束、权衡；结构/命名承载"What"（`AGENTS.md:4`）
- 为测试而设计：依赖注入、解耦组件、trait 接口、小纯函数（`AGENTS.md:5`）
- 优先静态分发：泛型 + trait bound 优于 `Box<dyn Trait>`，除非严格需要运行时多态（`AGENTS.md:6`）
- 使非法状态不可表示：用带关联数据的 `enum` 建模状态机，而非多 `Option` 字段的 struct（`AGENTS.md:7`）
- 无向后兼容：不留"以防万一"的兼容层；打破旧模式、移除废弃代码、旧的适应新的（`AGENTS.md:8`）
- 始终内联 `format!` 参数（clippy `uninlined_format_args = "deny"` 强制，`Cargo.toml:166`、`AGENTS.md:13,15`）
- 用 `&&` 合并可合并的条件折叠嵌套 `if`（`AGENTS.md:14`）
- 仅在参数上调用一个方法的闭包用方法引用替代（clippy `redundant_closure_for_method_calls` 强制，`Cargo.toml:163`、`AGENTS.md:16`）
- 避免布尔或歧义 `Option` 位置参数（`foo(false)`、`bar(None)`），优先 enum、命名方法、newtype（`AGENTS.md:17`）
- 无法改 API 时用 `/*param_name*/` 注释标记不透明字面量；参数名须精确匹配 callee 签名；string/char 字面量豁免（`AGENTS.md:18-21`）
- 尽可能穷尽 `match`，避免通配臂（`AGENTS.md:22`）
- 永不硬编码路径分隔符或手动拼接路径字符串：用 `Path`、`PathBuf`、`.join()`（`AGENTS.md:23`）
- 新 trait 须有文档注释解释角色及实现如何使用（`AGENTS.md:24`）
- 非测试代码中无 `unwrap()`/`expect()`（clippy `unwrap_used`/`expect_used` 禁止，`Cargo.toml:171,139`）

### 8.3 时间与日志

- 用本地时间而非 UTC（`AGENTS.md:36`）
- 用 `ora-logging` 包装宏——`ora_trace!`、`ora_debug!`、`ora_info!`、`ora_warn!`、`ora_error!`（`crates/logging/src/macros.rs:3,20,37,54,71`、`crates/logging/README.md:11`）——**而非** `tracing::` 宏（`AGENTS.md:37`）
- 用 `ora_logging::clock::now_local`，而非 `OffsetDateTime::now_local()`（`crates/logging/src/clock.rs:53`）。`ora-logging` 提供 `with_trace_logging`/`with_recorded_trace_logging` 测试辅助（`crates/logging/src/test_support.rs:15,22`、`crates/logging/README.md:30`）
- Cargo.toml 须依赖 `ora-logging = { workspace = true }`。注意 `tracing` 类型（span/Instrument）仍可使用；仅宏被替换

### 8.4 通用逻辑 → `ora-utils`

- 通用、无领域词汇的逻辑放 `ora-utils`（`crates/utils`），而非调用 crate；默认将候选逻辑放此处（`AGENTS.md:38`）
- 实现路径校验/归一化或压缩包提取前，优先 `ora-utils::path` 和 `ora-utils::archive`；若缺失则扩展 `ora-utils` 再消费（`AGENTS.md:39`）
- `ora-utils` 须保持叶子：无 `ora-*` 依赖，无领域词汇（`crates/utils/README.md:38-39`）
- 可用的 `ora-utils` 能力：`PortableRelativePath`/`StrictRelativePath`/`CanonicalPathRoot`（path）、zip-slip 防护的安全压缩包物化（archive feature）、流式 SHA-256（hash/validation）、`HttpDownload` trait + `LocalFileDownloader` + `ReqwestDownloader`（http/http-reqwest）、`Slug`、`GitBranchName`（`crates/utils/README.md:8-34`）
- MCP 插件做安装/下载须消费 `ora-utils::http::HttpDownload` 和 `ora-utils::archive`，如同 `ora-plugin-manager`（`crates/plugin-manager/README.md:17-19`、`crates/plugin-manager/Cargo.toml:15`），并在下载时校验 SHA-256（`crates/plugin-manager/README.md:18`）

### 8.5 模块 README

- `crates/` 下每个 crate 须在其 crate 根有英文 `README.md`——与 crate 同一变更添加（`AGENTS.md:43,47`）
- `src/` 下每个目录式生产模块须在模块目录有英文 `README.md`（`AGENTS.md:44`）。单文件模块不需要（由最近父覆盖）
- 测试/fixture/生成/示例/非生产目录**不需要** README（`AGENTS.md:45`）
- 例外：`crates/contracts`、`crates/domain`、`crates/pty` 及后代——类型定义+代码级文档是主要文档（`AGENTS.md:46`）
- 职责/边界/核心流/交互变更时更新对应 README——同一变更（`AGENTS.md:47`）
- README 内容为稳定事实：职责、非职责、公共边界、关键不变量、生命周期、失败语义、模块交互。本地理由/算法/数据结构选择/专用分支/性能权衡/临时约束放代码注释（`AGENTS.md:48`）

### 8.6 模块大小

- 目标 Rust 模块低于 500 LoC（不含测试，`AGENTS.md:31`）
- 超约 800 LoC 时在新模块而非扩展现有文件添加新功能，除非有强文档理由（`AGENTS.md:32-34`）
- 优先添加新模块而非增长现有模块（`AGENTS.md:30`）
- 从大模块提取代码时，将相关测试+模块/类型文档移向新实现，使不变量靠近拥有代码（`AGENTS.md:34-35`）

### 8.7 测试

- 用 `pretty_assertions::assert_eq` 获取更清晰 diff（workspace dep `pretty_assertions = "1"`，`Cargo.toml:102`、`AGENTS.md:102`）
- 优先深度相等比较，`assert_eq!` 整个对象而非逐字段（`AGENTS.md:103`）
- 测试中避免修改进程环境；从上方传入环境派生标志/依赖（`AGENTS.md:104`）
- 结构化日志测试：安装 test-scoped subscriber/dispatcher，`LevelFilter::TRACE`，通过 `tracing::subscriber::with_default`/`tracing::dispatcher::with_default` 限定到测试线程；所有触碰相同 callsite 的操作（setup、bootstrap、fixture、smoke check）都在其下——`tracing` 缓存 callsite interest，普通测试先触碰 callsite 可使后续断言不稳定。优先共享辅助 `with_trace_logging`/`with_recorded_trace_logging`（`AGENTS.md:105`）
- 迭代时运行最小相关 task；考虑 repo 级变更完成前运行完整 `task test`（`AGENTS.md:52-55`）

### 8.8 Taskfile 任务

- 格式化变更文件：`task format`（`Taskfile.yml:68-71`）
- 格式化所有 Rust：`task format:crates` → `cargo fmt --all`（`Taskfile.yml:90-94`）
- Rust lint：`task lint:crates` → `cargo fmt --all -- --check` + `cargo clippy --workspace -- -D warnings`，warning 为硬错误（`Taskfile.yml:114-119`）
- Rust 测试：`task test:crates`，前置需 `rg`（ripgrep）和 `deno` 在 PATH（`Taskfile.yml:143-147`）
- 全套：`task test` = `test:frontend` + `test:crates`（`Taskfile.yml:121-126`）
- 合约重新生成：`task export-contracts` → `cargo xtask export-contracts` + `pnpm --filter @ora/contracts generate:error-schema`（`Taskfile.yml:38-42`）

---

## 9. MCP 插件设计的缺口与影响

本节综合上述发现，提炼一个 MCP 插件需要但当前缺失或未连接的部分。MCP（Model Context Protocol）插件的典型形态是一个 stdio JSON-RPC 服务器，可能需要网络访问、子进程生成、以及暴露 tools/resources/prompts 给 host。

### 9.1 没有 MCP 插件 kind

`PluginKind` 仅有 `Workbench` 和 `Agent`（`crates/plugin-manifest/src/enums.rs:49-52`）。添加 MCP 插件需要新的 `Mcp` 变体——由于 `FromStr` 拒绝未知值（`enums.rs:75-83`），这会打破闭合的 v1 枚举。需协调修改三处：

- `ora-plugin-manifest`：添加 `PluginKind::Mcp` 变体并更新 `FromStr`/`as_str`（`enums.rs:49-91`）
- `ora-plugin-manager`：`PluginContribution` 需新变体（如 `Mcp(InstalledPluginMcp)`），`validate_contribution` 需新增分支（`validation.rs:169-183`）
- `crates/contracts/src/plugin.rs`：`InstalledPlugin` 的硬编码 `agent` 字段需重构为可选或枚举化的贡献字段（`plugin.rs:34-46`）

### 9.2 `Workbench` kind 无定义契约

存在 `defineAgent` 但无 `defineWorkbench`，无 workbench 方法集在任何规格中定义，host 侧仅实现 agent 契约校验（`crates/backend/src/agent_runtime/plugin_agent`）。`kind = "workbench"` 的插件可解析（`tests.rs:116-125`）但可注册什么除通用 `registerMethod` 机制外未定义。MCP 设计不应依赖 `workbench` 作为 kind 复用基础。

### 9.3 清单不携带运行时能力/权限模型

`permissions` 块（`specs/plugin/1-capability.md:82-171`）**不在** `PluginManifest` 中；`deny_unknown_fields` 会拒绝它。运行时能力完全通过 `ora/register` 的 `methods`/`emits` 运行时声明。一个需要 host 授予文件系统/网络/进程访问的 MCP 插件**今天没有清单字段声明它**。规格描述的权限模型需在 manifest 中建模（作为新字段或后继权限模型），并定义 MCP 特定权限存放位置。

### 9.4 反向请求/响应被故意移除

plugin-runtime 刻意拒绝任何携带 JSON-RPC `id` 的插件源消息（`crates/plugin-runtime/src/protocol.rs:94-97`）。MCP 协议中 server→host 的请求（如 sampling、elicitation）需要此能力，或需要不同的流量形态。设计文档明确说"no current plugin contract needs it"且"not reserving a pathway"（`plugin-agent-runtime.md:64-65,291-293`）。这是 MCP 插件设计须明确决策的设计缝。

**但 agent 桥接提供了替代模式**：ACP 通过通知透传绕过了反向请求的需求——host 是纯管道，内层协议拥有自己的 `id` 和关联。MCP 可遵循相同模式：将 MCP JSON-RPC 帧作为通知透传，让 MCP 自己的 id 机制处理关联，而非在 plugin-runtime 层引入反向请求。

### 9.5 设置管线完全未实现

从 `assets/config.json` 解析、`compile`/`resolve`、`store.json` 持久化到 OS 凭据存储集成——全部仅规格，零实现。需要配置（服务器 URL、API token、stdio 命令路径）的 MCP 插件没有设置基础设施可用。MCP 插件设计须决定是先构建设置管线还是用临时方案。

### 9.6 `PluginContribution` 是闭合枚举

`crates/plugin-manager/src/validation.rs:24-26`——仅 `Agent(InstalledPluginAgent)`。线缆合约 `InstalledPlugin`（`crates/contracts/src/plugin.rs:34-46`）硬编码 `agent` 字段。MCP 变体须在此处和合约中添加，且需设计前端如何路由 MCP 贡献而非 agent 贡献。

### 9.7 `PluginNamespace` 仅 `Official`

`crates/plugin-manifest/src/enums.rs:6-8`。第三方/社区 MCP 插件今天无法命名空间化，除非扩展枚举。安装路径也省略 namespace（`package_dir = …installed.join(manifest.name().as_str())`，`install.rs:71-74`），使用 `name` only，但发现按 `namespace/name` 去重。由于 namespace 固定为 `Official`，今天无冲突，但不同 namespace 的 MCP 插件会冲突或需路径方案变更。

### 9.8 固定入口 `main.js` 与仅 JS 运行时

`INSTALLED_ENTRYPOINT = "main.js"`（`validation.rs:10`），运行时启动 Deno（`crates/plugin-lifecycle/src/runtime.rs:30-76`）。MCP 插件通常是 stdio JSON-RPC 服务器（可能原生二进制），在此模型中无处安放——`PluginPackageType` 仅为 `Module`（`validation.rs:14-16`），`DenoPluginRuntimeLauncher` 以 `permissions: Vec::new()` 启动 `deno run`（`runtime.rs:65`）。若 MCP 服务器是 Deno 脚本则可复用；若是原生二进制则需新的 package type 和 launcher。

### 9.9 Deno 权限：agent 插件与零权限模型的矛盾

通用生命周期 launcher 硬编码零权限（`runtime.rs:66`），但 agent 插件路径授予 `--allow-run --allow-read --allow-env --allow-net`（`mod.rs:34-35`），使其与 host 同特权。MCP 插件不应需要 `--allow-run`（它不 spawn 子 CLI），因此如果 launcher 路径按 `kind` 参数化，它可以真正以零 Deno 权限运行。`PluginLifecycleConfig` 没有 permissions 字段（`lib.rs:28-31`），需第三条路径或泛化 launcher 以接受每 `kind` 的权限集。

### 9.10 `PluginRuntimeLauncher` 丢弃通知

`crates/plugin-lifecycle/src/runtime.rs:72`——生命周期适配器调用 `PluginRuntime::launch` 但以 `_notifications` 丢弃通知接收器。生命周期 crate 只管理进程启停，不管协议流量。需要向上游提供通知的 MCP 插件需不同接线路径——agent 路径完全绕过生命周期，通过后端 `plugin_agent::launch`（`mod.rs:66-96`）。MCP 插件很可能需要类似的专用桥接模块，而非复用通用生命周期 launcher。

### 9.11 `[[targets]]` 平台特定资产不支持

规格描述按 Rust target triple 键控的 `[[targets]]` 数组（`specs/plugin/3-registry.md:52-88`），manifest 无 `targets` 字段。含原生（每架构）二进制的 MCP 插件无法在当前 schema 中表达。索引也无 `release` 字段，UI 无法从索引判断插件是否可在当前平台安装。

### 9.12 SDK 无 host 调用面

插件只能 (a) 被 host 调用和 (b) 发送已声明通知。没有 `ora/request` 反向调用，也没有类型化的 host 方法客户端，尽管总览规格描述"Ora SDK 相当于系统调用接口"（`specs/plugin/0-overview.md:21`），插件可通过它请求 host 文件系统/网络/进程操作。该 host 调用 SDK 尚不存在于 `packages/plugin-sdk/src`。需通过 host 代理工具调用的 MCP 插件需设计此面。

### 9.13 规格与实现的方法名前缀矛盾

规格用 `ora/agent/start_agent` 等（`specs/plugin/4-agent.md:35-74`），实现用 `agent/start` 等（`agent.ts:4-7`、`control.rs:12-18`）。MCP 设计引用规格时须以 SDK + host 代码为权威线缆表面，不应传播此混淆。

### 9.14 两个独立关联表

`plugin-runtime` 有自己的 `PendingRequests`（`state.rs:25-69`，u64 id）和 `ora-acp` 有自己的 `PendingRequests`（`pending.rs:27-78`，`RequestId` 类型）。使用 MCP 自己的 JSON-RPC 关联的 MCP 插件会引入第三层。agent 设计的教训：不要双重关联——用通知让内层协议拥有自己的 id（`plugin-agent-runtime.md:221-229`、`crates/plugin-runtime/README.md:46-49`）。

### 9.15 桌面引导路由

桌面将每个发现的插件（全为 `Agent` kind）映射为 agent spec 并喂给后端（`apps/desktop/src-tauri/src/lib.rs:319-335`）。若发现 MCP 插件，桌面引导需将它们路由到 MCP supervisor 而非 agent supervisor。`agent_plugin_packages` 函数对 `PluginContribution::Agent(_)` 做穷尽 match（`lib.rs:322`），若添加 `Mcp` 变体须同时处理。

### 9.16 无 `CONTEXT.md` / `docs/adr/`

MCP 领域词汇（server、tool、resource、prompt、transport）无术语表归属。按 `docs/agents/domain.md:42-45`，发明不在术语表中的术语是触发 `/domain-modeling` 的信号。新 MCP 规格应放 `specs/` 下（如 `specs/mcp/`），`specs/drafts/` 是空暂存区。

### 9.17 摘要：MCP 插件落地须修改的最小变更集

| 变更                                  | 位置                                              | 性质                                    |
| ------------------------------------- | ------------------------------------------------- | --------------------------------------- |
| 新增 `PluginKind::Mcp`                | `crates/plugin-manifest/src/enums.rs:49-91`       | 扩展闭合枚举                            |
| 新增 `PluginContribution::Mcp`        | `crates/plugin-manager/src/validation.rs:24-26`   | 扩展闭合枚举                            |
| 新增 `validate_contribution` MCP 分支 | `crates/plugin-manager/src/validation.rs:169-183` | 新增 match 臂                           |
| 重构合约 `InstalledPlugin`            | `crates/contracts/src/plugin.rs:34-46`            | `agent` 硬编码字段 → 枚举化             |
| 新增 MCP 桥接模块                     | `crates/backend/src/agent_runtime/`               | 类似 `plugin_agent/` 的新模块           |
| 桌面引导路由 MCP 插件                 | `apps/desktop/src-tauri/src/lib.rs:319-335`       | 新增 MCP 包映射                         |
| MCP 协议方法集设计                    | `packages/plugin-sdk/src/`                        | 新增 `defineMcp` 或等效辅助             |
| `ora/register` 契约声明               | 运行时握手                                        | 定义 MCP 方法集与 emits 集              |
| （可选）清单权限模型                  | `crates/plugin-manifest/src/manifest.rs:249-281`  | 需放开 `deny_unknown_fields` 或分层解析 |
| （可选）设置管线实现                  | 新 crate 或扩展                                   | 全新建设                                |

---

_本文档基于 2026-08-21 的代码快照。所有引用均来自 `D:\project\desktop-mcp` 仓库的实际源文件。_
