# Ora 自动化测试与 AI 修复运行手册

## 目标与边界

Ora 把测试视为 AI 生成代码的可执行规格，而不是用测试数量替代人工审查。自动化负责尽早发现
确定性回归、保存可复现证据并提出最小修复；架构、安全边界、产品意图和最终合并仍由人负责。

桌面壳层 E2E 为了稳定，只验证 Tauri 窗口与打包后的前端，并通过 `desktop-e2e` feature 跳过
插件后端启动。真实 Bun、Windows Job、插件和 Web 后端生命周期由独立的夜间系统测试覆盖，二者
不能互相替代。

## 分层门禁

| 层级 | 本地入口 | CI | 失败证据 | 触发时机 |
| --- | --- | --- | --- | --- |
| 静态规则、生成漂移、单元/集成测试 | `task test:pr` | `Quality gates` | Nextest/组件 JUnit | 每个 PR、main |
| Rust/UI 覆盖率回归 | `task coverage:rust`、`task coverage:ui` | `Coverage gates` | LCOV、JSON | 每个 PR、main |
| Web 功能与 WCAG A/AA | `task test:web-e2e` | `Web end-to-end tests` | JUnit、trace、截图、视频 | 每个 PR、main |
| Windows 原生 Tauri 壳层 | `task test:desktop-e2e` | `Desktop end-to-end tests` | JUnit、宿主日志、失败截图 | 每个 PR、main |
| 依赖与源码安全 | `task audit` | `Security analysis` | audit/CodeQL 结果 | 每个 PR、main、每周 |
| 真实 Bun 与 Windows 进程树 | `task prepare-plugin-runtime && task test:system` | `Nightly full-system tests` | runtime log、asset manifest | 工作日夜间、手动 |
| 发布候选全门禁 | 对应上述命令 | `Release candidate gates` | 所有子工作流 artifact | `v*` tag、手动 |

`node scripts/collect-test-evidence.mjs` 会为当前存在的报告生成
`test-results/evidence-manifest.json`。清单记录 revision、相对路径、字节数、SHA-256，以及可解析的
JUnit/LCOV 汇总。收集器永远不掩盖原始测试退出码；缺失报告会进入 `missing` 字段。

## AI 自动修复闭环

`Codex auto-repair` 监听上述工作流的失败结果，执行一次有预算、可审计的修复尝试：

1. 只接受本仓库的受信任分支或同仓库 PR；fork PR 不会向 Codex 暴露密钥。
2. 只读 job 检出失败 revision，先安装依赖，再下载日志与 artifact。
3. Codex 只获得 `contents: read` 和 `workspace-write` 沙箱；日志被明确视为不可信数据。
4. Codex 复现最窄用例、提交最小代码与回归测试，但不能提交、推送或更改工作流与门禁阈值。
5. 只读 job 输出 binary patch 和修复报告。另一个没有 OpenAI API key 的写权限 job 校验并应用
   该 patch，创建中文 commit 和 draft PR。
6. `codex/auto-fix-*` 分支的再次失败不会触发第二轮修复；系统不自动合并。

自动 patch 的硬预算为 1 MiB、25 个文件和 1500 行变更；超过任一限制只保留修复报告，不创建 PR。

这是刻意的权限分离：持有模型密钥的 job 不能写仓库；能写仓库的 job 只能应用已保存的 patch，
且拿不到模型密钥。

## 仓库启用步骤

在 GitHub 仓库设置中完成以下配置后，AI 闭环才会启用：

1. 添加 Actions secret `OPENAI_API_KEY`。
2. 添加 Actions variable `CODEX_AUTO_REPAIR_ENABLED=true`。未配置时工作流安全地跳过。
3. 在 Actions 的 Workflow permissions 中启用“Allow GitHub Actions to create and approve pull
   requests”。工作流只创建 draft PR，不执行 approve；未启用时 patch artifact 仍会保留，但开 PR 失败。
4. 为 `main` 启用 branch protection，至少要求 Quality、Coverage、Web E2E、Desktop E2E 和
   Security checks；禁止自动修复 PR 绕过 review。
5. 对发布 tag 使用 Environment approval，并把 `Release candidate gates` 作为发布前置条件。

如果团队不使用 OpenAI API key，可保持 variable 未设置：全部确定性测试和证据链仍正常运行，失败
由人工或其他 agent 消费 artifact。官方 Codex Action 的模型/版本可由仓库管理员集中固定，但不要让
PR 内容控制 prompt、模型、sandbox 或 action 参数。

## 失败处理规则

- 先以测试名、错误类别和首个失败栈做 fingerprint；同一 revision 的同一工作流只保留一次修复。
- flaky 用例必须有可重复证据后才能隔离；不得通过 retry、延长 timeout 或降低覆盖率阈值掩盖问题。
- 修改公共 API 时同步更新 `docs/`；修改行为时优先增加回归测试并比较完整对象。
- Codex 无法在 Linux 复现 Windows-only 失败时，应依赖原始 Windows artifact，运行最近的确定性
  单元/集成门禁，并在 PR 中明确剩余风险。最终 Windows 门禁仍必须通过。
- 失败 artifact 保留 14 天，夜间与 Codex repair report 保留 30 天，patch 保留 7 天。重要发布证据
  应由发布流程另行长期归档。
