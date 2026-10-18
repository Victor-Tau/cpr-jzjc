# v1.4.2 验证记录

适配 Codex Proxy RS `>=3.29.1,<3.30.0`，Linux x86_64，glibc 2.36+。

交付文件：`dist/personal.modeltrace-health-1.4.2-x86_64-unknown-linux-gnu.tar.gz`，30,676,003 字节。

SHA-256：`2e1e9ea81644b37b226886a3dd22ee6ea7858875cd7d84474249584a21882edb`。

## 修复内容

1.4.1 中，模型回答在维护预算最后约5.5秒内到达时，有效样本已保存、评分被推迟到下一次维护回调，但阶段标签停留在 `validating`，界面显示「正在校验样本」并累计约30秒墙钟时间，容易被误读为校验耗时。

1.4.2 在推迟评分时上报独立阶段 `waiting_score`，前端显示「已获得有效样本，等待评分」。评分时机、请求是否重发、采样尝试预算均与1.4.1完全一致，仅修正显示。

对照验证使用同一回归测试分别运行两个发布二进制：`Integration.test_deferred_scoring_reports_waiting_score`（23秒返回的模拟回答，冷启动直接触发延后评分）在1.4.1二进制上失败于 `'validating' != 'waiting_score'`，在1.4.2二进制上通过；两次均只发送一次模型请求，下一次回调完成评分。

## 验证结果

| 验证 | 结果 |
| --- | --- |
| Rust 单元测试 | 11项通过，覆盖阈值、连续确认、旧记录兼容、时区、账户绑定、完整SSE要求、运行时解包及指纹库 |
| Python 单元测试 | 6项通过，评分算法与运行时未变更 |
| 发布二进制 RPC 集成 | 34项全部通过（174.830秒），含新增的延后评分阶段回归及上述1.4.1对照；调试二进制同一套件34项通过（145.709秒） |
| 真实版本升级 | 1.3.0→1.4.2、1.4.0→1.4.2、1.4.1→1.4.2均通过。检查自定义参数、配置版本、定时时区与下次执行时间、报告及指纹库保留，升级不重放请求 |
| 浏览器交互 | 19项检查通过：搜索、订阅筛选、批量提交、配置保存、备份与导入、移组重试、模型目录、指纹库更新回退、报告、移动端深色主题、2.5秒内实时进度、失败自动刷新、暂停恢复；无页面JavaScript异常 |
| 静态与构建 | Rust fmt、Clippy `-D warnings`、TypeScript及前端生产构建通过；Zig以glibc2.36构建，最高导入GLIBC符号版本2.34 |
| 安装包 | 包摘要与全部资源摘要、离线Python/NumPy评分及实际插件冷启动解包/完整评分通过，内置18个候选模型 |

多数RPC测试先显式预热评分引擎，以验证各自的业务路径；冷启动由新增的延后评分回归、准备延迟回归及完整安装包测试覆盖。模拟宿主会提供指定调用预算和延迟模型流，未连接真实RS管理服务或发送真实账户请求。

升级用例在比较前会等待旧版本任务结算：1.4起评分可能跨维护回调完成，而进程重启后首次载入会把未结算任务按设计标记为「中断，需重新检测」，不重放请求；该行为与1.4.1一致（1.4.1→1.4.1对照同样为中断），并非1.4.2引入。

插件ID、状态namespace和schema保持不变；1.4默认参数、移组/关闭调度、配置备份及1.3时区逻辑保留。本修复不要求更新宿主。宿主单次维护调用仍限制模型最长等待时间，插件配置120秒不代表可在30秒回调里等待120秒；真正的请求状态不确定仍保留中断保护。

## 复现

```bash
cargo test --manifest-path backend/Cargo.toml --locked
cargo clippy --manifest-path backend/Cargo.toml --locked --all-targets -- -D warnings
.venv/bin/python -m unittest discover -s engine/tests -v
.venv/bin/python scripts/verify_artifact.py dist/personal.modeltrace-health-1.4.2-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/protocol_smoke.py --binary /path/to/extracted/bin/modeltrace-health -v
.venv/bin/python scripts/upgrade_smoke.py dist/personal.modeltrace-health-1.4.1-x86_64-unknown-linux-gnu.tar.gz dist/personal.modeltrace-health-1.4.2-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/ui_fixture.py
PLAYWRIGHT_BROWSERS_PATH=.cache/browser-bin node scripts/ui_smoke.mjs
```

浏览器环境准备及完整构建命令见 [README](../README.md#开发与验证)。浏览器截图：[账户列表](screenshots/accounts.png)、[参数与指纹库设置](screenshots/settings.png)、[检测报告](screenshots/report.png)、[手机深色主题](screenshots/mobile.png)。

1.4功能验证见 [1.4.0记录](verification-v1.4.0.md)，1.4.1超时修复见 [1.4.1记录](verification-v1.4.1.md)，原性能与评分机制见 [1.3.0记录](verification-v1.3.0.md)。运行时SHA-256仍为 `a8e41e162360f8cbe7d352adbdd702ad93ce50a381fd00a90e6d4ca6cc52d980`，与1.4.1一致。
