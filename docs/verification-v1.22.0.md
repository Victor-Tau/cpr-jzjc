# v1.22.0 验证记录

适配 CPR `>=3.45.1,<3.46.0`，Linux x86_64 / ARM64（aarch64）、glibc 2.36+。SDK 与官方打包工具固定至 v3.45.1 提交 `431992a7a43be32a0fad2fbc8a554732096fa299`。

## 合同核对

对照 v3.44.0 与 v3.45.1 官方源码：

- SDK 新增 Client Key 显式预算窗口（`host.keys.get_budget_window` / `set_budget_window`），宿主设置字段 `account_warmup_schedule_time` 改为 `account_warmup_cron`，并新增周额度重置相关字段。插件不使用这些接口，编译通过即确认无破坏。
- 插件包协议、manifest、RPC v7、管理能力 v2 和官方 CLI 源码未变；CLI 使用 v3.45.1 workspace 锁文件重新构建。
- 插件读取的管理接口 `/api/admin/accounts`、`/api/admin/accounts/detail`、`/api/admin/accounts/batch-update`、`/api/admin/account-groups/options` 与 `/api/admin/operations/errors` 只新增字段，现有请求和读取字段保持兼容。
- 账户 `status` 新增 `quota_paused`。宿主对非诊断请求排除该状态账户，插件绑定账户的模型请求同样适用。插件页面新增「额度暂停」显示及筛选，后端筛选按状态字符串匹配，无需改动。

插件继续使用 180 秒维护预算、UI 0.8.0 和既有私有状态 schema。

## x86_64 本地构建与验证

使用 Rust 1.99.1、pnpm 9.12.2、Python 3.11 和 Zig，通过 `scripts/package.sh` 完整构建；发布二进制与官方 CLI 均在单核下构建。私有运行时由固定基础镜像与 wheel 构建，许可证重新收集后与 1.21.0 一致。

- Rust 格式检查、Clippy `--all-targets -- -D warnings` 和 26 项单元测试通过。
- 前端锁文件安装、TypeScript 类型检查与生产构建通过；Python 评分 6 项单元测试、构建脚本 5 项防错测试和分页 API 回归通过。
- 最终安装包摘要与资源摘要、manifest 与源码一致性、目标架构、RPC v7、18 个模型离线校验、Rust 运行时解包及完整评分通过。
- 最终安装包二进制的 82 项 RPC 场景全部通过。
- 1.21.0 → 1.22.0 升级验证通过，保留配置、计划、历史和指纹库，无探针重放。
- 最终安装包前端浏览器回归通过，页面错误为 0；模拟数据包含 `quota_paused` 账户。

本地安装包：`dist/personal.modeltrace-health-1.22.0-x86_64-unknown-linux-gnu.tar.gz`，30,832,899 字节，SHA-256 `7052a75b15f35084abe8ac4eb9cb19495b0d651210d6ae64c037919ed85abeb3`。Release 附件由 CI 构建，摘要以 Release 中的 `.sha256` 为准。

## 双架构 CI

GitHub Actions 在 `ubuntu-24.04`（x86_64）与 `ubuntu-24.04-arm`（ARM64）原生 runner 的 Debian 12 / Python 3.11.17 容器内，分别执行完整 `scripts/package.sh`、最终包校验和 82 项 RPC 回归。标签发布等待两个任务成功、核对四个文件及摘要后上传 Release。

## 复现命令

```bash
python scripts/install_packager.py
bash scripts/package.sh
python scripts/protocol_smoke.py --binary .cache/pkg-v1.22.0/bin/modeltrace-health -v
python scripts/upgrade_smoke.py dist/personal.modeltrace-health-1.21.0-x86_64-unknown-linux-gnu.tar.gz dist/personal.modeltrace-health-1.22.0-x86_64-unknown-linux-gnu.tar.gz
python scripts/ui_fixture.py --binary .cache/pkg-v1.22.0/bin/modeltrace-health
UI_DIST=.cache/pkg-v1.22.0/web node scripts/ui_smoke.mjs
```

构建命令与工具准备见[开发与构建](development.md)。

## 验证边界

使用最终安装包二进制、生产前端与模拟宿主验证，未在生产 CPR 宿主安装，未访问真实账户。ARM64 安装包由 CI 构建和验证，未在真实 ARM64 CPR 宿主上安装。

## 历史验证

[1.21.0](verification-v1.21.0.md)、[1.20.0](verification-v1.20.0.md)、[1.19.0](verification-v1.19.0.md)、[1.18.0](verification-v1.18.0.md)、[1.17.0](verification-v1.17.0.md)、[1.16.0](verification-v1.16.0.md)。
