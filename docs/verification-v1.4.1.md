# v1.4.1 验证记录

适配 Codex Proxy RS `>=3.29.1,<3.30.0`，Linux x86_64，glibc 2.36+。

交付文件：`dist/personal.modeltrace-health-1.4.1-x86_64-unknown-linux-gnu.tar.gz`，30,677,349 字节。

SHA-256：`b0edc243c86fb6638ff07b3ee65cfcd513efbd825300749c0616f5c4e0af8bc5`。

## 回归原因与修复

用已发布1.2及1.4安装包中的实际二进制，在相同30秒维护预算、默认120秒插件超时下，让模拟模型在25秒返回完整有效回答：

| 版本 | 实际结果 |
| --- | --- |
| 1.2.0 | 第一次回调建立任务，第二次准备挑战，第三次请求约25.6秒后成功保存有效样本 |
| 1.4.0 | 首次回调合并步骤后，约22.5秒主动记录「模型请求超时」 |
| 1.4.1 | 冷启动及额外3秒准备延迟时，先保存挑战，不发送请求也不消耗尝试；下一回调的25秒请求成功，最终完成评分且仅发送一次模型请求 |

1.3合并步骤时，30秒维护调用先扣除1.5秒安全时间，请求另扣6秒；准备阶段还继续占用同一预算。即使准备很快，请求也只剩约22.5秒。宿主在回调结束后约30秒再次调度，因此连续失败记录约52.5秒一次，与本次故障现象吻合。

1.4.1将请求额外预留改为0.5秒，加上原有1.5秒安全时间，共保留约2秒。请求可使用约28秒；发起前检查是否还有完整窗口，冷启动准备较慢时主动让出回调。评分预算不足时继续保留有效样本，下次评分不重发请求。

新增错误诊断区分维护预算超时与用户配置超时，保存实际窗口、耗时及已接收流数据量。连续两次维护预算超时提前终止；普通失败及配置超时仍最多5次。等待重试显示原因和次数，失败请求增加时立即触发报告刷新。

## 验证结果

| 验证 | 结果 |
| --- | --- |
| Rust 单元测试 | 11项通过，覆盖阈值、连续确认、旧记录兼容、时区、账户绑定、完整SSE要求、运行时解包及指纹库 |
| Python 单元测试 | 6项通过，评分算法与运行时未变更 |
| 发布二进制 RPC 集成 | 33项全部通过（159.902秒），包括上述25秒回归、连续预算超时停止、配置超时5次上限，以及1.4的两项调度操作、身份保护、定时、指纹库、取消与实时进度测试 |
| 真实版本升级 | 1.4.0→1.4.1、1.3.0→1.4.1均通过。使用安装包中的二进制与同一模拟状态存储，检查自定义参数、配置版本、定时时区与下次执行时间、报告及指纹库保留，升级不重放请求；旧报告新增可选诊断字段为空 |
| 浏览器交互 | 活跃任务每秒轮询，流进度与重试原因在2.5秒内显示；失败次数增加时报告自动更新，显示耗时、接收量和请求窗口；原有配置备份、导入、两项独立操作、宿主iframe隔离、桌面/手机与深色模式通过，无页面JavaScript异常 |
| 静态与构建 | Rust fmt、Clippy `-D warnings`、TypeScript及前端生产构建通过；Zig以glibc2.36构建，最高导入GLIBC符号版本2.34 |
| 安装包 | 包摘要与全部资源摘要、相对资源路径、私有Python/NumPy离线校验及实际插件冷启动解包/完整评分通过，内置18个候选模型 |

多数RPC测试先显式预热评分引擎，以验证各自的业务路径；冷启动由独立的准备延迟回归及完整安装包测试覆盖。模拟宿主会提供指定调用预算和延迟模型流，未连接真实RS管理服务或发送真实账户请求。

插件ID、状态namespace和schema保持不变；1.4默认参数、移组/关闭调度、配置备份及1.3时区逻辑保留。本修复不要求更新宿主。宿主单次维护调用仍限制模型最长等待时间，插件配置120秒不代表可在30秒回调里等待120秒；真正的请求状态不确定仍保留中断保护。

## 复现

```bash
cargo test --manifest-path backend/Cargo.toml --locked
cargo clippy --manifest-path backend/Cargo.toml --locked --all-targets -- -D warnings
.venv/bin/python -m unittest discover -s engine/tests -v
.venv/bin/python scripts/verify_artifact.py dist/personal.modeltrace-health-1.4.1-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/protocol_smoke.py --binary /path/to/extracted/bin/modeltrace-health -v
.venv/bin/python scripts/upgrade_smoke.py dist/personal.modeltrace-health-1.4.0-x86_64-unknown-linux-gnu.tar.gz dist/personal.modeltrace-health-1.4.1-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/ui_fixture.py
PLAYWRIGHT_BROWSERS_PATH=.cache/browser-bin node scripts/ui_smoke.mjs
```

浏览器环境准备及完整构建命令见 [README](../README.md#开发与验证)。1.4功能验证见 [1.4.0记录](verification-v1.4.0.md)，原性能与评分机制见 [1.3.0记录](verification-v1.3.0.md)。运行时SHA-256仍为 `a8e41e162360f8cbe7d352adbdd702ad93ce50a381fd00a90e6d4ca6cc52d980`。
