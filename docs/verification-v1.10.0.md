# v1.10.0 验证记录

适配 Codex Proxy RS `>=3.35.0,<3.36.0`，Linux x86_64、glibc 2.36+。SDK 和官方打包工具均固定至 `v3.35.0` 对应提交 `80f7d53085f5c9552e7e74b796a0bec50586ee1b`，继续使用 RPC v7、管理能力 v2、180 秒维护预算与既有私有状态 schema。

## 问题复现与修复

[CPR 3.35 管理响应解码](https://github.com/zyycn/codex-proxy-rs/blob/80f7d53085f5c9552e7e74b796a0bec50586ee1b/backend/crates/gateway-plugin/runtime/src/adapter/management/mod.rs)要求状态码为 200–599、内容类型符合注册声明、正文不超过 1 MiB。旧插件的状态码和 JSON 类型正确，但初始化、历史和进度会返回全部历史摘要，账户接口固定每批 500 条，均未限制编码后的正文大小。

用实际 1.9.0 发布二进制复现：

| 场景 | 旧版响应正文 | 结果 |
| --- | ---: | --- |
| 1,200 条带中文详情的历史，另有原始任务 | 2,968,400 字节 | 超过 1 MiB，复现宿主拒绝条件 |
| 500 个账户，其中 499 个为长中文名称 | 1,572,588 字节 | 超过 1 MiB，复现宿主拒绝条件 |

新版按 UTF-8 JSON 的实际字节数分页，单页预算为 768 KiB，所有管理响应另有 1 MiB 最终保护。页面自动合并历史、账户及模型列表，不截断或删除历史；分页快照包含完整响应摘要，数据变化时返回 409，由页面最多重新读取三次，避免混合不同版本。写操作不会因分页重试。异常超大的单条内容返回有界、可读的 413 JSON 错误。

最终安装包的上述两个场景均通过：初始化、历史、进度完整保留 1,201 条任务，账户完整保留 500 条；每个管理响应均经过与宿主相同的状态码、内容类型和 1 MiB 正文校验。另验证进度未变化时不返回列表、报告正文保留，以及删除历史后旧分页快照被拒绝。

## 单核构建

前端类型检查与生产构建、Rust 单元测试、Clippy、发布编译及官方打包工具编译均使用 `taskset -c 0` 绑定 CPU 0。设置 `CARGO_BUILD_JOBS=1`、`RAYON_NUM_THREADS=1`、`UV_THREADPOOL_SIZE=1`、`OPENBLAS_NUM_THREADS=1`、`OMP_NUM_THREADS=1`，Rust release 的 `codegen-units=1`。

使用 Rust 1.99.1、Node 24.22.0 与锁文件依赖；UI 组件库沿用宿主的 `@codex-proxy/ui` 0.7.0。SDK 提交之外无依赖升级。Python 评分代码和依赖未变，复用工作区已验证的私有 Python/NumPy 运行时。第三方许可内容核对相同。

```bash
# 激活上述工具链与单线程环境变量后：
(cd frontend && taskset -c 0 node node_modules/vue-tsc/bin/vue-tsc.js --noEmit \
  && taskset -c 0 node node_modules/vite/bin/vite.js build)
taskset -c 0 cargo test --manifest-path backend/Cargo.toml --locked --offline
taskset -c 0 cargo clippy --manifest-path backend/Cargo.toml --locked --offline -- -D warnings
taskset -c 0 cargo zigbuild --manifest-path backend/Cargo.toml --locked --offline \
  --release --target x86_64-unknown-linux-gnu.2.36
# 使用官方 v3.35.0 源码及锁文件编译打包工具。
taskset -c 0 cargo build --manifest-path /path/to/codex-proxy-rs/backend/Cargo.toml \
  -p codex-proxy-plugin-cli --release --locked
taskset -c 0 cpr-plugin package --manifest plugin.json \
  --binary backend/target/x86_64-unknown-linux-gnu/release/modeltrace-health \
  --target x86_64-unknown-linux-gnu --resource-map web=frontend/dist --output-dir dist
```

Rust 发布编译耗时 3 分 12 秒，官方 CLI 编译耗时 1 分 49 秒，前端构建约 3.5 秒、处理 2,159 个模块。发布链接仅显示既有的优化参数弃用提示；最高导入 GLIBC 符号版本为 2.34。

## 验证结果

- 最终安装包全部 73 项 RPC 回归通过，耗时 513.126 秒，覆盖 121 秒长请求、16 路并发、取消、结果保存与恢复、账户操作、指纹库更新，以及新增的大列表与快照变更场景。
- Rust 格式检查、20 项单元测试及 Clippy `-D warnings` 通过。
- TypeScript 类型检查、前端生产构建通过。
- 前端请求回归通过：三类列表、中文数据、查询参数保留、快照变化后重读、重试次数限制、异常分页游标及 POST 错误不重试。
- Python 评分 6 项单元测试通过。
- 最终包及所有资源摘要、RPC v7、宿主兼容范围、18 个模型离线校验、实际插件解包与评分通过。
- 1.9.0 → 1.10.0 升级验证通过，保留配置、计划、历史、指纹库与提交幂等，不重发检测请求。
- 浏览器使用最终安装包生成的模拟数据与生产前端，分页合并、筛选、批量操作、配置、报告、实时进度、取消、移动布局及隔离页面回归通过，页面错误为 0。

安装包：`dist/personal.modeltrace-health-1.10.0-x86_64-unknown-linux-gnu.tar.gz`，30,782,226 字节。

SHA-256：`14c77baf90ed46143dc2690d4f7e15f8a35cb853b4c4ae647d1fc5afc7d120f1`。

```bash
taskset -c 0 node scripts/api_smoke.mjs
.venv/bin/python scripts/verify_artifact.py dist/personal.modeltrace-health-1.10.0-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/upgrade_smoke.py \
  dist/personal.modeltrace-health-1.9.0-x86_64-unknown-linux-gnu.tar.gz \
  dist/personal.modeltrace-health-1.10.0-x86_64-unknown-linux-gnu.tar.gz
# 将安装包解至 .cache/release-v1.10.0 后：
.venv/bin/python scripts/protocol_smoke.py --binary .cache/release-v1.10.0/bin/modeltrace-health -v
.venv/bin/python scripts/ui_fixture.py --binary .cache/release-v1.10.0/bin/modeltrace-health
PLAYWRIGHT_BROWSERS_PATH=.cache/browser-bin node scripts/ui_smoke.mjs
```

## 验证边界

集成使用实际发布二进制、生产前端与模拟宿主，不访问真实账号，未在生产 CPR 宿主安装。浏览器首次启动被沙箱禁止创建子进程，随后在允许启动浏览器的环境重跑。

## 历史验证

[1.9.0](verification-v1.9.0.md)、[1.8.2](verification-v1.8.2.md)、[1.8.1](verification-v1.8.1.md)、[1.8.0](verification-v1.8.0.md)。
