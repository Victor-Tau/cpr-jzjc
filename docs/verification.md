# v1.14.0 验证记录

适配 CPR `>=3.38.1,<3.39.0`，Linux x86_64、glibc 2.36+。SDK 与官方打包工具仍固定至 v3.38.1 提交 `e0b7a841ca074f17e11dad73f39a159c75cb2fb1`。

## 问题复现与根因核对

对照 CPR v3.38.1 源码确认：`/api/admin/usage/records` 列表视图（`UsageListRecordView`）不含 HTTP 状态字段，`errorCode` 映射自 `provider_error_code`（如 `server_is_overloaded`）；`OpenAI upstream failure: kind=…, status=503` 是 Provider 诊断信息，写入请求的 `error_message` 与运维事件的 `message`。换号重试成功的中间失败只写入 `ops_events`。运维错误列表 `/api/admin/operations/errors` 合并请求错误与运维事件，提供 `upstreamStatusCode`、`clientStatusCode`、`message`、`requestKind` 与 `subagentKind`，插件子请求标记为 `plugin_bound_model`/`plugin_child_model` 并带插件实例 ID。

## 构建

发布二进制编译与官方 CLI 打包绑定 CPU 0（`taskset -c 0`、`nice -n 10`、`CARGO_BUILD_JOBS=1`），Rust release 使用 `codegen-units=1`；回归测试在其余核心并行执行。使用 Rust 1.99.1、Node 24.22.2、pnpm 9.12.2、Zig 0.15.2，官方 CLI 由 CPR v3.38.1 源码编译。Rust 与前端锁文件仅变更插件版本，依赖不变。Python/NumPy 依赖未变，从已校验摘要的 1.13.0 发布包复用运行时并刷新评分与上游源码；第三方许可证条目与 1.13.0 相同。

## 验证结果

- Rust 格式检查、Clippy `-D warnings` 与 25 项单元测试通过；新增用例覆盖诊断信息 `status=503` 解析、CPR 3.38.1 运维错误条目、插件自身请求识别、冷却配置边界与旧配置补齐。
- TypeScript 类型检查、前端生产构建、Python 评分 6 项单元测试与分页接口回归通过。
- 最终安装包摘要、全部资源摘要、RPC v7、宿主范围、18 个模型离线校验、Rust 运行时解包与完整评分通过；最高导入 GLIBC 符号版本为 2.34。
- 最终安装包二进制的 75 项 RPC 集成场景全部通过，耗时 534.163 秒。新增场景覆盖仅在诊断信息中出现 503 的运维事件触发检测、账户 ID 与邮箱匹配、去重、冷却、插件自身请求与本插件请求 ID 排除、日志读取失败提示与恢复、停用后重新开启的读取窗口，以及按 `error_code` 来源停止队列。
- 1.13.0 → 1.14.0 升级验证通过，保留配置、计划、历史和指纹库，无检测请求重放。
- 浏览器使用最终包生成的模拟数据与生产前端，39 组检查全部通过，页面错误为 0：账户与历史筛选、统计卡片、日志触发状态（未开启、监听、命中、读取错误）、日志触发来源标签与停止日志触发队列、结论去重显示、报告摘要与触发来源、降级模型高亮、设置分区与保存、备份导入、指纹库更新、实时进度、明暗主题、移动布局和隔离页面。

安装包：`dist/personal.modeltrace-health-1.14.0-x86_64-unknown-linux-gnu.tar.gz`，30,825,120 字节。

SHA-256：`1478a3759674e0fe5ca3950e8a2e7d16ce5a73b4b4d687a6dcfa0fcd64656eb1`。

```bash
export RUST_TEST_THREADS=1
taskset -c 0 cargo zigbuild --manifest-path backend/Cargo.toml --locked --release --target x86_64-unknown-linux-gnu.2.36
cpr-plugin package --manifest plugin.json --binary backend/target/x86_64-unknown-linux-gnu/release/modeltrace-health \
  --target x86_64-unknown-linux-gnu --resource-map web=frontend/dist --output-dir dist
.venv/bin/python scripts/verify_artifact.py dist/personal.modeltrace-health-1.14.0-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/upgrade_smoke.py dist/personal.modeltrace-health-1.13.0-x86_64-unknown-linux-gnu.tar.gz dist/personal.modeltrace-health-1.14.0-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/protocol_smoke.py --binary .cache/release-v1.14.0/bin/modeltrace-health -v
.venv/bin/python scripts/ui_fixture.py --binary .cache/release-v1.14.0/bin/modeltrace-health
PLAYWRIGHT_BROWSERS_PATH=.cache/browser-bin node scripts/ui_smoke.mjs
```

## 测试修正

`test_restart_replays_receipt_and_saved_sample_without_model_request` 的 `sample` 子场景新建模拟宿主后未像其他用例一样预热评分运行时。发布二进制首次解包运行时后按设计将首个请求推迟到下一次维护，导致该子场景在发布二进制上固定失败（已发布的 1.13.0 安装包同样复现，开发构建使用本地 Python 不受影响）。现补齐预热，插件代码未因此修改；1.13.0、1.14.0 发布二进制及开发构建各重复 3 次均通过。

## 验证边界

集成验证使用最终发布包二进制、生产前端与模拟宿主；运维错误日志由模拟宿主按 CPR v3.38.1 字段返回。未在生产 CPR 宿主安装，也未访问真实账号。浏览器使用模拟数据。

## 历史验证

[1.13.0](verification-v1.13.0.md)、[1.12.0](verification-v1.12.0.md)、[1.10.0](verification-v1.10.0.md)、[1.9.0](verification-v1.9.0.md)、[1.8.2](verification-v1.8.2.md)。
