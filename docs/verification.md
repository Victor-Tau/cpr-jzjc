# v1.19.0 验证记录

适配 CPR `>=3.43.0,<3.44.0`，Linux x86_64、glibc 2.36+。SDK 与官方打包工具固定至 v3.43.0 提交 `402bc37fbb5c83378cc83cec89e9fa251e4a99c1`。

## 合同核对

对照 v3.42.0 与 v3.43.0 官方源码，插件 SDK 生产代码与官方 CLI 保持一致；SDK 文档补充管理页面外链能力，本插件未调用。账户选择与管理、运维错误日志接口保持兼容，继续读取 `/api/admin/operations/errors`。检测请求继续绑定提交账户。

保留 RPC v7、管理能力 v2、180 秒维护预算、UI 0.8.0 和既有私有状态 schema。新配置字段由默认值补齐，历史报告与活动任务的配置快照也可读取。

## 行为变更

`failureRetries` 允许 0–99 次额外请求，默认 4；请求拒绝、普通网络故障、无效样本及超时均使用此预算。原先认证拒绝直接终止和连续两次宿主预算超时提前终止的规则，改为遵循配置。每次维护对单个任务最多推进一次采样，重试次数在重启后保留。

`requestRejectedAction` 为 `continue`（默认）或 `disable`。请求拒绝且重试耗尽后，前者保留账户调度供后续自动检测使用，后者复用现有停用操作与身份核验。失败状态保持 `failed/inconclusive`，不会标记为降智或触发移组；停用操作失败可单独重试，不重新发送探针。普通网络故障和本地持久化上限不触发此项停用。

高重试次数可能增大任务记录；回答或记录达到既有持久化上限时归类为 `persistence_limit`，直接终止并保留账户调度，避免把本地存储限制当成请求拒绝。

## 构建与验证

使用 Rust 1.99.1、Node.js 24.22.2、pnpm 9.12.2、Zig 0.15.2；发布二进制与同版官方 CLI 使用单核编译，`CARGO_BUILD_JOBS=1`。Python/NumPy 依赖未变，从校验通过的 1.18.0 包复用运行时，并刷新评分与上游源码。

- Rust 格式检查、Clippy `--all-targets -- -D warnings` 与 26 项单元测试通过。
- 前端锁文件离线安装、TypeScript 类型检查及生产构建通过；Python 评分 6 项单元测试与分页 API 回归通过。
- 最终安装包摘要、全部资源摘要、RPC v7、宿主范围、18 个模型离线校验、Rust 运行时解包与完整评分通过。ELF 所需 GLIBC 符号最高为 2.34。
- 最终安装包二进制的 82 项 RPC 场景通过：全量分三组执行，更新两项旧超时断言后定向复验通过。覆盖 0/2/99 次重试、16 路超时按配置终止、认证拒绝后继续定时检测或停用、身份变化保护、停用失败单独重试、配置快照与重启恢复，以及任务记录存储上限。
- 1.18.0 → 1.19.0 升级验证通过，保留配置、计划、历史、指纹库和幂等提交，新字段补齐默认值，无探针重放。
- 浏览器回归通过，页面错误为 0；覆盖 0/99 次重试与处理选项保存、导出、导入、恢复默认值，以及筛选、报告、进度、明暗主题和移动布局。设置页截图已更新。

安装包：`dist/personal.modeltrace-health-1.19.0-x86_64-unknown-linux-gnu.tar.gz`，30,832,608 字节。

SHA-256：`001daccef4151825db2dbd37973babb81c1fe19dc9a6ec6a77c5158e5388e37e`。

## 复现命令

```bash
cargo fmt --manifest-path backend/Cargo.toml --check
cargo clippy --manifest-path backend/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path backend/Cargo.toml --locked
pnpm --dir frontend install --offline --frozen-lockfile
pnpm --dir frontend build
.venv/bin/python -m unittest discover -s engine/tests -v
node scripts/api_smoke.mjs
CARGO_BUILD_JOBS=1 RAYON_NUM_THREADS=1 taskset -c 0 cargo zigbuild \
  --manifest-path backend/Cargo.toml --locked --release --target x86_64-unknown-linux-gnu.2.36
cpr-plugin package --manifest plugin.json \
  --binary backend/target/x86_64-unknown-linux-gnu/release/modeltrace-health \
  --target x86_64-unknown-linux-gnu --resource-map web=frontend/dist --output-dir dist
.venv/bin/python scripts/verify_artifact.py dist/personal.modeltrace-health-1.19.0-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/upgrade_smoke.py \
  /path/to/personal.modeltrace-health-1.18.0-x86_64-unknown-linux-gnu.tar.gz \
  dist/personal.modeltrace-health-1.19.0-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/protocol_smoke.py --binary .cache/pkg-v1.19.0/bin/modeltrace-health -v
.venv/bin/python scripts/ui_fixture.py --binary .cache/pkg-v1.19.0/bin/modeltrace-health
LD_LIBRARY_PATH=/home/tau/.local/opt/chromium-libs/usr/lib/x86_64-linux-gnu \
  UI_DIST=.cache/pkg-v1.19.0/web node scripts/ui_smoke.mjs
```

官方 CLI 的构建命令及缓存、工具环境准备见[开发与构建](development.md)。

## 验证边界

使用最终发行包二进制、生产前端与模拟宿主验证，未在生产 CPR 宿主安装，未访问真实账号。浏览器截图使用模拟数据。

## 历史验证

[1.18.0](verification-v1.18.0.md)、[1.17.0](verification-v1.17.0.md)、[1.16.0](verification-v1.16.0.md)。
