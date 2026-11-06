# v1.16.0 验证记录

适配 CPR `>=3.40.0,<3.41.0`，Linux x86_64、glibc 2.36+。SDK 与官方打包工具固定至 v3.40.0 提交 `789609dd53663b323e43567d2476f7f83b6b77a1`。

## 合同核对

对照 v3.39.1 与 v3.40.0 官方源码，新增用量扫描与统计读取回调不影响本插件使用的接口；管理、维护、账户、模型及私有状态合同保持兼容。运维错误日志公开字段保持兼容，继续读取 `/api/admin/operations/errors`。保留 RPC v7、管理能力 v2、180 秒维护预算及既有私有状态 schema，未修改检测业务或页面交互。

## 构建

使用 Rust 1.99.1、Node.js 24.22.2、pnpm 9.12.2、Zig 0.15.2。发布二进制及官方 CLI 编译绑定 CPU 0，`CARGO_BUILD_JOBS=1`，release 使用 `codegen-units=1`。CLI 从 CPR v3.40.0 源码以官方 workspace 锁文件构建，构建目录复用 `.cache/codex-proxy-rs-v3.39.1/backend/target`，实际源码来自 v3.40.0。

Python/NumPy 依赖未变，从摘要及全部资源校验通过的 1.15.0 安装包复用运行时，刷新评分与上游源码；重新收集第三方许可证，内容不变。

## 验证结果

- Rust 格式检查、Clippy `--all-targets -- -D warnings` 与 25 项单元测试通过。
- 前端锁文件安装、TypeScript 类型检查及生产构建通过；Python 评分 6 项单元测试与分页接口回归通过。
- 最终安装包摘要、全部资源摘要、RPC v7、宿主范围、18 个模型离线校验、Rust 运行时解包与完整评分通过。二进制 GLIBC 符号最高版本为 2.34。
- 最终安装包二进制的 75 项 RPC 集成场景全部通过，分三组各 25 项并行执行，耗时分别为 140.715、237.193、159.816 秒；覆盖请求绑定、复测、日志触发、去重与冷却、取消、并发预算、保存与重启恢复。
- 1.15.0 → 1.16.0 升级验证通过，保留配置、计划、历史及指纹库，提交幂等性保留，无检测请求重放。
- 浏览器使用最终包生成的模拟数据与生产前端，39 组检查通过，页面错误为 0，覆盖账户筛选、历史、参数保存、日志触发、报告、指纹库、进度、明暗主题及移动布局。

安装包：`dist/personal.modeltrace-health-1.16.0-x86_64-unknown-linux-gnu.tar.gz`，30,831,522 字节。

SHA-256：`3a61afed1cea4e74431d940c1120d55bd1e82d91972433ef9e00c7ed14ad8cad`。

## 复现命令

```bash
cargo fmt --manifest-path backend/Cargo.toml --check
cargo clippy --manifest-path backend/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path backend/Cargo.toml --locked
pnpm --dir frontend install --frozen-lockfile
pnpm --dir frontend build
.venv/bin/python -m unittest discover -s engine/tests -v
node scripts/api_smoke.mjs
CARGO_BUILD_JOBS=1 RAYON_NUM_THREADS=1 taskset -c 0 cargo zigbuild \
  --manifest-path backend/Cargo.toml --locked --release --target x86_64-unknown-linux-gnu.2.36
CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR="$PWD/.cache/codex-proxy-rs-v3.39.1/backend/target" \
  taskset -c 0 cargo build --manifest-path .cache/codex-proxy-rs-v3.40.0/backend/Cargo.toml \
  -p codex-proxy-plugin-cli --release --locked
.cache/codex-proxy-rs-v3.39.1/backend/target/release/cpr-plugin package \
  --manifest plugin.json --binary backend/target/x86_64-unknown-linux-gnu/release/modeltrace-health \
  --target x86_64-unknown-linux-gnu --resource-map web=frontend/dist --output-dir dist
.venv/bin/python scripts/verify_artifact.py dist/personal.modeltrace-health-1.16.0-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/upgrade_smoke.py \
  dist/personal.modeltrace-health-1.15.0-x86_64-unknown-linux-gnu.tar.gz \
  dist/personal.modeltrace-health-1.16.0-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/protocol_smoke.py --binary .cache/pkg-v1.16.0/bin/modeltrace-health -v
.venv/bin/python scripts/ui_fixture.py --binary .cache/pkg-v1.16.0/bin/modeltrace-health
LD_LIBRARY_PATH=/home/tau/.local/opt/chromium-libs/usr/lib/x86_64-linux-gnu node scripts/ui_smoke.mjs
```

## 验证边界

集成验证使用最终安装包二进制、生产前端与模拟宿主，未在生产 CPR 宿主安装，也未访问真实账号。浏览器使用模拟数据。

## 历史验证

[1.14.0](https://github.com/Victor-Tau/cpr-jzjc/blob/v1.14.0/docs/verification.md)、[1.13.0](verification-v1.13.0.md)、[1.12.0](verification-v1.12.0.md)、[1.10.0](verification-v1.10.0.md)、[1.9.0](verification-v1.9.0.md)、[1.8.2](verification-v1.8.2.md)。
