# v1.12.0 验证记录

适配 CPR `>=3.37.2,<3.38.0`，Linux x86_64、glibc 2.36+。SDK 与官方打包工具固定至 v3.37.2 提交 `641f43b3030b1c68961e05d525e58ec6f4264a15`，UI 组件库升级至 0.8.0。继续使用 RPC v7、管理能力 v2、180 秒维护预算与原有私有状态 schema。

## 适配核对

对比 CPR v3.36.0 与 v3.37.2 的 SDK、管理适配器和官方 CLI：SDK 新增独立的 `ModelPresentation.reasoning_summary` 能力，插件不注册模型适配目录，无需新增该声明；管理与维护协议继续兼容。新版主题桥传递无前缀的语义 token，并按页面声明的前缀注入变量。自定义样式从 `--cp-*` 迁移至默认的 `--z-*`，UI Vite 插件在最终页面声明 `codex-proxy-ui-theme-prefix=z`。浏览器脚本使用公开的 `createTheme`、`createThemeVars` API 构造宿主主题数据。

## 单核构建

官方 CLI、前端类型检查及生产构建、Rust 单元测试、Clippy 与发布编译全部绑定 CPU 0（`taskset -c 0`）。设置 `CARGO_BUILD_JOBS=1`、`RAYON_NUM_THREADS=1`、`UV_THREADPOOL_SIZE=1`、`OPENBLAS_NUM_THREADS=1`、`OMP_NUM_THREADS=1`，Rust release 使用 `codegen-units=1`。

使用 Rust 1.99.1、Node 24.22.0、pnpm 10.32.1、Zig 0.13.0，沿用锁文件，Rust 仅变更插件版本与 SDK 提交。官方 CLI 使用上游 workspace 锁文件单核编译。Python/NumPy 依赖未变，从已校验的 1.11.1 包复用运行时并刷新源码与许可。发布编译耗时 2 分 24 秒，链接仅显示既有优化参数弃用提示。

## 验证结果

- 最终安装包完成 73 项 RPC 场景：首次全量运行耗时 532.254 秒，70 项通过；3 项旧调度用例断言在 1.11.1 上同样失败，修正测试前提后单独重跑 3 项通过。覆盖长请求、16 路并发、取消、保存恢复、账户操作、指纹库更新与大列表分页。
- Rust 格式检查、21 项单元测试与 Clippy `-D warnings` 通过。
- TypeScript 类型检查、前端生产构建及分页接口回归通过。
- Python 评分 6 项单元测试通过。
- 最终安装包摘要、全部资源摘要、RPC v7、宿主范围、18 个模型离线校验、Rust 运行时解包与完整评分通过。
- 1.11.1 → 1.12.0 升级验证通过，保留配置、计划、历史和指纹库，无检测请求重放。
- 浏览器使用最终包生成的模拟数据与生产前端，筛选、批量操作、设置、报告、实时进度、明暗主题、移动布局和隔离页面全部通过，页面错误为 0。主题验证检查最终页面的 `z` 前缀声明、深色状态与实际计算后的文字颜色。
- 最高导入 GLIBC 符号版本为 2.34。

安装包：`dist/personal.modeltrace-health-1.12.0-x86_64-unknown-linux-gnu.tar.gz`，30,788,966 字节。

SHA-256：`8a3beb5ee7ae5bcdabad50ffd51621e385153a2e5118f96d546ff3fafb207862`。

```bash
source .cache/env-v173.sh
taskset -c 0 cargo test --manifest-path backend/Cargo.toml --locked --offline
taskset -c 0 cargo clippy --manifest-path backend/Cargo.toml --locked --offline -- -D warnings
taskset -c 0 cargo zigbuild --manifest-path backend/Cargo.toml --locked --offline --release --target x86_64-unknown-linux-gnu.2.36
taskset -c 0 .venv/bin/python scripts/verify_artifact.py dist/personal.modeltrace-health-1.12.0-x86_64-unknown-linux-gnu.tar.gz
taskset -c 0 .venv/bin/python scripts/upgrade_smoke.py dist/personal.modeltrace-health-1.11.1-x86_64-unknown-linux-gnu.tar.gz dist/personal.modeltrace-health-1.12.0-x86_64-unknown-linux-gnu.tar.gz
taskset -c 0 .venv/bin/python scripts/protocol_smoke.py --binary .cache/release-v1.12.0/bin/modeltrace-health -v
```

## 回归脚本维护

浏览器断言同步 1.11 起的“定时”来源文案，切换设置页后等待控件可见。手动去重用例原先在维护回调结束后断言拒绝提交，此时任务可能已完成；旧版 1.11.1 与新版均能复现该断言失败。修正为任务仍排队时检查拒绝重复提交，保留回调后幂等重放、定时任务并存与 nextDue 不变的检查，单项重跑通过。另一个公平调度用例原先忽略定时任务在首次回调中已完成的一轮，改为检查共享回调前后的轮数与请求数增量；旧版也能复现原断言失败，来源取消用例同样在任务已完成后检查取消，改用需要复测的配置确保两类任务保持活动，并检查取消手动任务不改变定时任务状态。三项修正用例重跑通过。

## 验证边界

集成使用完整发布包二进制、生产前端与模拟宿主，不访问真实账号，未在生产 CPR 宿主安装。浏览器在允许启动 Chromium 的环境执行。

## 历史验证

[1.10.0](verification-v1.10.0.md)、[1.9.0](verification-v1.9.0.md)、[1.8.2](verification-v1.8.2.md)、[1.8.1](verification-v1.8.1.md)、[1.8.0](verification-v1.8.0.md)。
