# v1.20.0 验证记录

适配 CPR `>=3.44.0,<3.45.0`，Linux x86_64、glibc 2.36+。SDK 与官方打包工具固定至 v3.44.0 提交 `c622f085748f812936a7371fcc85d1621fa72ca8`。

## 合同核对

对照 v3.43.0 与 v3.44.0 官方 SDK，新增代理目录查询与按代理 ID 请求能力，现有调用保持兼容。官方 CLI 源码未变，使用 v3.44.0 workspace 锁文件重新构建。插件继续使用 RPC v7、管理能力 v2、180 秒维护预算、UI 0.8.0 和既有私有状态 schema。

## 构建与验证

使用 Rust 1.99.1、Node.js 24.22.2、pnpm 9.12.2 和 Zig；发布二进制使用单核构建。SDK 与版本之外的 Rust 锁定依赖未变。私有 Python/NumPy 从摘要及资源校验通过的 1.19.0 安装包复用，刷新评分源码并重新收集许可。

- Rust 格式检查、Clippy `--all-targets -- -D warnings` 和 26 项单元测试通过。
- 前端锁文件离线安装、TypeScript 类型检查与生产构建通过；Python 评分 6 项单元测试和分页 API 回归通过。
- 最终安装包摘要与资源摘要、RPC v7、宿主范围、18 个模型离线校验、Rust 运行时解包及完整评分通过。
- 最终安装包二进制的 82 项 RPC 场景全部通过，覆盖账户绑定、重试、并发预算、超时、身份校验、状态恢复与指纹库更新。
- 1.19.0 → 1.20.0 升级验证通过，保留配置、计划、历史和指纹库，无探针重放。
- 最终安装包前端浏览器回归通过，页面错误为 0，覆盖设置、报告、进度、筛选、明暗主题及移动布局。

安装包：`dist/personal.modeltrace-health-1.20.0-x86_64-unknown-linux-gnu.tar.gz`，30,832,939 字节。

SHA-256：`746f1cc4a30567fb47a293c355b01cdc0dc453f533684941ebbb66411ba3ca3b`。

## 复现命令

```bash
cargo fmt --manifest-path backend/Cargo.toml --check
cargo clippy --manifest-path backend/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path backend/Cargo.toml --locked
pnpm --dir frontend install --offline --frozen-lockfile
pnpm --dir frontend build
.venv/bin/python -m unittest discover -s engine/tests -v
node scripts/api_smoke.mjs
.venv/bin/python scripts/verify_artifact.py dist/personal.modeltrace-health-1.20.0-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/upgrade_smoke.py dist/personal.modeltrace-health-1.19.0-x86_64-unknown-linux-gnu.tar.gz dist/personal.modeltrace-health-1.20.0-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/protocol_smoke.py --binary .cache/pkg-v1.20.0/bin/modeltrace-health -v
.venv/bin/python scripts/ui_fixture.py --binary .cache/pkg-v1.20.0/bin/modeltrace-health
UI_DIST=.cache/pkg-v1.20.0/web node scripts/ui_smoke.mjs
```

构建命令与工具准备见[开发与构建](development.md)。浏览器在沙箱外运行本地模拟页面。

## 验证边界

使用最终发行包二进制、生产前端与模拟宿主验证，未在生产 CPR 宿主安装，未访问真实账户。浏览器截图使用模拟数据。

## 历史验证

[1.19.0](verification-v1.19.0.md)、[1.18.0](verification-v1.18.0.md)、[1.17.0](verification-v1.17.0.md)、[1.16.0](verification-v1.16.0.md)。
