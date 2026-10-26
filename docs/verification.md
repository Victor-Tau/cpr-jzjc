# v1.8.0 验证记录

适配 Codex Proxy RS `>=3.32.0,<3.33.0`，Linux x86_64、glibc 2.36+。SDK 和打包工具固定到官方 `v3.32.0` 标签对应提交 `6a5300a7b5c649f2a6d548df70ccdd58d15e0025`。安装包使用 RPC v6；管理能力 v2、维护能力 v1、私有状态 schema 和评分算法保持不变。

## 30 秒超时结论

旧的维护预算截断已解决，需要同时升级 CPR 宿主及本插件。CPR 3.32.0 将默认维护预算从 30 秒改为 120 秒，并移除 SDK 统一 120 秒截断。本插件通过 `PluginSession::set_call_budgets` 显式声明维护阶段 180,000 毫秒，宿主和 SDK 从同一份声明解析预算；执行器通过 `HostClient::remaining_budget()` 获取实际剩余时间，不再从初始上下文重新起算。

选择 180 秒是为了给默认 120 秒模型请求预留准备、并行结果保存和评分时间。仅采用宿主默认的 120 秒维护预算，模型请求仍会被准备与收尾时间压缩。请求实际期限仍取配置超时与本次剩余维护预算扣除收尾时间后的较短值；0 只关闭插件请求超时，不能无限等待。

实际发布二进制在模拟宿主下通过两项长请求回归：

- 4 个账户并行，模型结果统一延迟 31 秒返回；全部在同次维护回调完成评分，每账户只请求一次，诊断中的请求窗口约 120 秒，无超时、无主动关闭。
- 请求超时配置为 150 秒，模型结果延迟 121 秒返回；同次回调正常完成评分，只请求一次，证明 SDK 不再在 120 秒统一截断。

维护回调结束后约 30 秒再次调度的间隔仍存在，与单次处理预算不同。独立网络期限、上游错误、主动取消、停用或替换实例及进程重启仍可能提前终止请求；长预算不提供跨重启续跑。有效数字不足、上游凭据失效等问题也不能仅靠增加预算保证解决。已经结束的旧失败或中断任务需重新提交。

宿主维护调度源码保留未被替换的实例任务，无关配置发布只合并通知；替换实例前先等待旧调用取消确认或进程退出。此项为官方源码核对，未在生产宿主复现验证。

依据：[官方 3.32.0 发布说明](https://github.com/zyycn/codex-proxy-rs/releases/tag/v3.32.0)、[SDK 预算声明合同](https://github.com/zyycn/codex-proxy-rs/blob/6a5300a7b5c649f2a6d548df70ccdd58d15e0025/backend/crates/gateway-plugin/sdk/README.md#声明调用预算)、[维护任务生命周期](https://github.com/zyycn/codex-proxy-rs/blob/6a5300a7b5c649f2a6d548df70ccdd58d15e0025/backend/crates/gateway-plugin/runtime/src/generation/prepare/maintenance.rs)。

## 构建与安装包

前端、Rust 插件和 CPR CLI 构建均通过 `taskset -c 0` 绑定 CPU 0，设置 `CARGO_BUILD_JOBS=1`、`RAYON_NUM_THREADS=1`。使用 Rust 1.99.1、Node 24.22.0，依赖锁文件固定。

```bash
taskset -c 0 env CARGO_BUILD_JOBS=1 RAYON_NUM_THREADS=1 \
  cargo zigbuild --manifest-path backend/Cargo.toml --locked --offline \
  --release --target x86_64-unknown-linux-gnu.2.36
```

CPR CLI 从 3.32.0 官方源码的 `backend/apps/plugin-cli` 构建，使用官方 workspace 锁文件。Python/NumPy 运行时由 `refresh_runtime.py` 从已验证包与全部资源摘要的 1.7.3 安装包复用，算法和依赖没有变化。第三方许可条目及正文核对相同，保留原排序。

交付文件：`dist/personal.modeltrace-health-1.8.0-x86_64-unknown-linux-gnu.tar.gz`，30,763,588 字节。

SHA-256：`7288ee9d7b85de8c466b00a9ccaf62771ae488a9baacb952c1a73e76a107d64c`。

运行时归档包含 1,303 个文件，展开 90,710,161 字节，SHA-256 `f57f34194ad4dd313bb0942d23d213970b9bba96c1cd4145a94b07db8d4ec37e`。

## 验证结果

| 验证 | 结果 |
| --- | --- |
| Rust 单元测试 | 17 项通过 |
| Python 评分单元测试 | 6 项通过 |
| 静态检查 | `cargo fmt --check`、Clippy `-D warnings`、前端类型检查通过 |
| 前端生产构建 | Vite 构建通过，2,158 个模块 |
| 安装包校验 | 协议 v6、宿主范围、包和全部资源摘要、18 个模型离线校验、实际插件解包与评分通过 |
| RPC 回归（安装包二进制） | 70 项均通过，包含 68 项原有用例和 2 项新增长请求用例 |
| 升级回归 | 1.7.3 → 1.8.0 通过，配置、计划、历史和指纹库保留，无探针重发 |
| 浏览器回归 | 既有 Playwright 检测、配置、取消、报告、移动布局与深浅主题用例通过，无页面错误；设置页截图已更新 |
| 受限解包 | strace 拒绝时间戳系统调用后，发布二进制仍完成解包、检测和评分 |

RPC 回归分为 34 + 34 + 2 项三批运行。第一批 34 项通过（186.223 秒），第二批原有用例中注册用例首次因模拟宿主仍向注册阶段传入 30 秒而被新版 SDK 拒绝；改为合同规定的 5 秒后该项单独重跑通过（1.161 秒），其余 33 项通过。新增长请求批次 2 项通过（156.679 秒）。旧回归继续显式使用 30 秒或更短预算，以覆盖预算不足、部分结果、并行收尾、重复超时终止、取消与重启恢复。

新增长请求可单独复现（替换为解包后的二进制路径）：

```bash
.venv/bin/python scripts/protocol_smoke.py --binary /path/bin/modeltrace-health \
  Integration.test_declared_budget_allows_parallel_responses_after_thirty_seconds \
  Integration.test_declared_budget_allows_response_after_old_sdk_cap -v
```

完整回归：`.venv/bin/python scripts/protocol_smoke.py --binary /path/bin/modeltrace-health -v`。

Zig 链接保留既有优化参数弃用提示，构建成功。测试使用实际插件进程与模拟宿主、模型流和私有状态，不发送真实账户模型请求；本次未在生产 CPR 宿主安装或切换版本。

## 历史验证

[1.7.3](verification-v1.7.3.md)、[1.7.2](verification-v1.7.2.md)、[1.7.1](verification-v1.7.1.md)、[1.7.0](verification-v1.7.0.md)。
