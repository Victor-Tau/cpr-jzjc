# v1.28.0 验证记录

适配 CPR `>=3.53.1,<3.54.0`，Linux x86_64 / ARM64（aarch64）、glibc 2.36+。SDK 与官方打包工具固定至 CPR v3.53.1 提交 `188a6817d141a8d9a1a0bceead4f07477d35276e`。

## 合同核对

对照 [CPR v3.52.2 与 v3.53.1 官方源码](https://github.com/zyycn/codex-proxy-rs/compare/v3.52.2...v3.53.1)：

- `gateway-plugin/sdk/src`、`gateway-plugin/runtime/src`、官方 CLI 的 `src` 和 `plugin-host-compatibility.json` 均无差异；继续使用 manifest v2、RPC v7、管理能力 v2、维护能力 v1 和 180 秒维护预算。
- 3.53.1 新增的 `errorOrigin` 筛选属于用量请求记录；插件仍读取 `/api/admin/operations/errors`，其查询和返回的账户、状态码、请求来源字段保持兼容。
- OpenAI 账户选择器调整了候选加载流程，但 `required_account` 仍作为固定账户优先传入 `load_candidate_accounts`，排队换号路径仍排除固定账户。检测请求继续绑定提交的账户。
- 管理 API 的校验错误文案细化不改变插件使用的账户列表、详情、分组选项和批量更新协议。SDK 和插件业务代码无需迁移，前端继续使用 UI 0.8.0。
- 私有状态 schema 未变；本次仅更新插件版本、宿主范围、SDK 固定提交及相关文档。

## 构建与验证

使用 Rust 1.99.1、Node.js 24.22.3、pnpm 11.21.0、Zig 0.15.2 和 cargo-zigbuild 0.24.1。Rust 发布构建及同版官方 CLI 使用单核、`CARGO_BUILD_JOBS=1`。从摘要及资源摘要校验通过的上游 1.27.0 x86_64 包复用 Python/NumPy 运行时并刷新源码。

- Rust 格式检查、26 项单元测试及 Clippy（`--all-targets --all-features -- -D warnings`）通过。
- 前端锁文件安装、TypeScript 检查和生产构建通过；6 项 Python 评分测试、5 项运行时目标测试及分页接口回归通过。
- x86_64 最终包摘要、全部资源摘要、目标架构、RPC v7、18 个模型离线校验、Rust 运行时解包和完整评分通过。ELF 引用的 GLIBC 符号最高版本为 2.34。
- 1.27.0 → 1.28.0 升级验证通过：配置、计划、检测历史和指纹库保留，提交幂等性保留，无检测请求重放。
- 使用最终包二进制生成模拟数据，包内前端通过 39 组浏览器检查，页面错误为 0。
- 最终包二进制的 82 项 RPC 回归全部通过；分为 28/27/27 项运行，耗时分别为 160.896/210.804/84.323 秒。覆盖账户绑定、失败重试和拒绝处理、日志触发、长响应与预算、并发、取消及重启恢复。

本地 x86_64 验证包为 `personal.modeltrace-health-1.28.0-x86_64-unknown-linux-gnu.tar.gz`，30,832,833 字节，SHA-256 为 `40b9ebcb5a1b67568b5b6576f45c941efcbf5b4ddccb14950f4c827b11bd60f6`。

## 复现命令

使用 Node.js >=24、已激活的 Python 虚拟环境和构建工具，在仓库根目录执行：

```bash
cargo fmt --manifest-path backend/Cargo.toml --check
cargo test --manifest-path backend/Cargo.toml --locked
cargo clippy --manifest-path backend/Cargo.toml --locked --all-targets --all-features -- -D warnings
pnpm --dir frontend install --frozen-lockfile
pnpm --dir frontend build
python -m unittest discover -s engine/tests -v
python -m unittest discover -s scripts/tests -v
node scripts/api_smoke.mjs
python scripts/install_packager.py
python scripts/refresh_runtime.py /path/to/personal.modeltrace-health-1.27.0-x86_64-unknown-linux-gnu.tar.gz
CARGO_BUILD_JOBS=1 taskset -c 0 cargo zigbuild --manifest-path backend/Cargo.toml --locked --release --target x86_64-unknown-linux-gnu.2.36
.cache/tools/bin/cpr-plugin package --manifest plugin.json --binary backend/target/x86_64-unknown-linux-gnu/release/modeltrace-health --target x86_64-unknown-linux-gnu --resource-map web=frontend/dist --output-dir dist
python scripts/verify_artifact.py dist/personal.modeltrace-health-1.28.0-x86_64-unknown-linux-gnu.tar.gz
python scripts/upgrade_smoke.py /path/to/personal.modeltrace-health-1.27.0-x86_64-unknown-linux-gnu.tar.gz dist/personal.modeltrace-health-1.28.0-x86_64-unknown-linux-gnu.tar.gz
mkdir -p .cache/pkg-v1.28.0
tar -xzf dist/personal.modeltrace-health-1.28.0-x86_64-unknown-linux-gnu.tar.gz -C .cache/pkg-v1.28.0
python scripts/protocol_smoke.py --binary .cache/pkg-v1.28.0/bin/modeltrace-health -v
python scripts/ui_fixture.py --binary .cache/pkg-v1.28.0/bin/modeltrace-health
PLAYWRIGHT_HOST_PLATFORM_OVERRIDE=ubuntu24.04-x64 PLAYWRIGHT_BROWSERS_PATH="$PWD/.cache/browser-bin" UI_DIST=.cache/pkg-v1.28.0/web node scripts/ui_smoke.mjs
```

## 验证边界

本地构建与集成验证面向 Linux x86_64，使用实际插件进程与模拟宿主，不访问真实账户，也未在生产 CPR 3.53.1 实例部署。ARM64 沿用原生 CI 构建与测试工作流，本机未执行 ARM64 二进制。发布附件及摘要以之后的 Release 为准。

## 历史验证

[1.27.0](verification-v1.27.0.md)、[1.26.0](verification-v1.26.0.md)、[1.25.0](verification-v1.25.0.md)、[1.24.0](verification-v1.24.0.md)、[1.23.0](verification-v1.23.0.md)、[1.22.0](verification-v1.22.0.md)。
