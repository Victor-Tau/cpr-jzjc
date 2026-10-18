# v1.4.3 验证记录

适配 Codex Proxy RS `>=3.29.1,<3.30.0`，Linux x86_64，glibc 2.36+。

交付文件：`dist/personal.modeltrace-health-1.4.3-x86_64-unknown-linux-gnu.tar.gz`，30,676,707 字节。

SHA-256：`ca4cd7838304b281914a1496ed384609395e735629e9874367c735e74ecba542`。

## 改进内容

账户页顶部新增自动检测状态：是否开启与检测周期（绿色开启/灰色关闭），以及下一次检测时间及来源标注（定时为「（自动）」，手动任务为「（手动）」）。下一次时间取定时 `nextDue` 与活动任务 `nextAt` 中较早者：正在进行显示「现在」，当天只显示时刻、跨天附带日期，悬停可见完整时间与时区。

展示数据全部来自既有 `api/bootstrap` 与 `api/progress` 快照，未改动后端、调度、评分与持久化；1.4.2 的进度显示修复继续保留。

## 验证结果

| 验证 | 结果 |
| --- | --- |
| Rust 单元测试 | 11项通过，覆盖阈值、连续确认、旧记录兼容、时区、账户绑定、完整SSE要求、运行时解包及指纹库 |
| Python 单元测试 | 6项通过，评分算法与运行时未变更 |
| 发布二进制 RPC 集成 | 34项全部通过（176.254秒），含延后评分阶段回归；调试二进制同一套件34项通过（145.024秒） |
| 真实版本升级 | 1.3.0→1.4.3、1.4.0→1.4.3、1.4.1→1.4.3、1.4.2→1.4.3均通过。检查自定义参数、配置版本、定时时区与下次执行时间、报告及指纹库保留，升级不重放请求 |
| 浏览器交互 | 20项检查通过，新增「自动检测状态与下次检测来源」：定时开启时显示周期并标注「（自动）」；关闭后显示「已关闭」与「下次检测 —」；手动任务运行时显示「现在（手动）」；无页面JavaScript异常 |
| 静态与构建 | Rust fmt、Clippy `-D warnings`、TypeScript及前端生产构建通过；以单核 `-j1`、Zig glibc2.36 构建，最高导入GLIBC符号版本2.34 |
| 安装包 | 包摘要与全部资源摘要、离线Python/NumPy评分及实际插件冷启动解包/完整评分通过，内置18个候选模型 |

多数RPC测试先显式预热评分引擎，以验证各自的业务路径；冷启动由延后评分回归、准备延迟回归及完整安装包测试覆盖。模拟宿主会提供指定调用预算和延迟模型流，未连接真实RS管理服务或发送真实账户请求。

升级用例在比较前会等待旧版本任务结算：1.4起评分可能跨维护回调完成，而进程重启后首次载入会把未结算任务按设计标记为「中断，需重新检测」，不重放请求；该行为自1.4.1起一致，并非1.4.3引入。

插件ID、状态namespace和schema保持不变；1.4默认参数、移组/关闭调度、配置备份及1.3时区逻辑保留。本次改进不要求更新宿主。

## 复现

```bash
cargo test --manifest-path backend/Cargo.toml --locked
cargo clippy --manifest-path backend/Cargo.toml --locked --all-targets -- -D warnings
.venv/bin/python -m unittest discover -s engine/tests -v
.venv/bin/python scripts/verify_artifact.py dist/personal.modeltrace-health-1.4.3-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/protocol_smoke.py --binary /path/to/extracted/bin/modeltrace-health -v
.venv/bin/python scripts/upgrade_smoke.py dist/personal.modeltrace-health-1.4.2-x86_64-unknown-linux-gnu.tar.gz dist/personal.modeltrace-health-1.4.3-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/ui_fixture.py
PLAYWRIGHT_BROWSERS_PATH=.cache/browser-bin node scripts/ui_smoke.mjs
```

浏览器环境准备及完整构建命令见 [README](../README.md#开发与验证)。浏览器截图：[账户列表](screenshots/accounts.png)、[参数与指纹库设置](screenshots/settings.png)、[检测报告](screenshots/report.png)、[手机深色主题](screenshots/mobile.png)。

1.4.2进度显示修复见 [1.4.2记录](verification-v1.4.2.md)，1.4功能验证见 [1.4.0记录](verification-v1.4.0.md)，1.4.1超时修复见 [1.4.1记录](verification-v1.4.1.md)，原性能与评分机制见 [1.3.0记录](verification-v1.3.0.md)。运行时SHA-256仍为 `a8e41e162360f8cbe7d352adbdd702ad93ce50a381fd00a90e6d4ca6cc52d980`，与1.4.1、1.4.2一致。
