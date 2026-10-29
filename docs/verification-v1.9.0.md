# v1.9.0 验证记录

适配 Codex Proxy RS `>=3.33.0,<3.34.0`，Linux x86_64、glibc 2.36+。SDK 和官方打包工具均使用 `v3.33.0` 对应提交 `8c8618aa0f841638262262e3338c115bc075f125`，RPC 升级为 v7；管理能力仍为 v2、维护预算仍为 180 秒，私有状态 schema 不变。

## 单核构建

前端类型检查与生产构建、Rust 测试与发布编译、官方打包工具编译均通过 `taskset -c 0` 绑定 CPU 0。Rust 使用 `CARGO_BUILD_JOBS=1`、`RAYON_NUM_THREADS=1`，前端使用 `UV_THREADPOOL_SIZE=1`；评分测试使用 `OPENBLAS_NUM_THREADS=1`、`OMP_NUM_THREADS=1`。沿用 Rust 1.99.1、Node 24.22.0 与锁文件依赖，更新 SDK 提交及 UI 组件库 `@codex-proxy/ui` 0.7.0 和其新增依赖；复用工作区已有私有 Python/NumPy 运行时，评分代码和依赖未变。

```bash
# 环境需具备 README 中的依赖与单线程环境变量。
(cd frontend && taskset -c 0 node node_modules/vue-tsc/bin/vue-tsc.js --noEmit \
  && taskset -c 0 node node_modules/vite/bin/vite.js build)
taskset -c 0 cargo test --manifest-path backend/Cargo.toml --locked --offline
taskset -c 0 cargo zigbuild --manifest-path backend/Cargo.toml --locked --offline \
  --release --target x86_64-unknown-linux-gnu.2.36
# cpr-plugin 使用 CPR v3.33.0 的源码编译。
taskset -c 0 cargo build --manifest-path /path/to/codex-proxy-rs/backend/Cargo.toml \
  -p codex-proxy-plugin-cli --release --locked
cpr-plugin package --manifest plugin.json \
  --binary backend/target/x86_64-unknown-linux-gnu/release/modeltrace-health \
  --target x86_64-unknown-linux-gnu --resource-map web=frontend/dist --output-dir dist
```

## 验证结果

- 前端类型检查与生产构建通过，Vite 处理 2,159 个模块。
- Rust 格式检查、17 项单元测试和 `cargo clippy -- -D warnings` 通过。
- Rust 发布编译耗时 3 分 07 秒，官方打包工具编译耗时 1 分 50 秒；发布链接仅显示既有的优化参数弃用提示。
- Python 评分 6 项单元测试通过。
- 安装包及资源摘要、RPC v7、宿主兼容范围、18 个模型离线校验、实际插件解包和评分通过。
- 全部 71 项 RPC 回归通过，耗时 521.302 秒，覆盖长请求（含 121 秒响应）、16 路并发、取消、保存确认超时恢复、账户操作和指纹库更新。
- RPC v7 调度专项通过，实际观察并验证 `suspend`、`resume` / `resumed`、`callback_consumed` 消息、递增 epoch、回调父调用关联和结果回收。
- 1.8.2 → 1.9.0 升级回归通过：配置、计划、历史、指纹库保留，无探针重发。
- 浏览器回归通过，涵盖默认每页 100 条、结果筛选、批量操作、配置、报告、进度、取消、移动布局、深浅主题及宿主隔离页面；页面错误为 0。浏览器使用模拟数据。

安装包：`dist/personal.modeltrace-health-1.9.0-x86_64-unknown-linux-gnu.tar.gz`，30,773,742 字节。

SHA-256：`dae205f053b8cb1e3b35f7c534a099265e0194e2479d87270f17b4a96f5cdb14`。

包验证、升级和 RPC 回归可通过以下入口复现：

```bash
.venv/bin/python scripts/verify_artifact.py dist/personal.modeltrace-health-1.9.0-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/upgrade_smoke.py \
  dist/personal.modeltrace-health-1.8.2-x86_64-unknown-linux-gnu.tar.gz \
  dist/personal.modeltrace-health-1.9.0-x86_64-unknown-linux-gnu.tar.gz
# 将安装包解压至 .cache/release-v1.9.0 后运行：
.venv/bin/python scripts/protocol_smoke.py --binary .cache/release-v1.9.0/bin/modeltrace-health -v
.venv/bin/python scripts/ui_fixture.py --binary .cache/release-v1.9.0/bin/modeltrace-health
PLAYWRIGHT_BROWSERS_PATH=.cache/browser-bin node scripts/ui_smoke.mjs
```

## 验证边界

集成验证使用实际发布二进制、生产前端及模拟宿主，不访问真实账号，未部署生产宿主。

## 历史验证

[1.8.2](verification-v1.8.2.md)、[1.8.1](verification-v1.8.1.md)、[1.8.0](verification-v1.8.0.md)。
