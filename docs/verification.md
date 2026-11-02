# v1.13.0 验证记录

适配 CPR `>=3.38.1,<3.39.0`，Linux x86_64、glibc 2.36+。SDK 与官方打包工具固定至 v3.38.1 提交 `e0b7a841ca074f17e11dad73f39a159c75cb2fb1`。

## 适配核对

对比 CPR v3.37.2 与 v3.38.1 的官方源码：SDK 的 `call` 协议类型、能力文档、模型执行回调和官方 CLI 源码保持一致，客户端流读取相关变化为模块重组；宿主插件运行时重组 generation、RPC 流与状态 schema 校验代码。插件继续使用 RPC v7、管理能力 v2、180 秒维护预算和原有私有状态 schema。UI 最新发行仍为 0.8.0，继续使用 `z` 主题变量。

CPR 3.38.1 加强原生 Responses 请求的账号模型资格检查。插件继续使用宿主模型执行链路，按账户 ID 固定检测目标，由宿主检查 Key、模型目录与账户权限；未绕过新版调度规则。插件不实现 CPR Link 快照协议，无需引入 Link v2 字段转换。

## 单核构建

官方 CLI、前端构建、Rust 检查与发布编译、Python 和 RPC 回归、浏览器验证全部绑定 CPU 0（`taskset -c 0`），重型步骤串行执行，使用 `nice -n 10` 与空闲 I/O 优先级。设置 `CARGO_BUILD_JOBS=1`、`RAYON_NUM_THREADS=1`、`UV_THREADPOOL_SIZE=1`、`OPENBLAS_NUM_THREADS=1`、`OMP_NUM_THREADS=1`、`RUST_TEST_THREADS=1`，Rust release 使用 `codegen-units=1`。

使用 Rust 1.99.1、Node 24.22.0、pnpm 10.32.1、Zig 0.13.0。Rust 锁文件仅变更插件版本与 SDK 提交，前端依赖及锁文件保持原版本，使用已有依赖离线校验锁文件后构建。官方 CLI 使用 CPR v3.38.1 的 workspace 锁文件离线编译。Python/NumPy 依赖未变，从已校验的 1.12.0 包复用运行时并刷新源码与许可。

## 验证结果

- Rust 格式检查、21 项单元测试与 Clippy `-D warnings` 通过。
- TypeScript 类型检查、前端生产构建及分页接口回归通过。
- Python 评分 6 项单元测试通过。
- 最终安装包摘要、全部资源摘要、RPC v7、宿主范围、18 个模型离线校验、Rust 运行时解包与完整评分通过。
- 1.12.0 → 1.13.0 升级验证通过，保留配置、计划、历史和指纹库，无检测请求重放。
- 最高导入 GLIBC 符号版本为 2.34。
- 最终安装包的 73 项 RPC 集成场景全部通过，耗时 511.115 秒，覆盖 121 秒长请求、16 路并发、取消与重启、手动和定时任务公平调度、账户操作重试、指纹库更新与大列表分页。
- 浏览器使用最终包生成的模拟数据与生产前端，账户和历史筛选、批量提交、设置与备份、报告、指纹库更新、实时进度、明暗主题、移动布局和隔离页面全部通过，页面错误为 0。

发布编译耗时 1 分 54 秒，链接仅显示 Zig 既有优化参数弃用提示。安装包：`dist/personal.modeltrace-health-1.13.0-x86_64-unknown-linux-gnu.tar.gz`，30,790,101 字节。

SHA-256：`a43f2477da05d4659a49b1105930a4f347d6b6665cc2de6cf7c5eba8e6d62dc9`。

```bash
source .cache/env-v173.sh
export RUST_TEST_THREADS=1
taskset -c 0 cargo test --manifest-path backend/Cargo.toml --locked --offline
taskset -c 0 cargo clippy --manifest-path backend/Cargo.toml --locked --offline -- -D warnings
taskset -c 0 cargo zigbuild --manifest-path backend/Cargo.toml --locked --offline --release --target x86_64-unknown-linux-gnu.2.36
taskset -c 0 .venv/bin/python scripts/verify_artifact.py dist/personal.modeltrace-health-1.13.0-x86_64-unknown-linux-gnu.tar.gz
taskset -c 0 .venv/bin/python scripts/upgrade_smoke.py dist/personal.modeltrace-health-1.12.0-x86_64-unknown-linux-gnu.tar.gz dist/personal.modeltrace-health-1.13.0-x86_64-unknown-linux-gnu.tar.gz
taskset -c 0 .venv/bin/python scripts/protocol_smoke.py --binary .cache/release-v1.13.0/bin/modeltrace-health -v
```

## 验证边界

集成验证使用最终发布包二进制、生产前端与模拟宿主，不访问真实账号，未在生产 CPR 宿主安装。浏览器使用模拟数据，在允许启动 Chromium 的环境执行。

## 历史验证

[1.12.0](verification-v1.12.0.md)、[1.10.0](verification-v1.10.0.md)、[1.9.0](verification-v1.9.0.md)、[1.8.2](verification-v1.8.2.md)。
