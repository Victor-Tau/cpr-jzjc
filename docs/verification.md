# v1.4.5 验证记录

适配 Codex Proxy RS `>=3.29.1,<3.30.0`，Linux x86_64，glibc 2.36+。

交付文件：`dist/personal.modeltrace-health-1.4.5-x86_64-unknown-linux-gnu.tar.gz`，30,677,936 字节。

SHA-256：`25ed3fca955a35190d658a6ad7ffe5ff232c2ce9bd6d629045714a427422778f`。

## 改进内容

手动与自动任务分别按「账户 + 来源」去重，同一账户可以同时保留两类活动任务。自动任务不会禁用手动检测按钮或批量选择，手动任务不会使自动计划跳过该账户。两类就绪任务在维护回调中交替执行，各任务独立保留采样、重试、复测与报告。执行仍受宿主维护调用预算约束。

新增「停止手动队列」和「停止自动队列」，按来源取消任务，进度缓存中的即时取消与持久任务取消使用相同来源条件，避免运行期间误停另一类任务。取消不改变自动计划；原 API 省略来源与任务 ID 时仍可停止全部任务。

账户列表和检测历史在检测时间后显示「（手动）」或「（自动）」。顶部「下次检测」只显示自动计划的 `nextDue`，不再附加来源，也不混入手动任务的执行、重试或复测时间。关闭自动计划时显示「—」。

插件 ID、状态 namespace 和 schema 保持不变。原实例升级保留配置、历史、指纹库与自动计划；旧任务 ID 仍支持原提交的幂等重试。

## 验证结果

| 验证 | 结果 |
| --- | --- |
| Rust 单元测试 | 11 项通过 |
| Python 单元测试 | 6 项通过 |
| 调试二进制 RPC 集成 | 38 项通过，175.711 秒 |
| 发布二进制 RPC 集成 | 38 项通过，185.804 秒 |
| 真实版本升级 | 1.3.0→1.4.5、1.4.3→1.4.5 通过；配置、配置版本、下次自动执行时间、报告、指纹库保留，旧提交重试返回原任务，无探针重放 |
| 浏览器交互 | 25 项通过，无页面 JavaScript 异常；覆盖时间后的来源标记、自动检测期间手动提交、分别停止队列、较新的自动结果不解锁仍有活动手动任务的账户，以及移动端、深色主题、实时进度和配置备份等既有交互 |
| 静态与构建 | Rust fmt、Clippy `-D warnings`、TypeScript 和 Vite 生产构建通过；单核 Zig glibc 2.36 发布构建完成，最高导入 GLIBC 符号版本为 2.34 |
| 安装包 | 包摘要、全部资源摘要、离线 Python/NumPy 评分、实际插件冷启动解包与完整检测流程通过，内置 18 个候选模型 |

新增 RPC 用例覆盖两个方向的交叉提交、相同请求字符串的来源隔离、同来源重复提交拒绝、两类任务交替执行、来源取消不影响另一队列或自动计划，以及探针执行期间取消另一来源。

测试使用模拟宿主与模型流，没有向真实账户发送检测请求。浏览器运行实际前端构建，使用模拟宿主数据；Chromium 在沙箱外执行，解决当前环境对浏览器启动系统调用的限制。前端使用现有锁定依赖，Node 22.22.1 直接执行 vue-tsc 与 Vite 完成验证。

Python/NumPy 运行时未修改，SHA-256 为 `a8e41e162360f8cbe7d352adbdd702ad93ce50a381fd00a90e6d4ca6cc52d980`。

## 复现

```bash
cargo test --manifest-path backend/Cargo.toml --locked
cargo clippy --manifest-path backend/Cargo.toml --locked --all-targets -- -D warnings
.venv/bin/python -m unittest discover -s engine/tests -v
.venv/bin/python scripts/verify_artifact.py dist/personal.modeltrace-health-1.4.5-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/protocol_smoke.py --binary /path/to/extracted/bin/modeltrace-health -v
.venv/bin/python scripts/upgrade_smoke.py dist/personal.modeltrace-health-1.4.3-x86_64-unknown-linux-gnu.tar.gz dist/personal.modeltrace-health-1.4.5-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/ui_fixture.py
PLAYWRIGHT_BROWSERS_PATH=.cache/browser-bin node scripts/ui_smoke.mjs
```

构建和浏览器环境准备见 [README](../README.md#开发与验证)。界面截图：[账户列表](screenshots/accounts.png)、[自动检测来源与独立手动队列](screenshots/detection-source.png)、[历史中的手动与自动时间标记](screenshots/history.png)、[参数配置](screenshots/settings.png)、[报告](screenshots/report.png)、[手机深色主题](screenshots/mobile.png)。

历史验证：[1.4.3](verification-v1.4.3.md)、[1.4.2](verification-v1.4.2.md)、[1.4.1](verification-v1.4.1.md)、[1.4.0](verification-v1.4.0.md)、[1.3.0](verification-v1.3.0.md)。
