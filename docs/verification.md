# v1.7.1 验证记录

适配 Codex Proxy RS 3.30.2，修复维护回调取消后的检测停滞，并将检测历史改为保留最近三天。

适配 Codex Proxy RS `>=3.30.2,<3.31.0`，Linux x86_64，glibc 2.36+。

交付文件：`dist/personal.modeltrace-health-1.7.1-x86_64-unknown-linux-gnu.tar.gz`，30,745,300 字节，SHA-256 `5d352d71eee62aef8d55353bb0729755b5b9cd96e04222969b404ce3b19d4084`。

## 实现要点

- **取消恢复**：维护和管理 RPC 使用取消守卫。处理 future 被宿主或 SDK 丢弃时，下一次调用会强制重新读取持久化状态；已发出但结果未收齐的 `pendingProbe` 标记为 `interrupted`，不永久跳过，也不盲目重发模型请求。未完成的幂等账户操作恢复为 `pending`，下一回调先校验再执行。
- **预算收尾**：并行批次按并发数预留收尾时间（单任务约 0.3 秒，16 路约 3 秒），先把同批全部探针结果校验并保存，再使用剩余预算评分。评分或保存撞上截止时间时保留已落盘样本，下一回调优先评分，不重发请求。单任务请求窗口约 29 秒，16 路并发约 26.5 秒。
- **历史保留**：检测历史固定保留最近 3 天，不再限制 500 条；旧配置的 `retentionDays` 在载入和保存时归一化为 3。12 MiB 历史存储保护、单条 256 KiB 上限和任务 manifest 的 2,000 条上限保留。
- **移组与操作超时**：移组前先读取当前分组，已在目标分组时直接记为完成；移组或关闭调度在回调剩余预算不足时保持 `pending`，下一回调幂等重试，不再误报失败。
- **前端进度**：使用 `updatedAt` 判断进度是否失效。`receiving` 超过 8 秒无更新显示「进度已过期，等待后台恢复」，其他阶段超过 90 秒无更新也会提示。
- **宿主适配**：SDK 固定到 Codex Proxy RS 3.30.2 的提交 `76291f448cdf1d1da2027da32b7c949deb998c7a`，打包使用同版 `cpr-plugin`，安装包为协议 v5。UI 组件包升级到 `@codex-proxy/ui` 0.6.0，并使用 `ZConfigProvider` + `zhHans` 保持中文文案。

## 构建约束

全部编译命令通过 `taskset -c 0` 绑定到一个 CPU 核心，同时设置 `CARGO_BUILD_JOBS=1`、`RAYON_NUM_THREADS=1`。前端使用 Node 24 和 pnpm 单核构建；发布二进制使用 `cargo zigbuild` 面向 `x86_64-unknown-linux-gnu.2.36`。运行时归档沿用 1.7.0 的私有 Python/NumPy 运行时，未重新下载或改动。

单核发布构建耗时 1 分 37 秒；前端生产构建耗时约 2.4 秒，转换 2,158 个模块，`app.js` 215.15 kB。

## 验证结果

| 验证 | 结果 |
| --- | --- |
| Rust 单元测试 | 17 项通过（`cargo test --locked --offline`，单核） |
| Python 评分单元测试 | 6 项通过（项目私有 Python 环境，含 NumPy） |
| 静态检查 | `cargo fmt --check`、Clippy `-D warnings`、TypeScript `vue-tsc --noEmit` 通过 |
| 前端生产构建 | Vite 构建通过 |
| RPC 回归（debug 二进制） | 58 项全部通过，235.9 秒 |
| RPC 回归（安装包二进制） | 58 项全部通过，284.7 秒 |
| 安装包校验 | `verify_artifact.py` 通过：SHA-256 附件一致、manifest/协议 v5/资源路径与摘要核对、包内私有 Python/NumPy 离线评分校验、正式二进制实际解包并完成一轮检测评分 |
| 升级回归 | 1.7.0 → 1.7.1 通过：配置、计划、历史、指纹库和提交去重保留；`retentionDays=47` 归一化为 3；未重发探针 |
| UI 交互回归 | 通过：账户与历史批量操作、配置、移组重试、报告、移动端/深色主题、实时进度、停顿进度失效提示等；无页面错误 |

安装包校验输出：`runtimeValidated=true`、`pluginExtractionValidated=true`、指纹库 18 个模型。

## 超时专项验证

- **维护取消恢复**：两路并发时，把第二个账户 `pendingProbe=true` 的状态写入延迟到 8 秒预算之外。维护调用返回超时；下一次回调重新载入状态，两个结果不确定的任务均变为 `interrupted`，`pendingProbe=false`，模型执行次数没有增加。
- **16 路接近期限完成 + 存储变慢**：16 个账户并发，模型首包响应统一延迟 25 秒，每个样本保存额外延迟 80 毫秒。维护回调在 30 秒内完成；16 个任务均只发起一次请求，全部保存结果并最终完成，没有 `pendingProbe` 残留，也没有重发。该用例覆盖了诊断中的 16 路并发、临期收尾和存储变慢组合。
- **历史保留**：构造超过 500 条、跨越 3 天边界的历史记录，验证只按时间清理、不按固定条数截断；旧 `retentionDays` 值在升级后归一化为 3。
- **目标分组幂等**：账户已在目标分组时，维护回调不重复调用更新分组接口，直接完成移组。
- **慢操作重试**：账户操作跨过回调预算时保留 `pending`，下一回调完成，且不会产生重复宿主更新。

新增或更新的 RPC 用例包括 `test_cancelled_maintenance_recovers_pending_probes`、`test_sixteen_way_wave_finishes_near_deadline_with_slow_storage`、`test_history_keeps_three_days_without_count_limit`、`test_already_in_target_group_skips_redundant_move`、`test_slow_move_defers_to_next_callback_without_duplicate_update`。测试总数由 1.7.0 的 54 项增加到 58 项。

UI 回归在 Playwright 的 Ubuntu 24.04 Chromium 回退构建上执行，包含 `stale receiving progress expires` 检查；页面错误为空。

没有在生产宿主安装插件或发送真实账号检测请求。

## 历史验证

[1.7.0](verification-v1.7.0.md)、[1.6.1](verification-v1.6.1.md)、[1.6.0](verification-v1.6.0.md)、[1.5.1](verification-v1.5.1.md)、[1.5.0](verification-v1.5.0.md)、[1.4.6](verification-v1.4.6.md)。
