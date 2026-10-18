# v1.4.0 验证记录

适配 Codex Proxy RS `>=3.29.1,<3.30.0`，Linux x86_64，glibc 2.36+。

交付文件：`dist/personal.modeltrace-health-1.4.0-x86_64-unknown-linux-gnu.tar.gz`，30,672,255 字节。

SHA-256：`f3e4db7a9139ce6b2af661f549bf36b7301407e602c238b0b20fbb52ddc7f7f8`。

| 验证 | 结果 |
| --- | --- |
| Rust 单元测试 | 11 项通过，包括新默认阈值边界、旧任务兼容、连续确认、时区、账户绑定、SSE、运行时解包及指纹库 |
| Python 单元测试 | 6 项通过，评分算法与运行时沿用1.3 |
| 发布二进制 RPC 集成 | 30 项通过，覆盖关闭调度与移组的四种开关组合、单项失败不阻止另一项、分别重试、空分组及分页、目标删除、身份变化、移组校验、任务配置快照、中断后不重复写入和配置导入预览不保存 |
| 真实版本升级 | 使用1.3和1.4安装包中的实际二进制，沿用同一模拟状态存储；自定义参数、配置版本、定时范围与时区、下次执行时间、指纹库及历史报告保留，新移组选项默认关闭，不重放模型请求 |
| 浏览器交互 | 新默认参数、独立操作开关、目标分组、JSON 下载/复制备份、导入预览、单独移组重试、桌面/手机及深色主题通过；另外使用 `sandbox="allow-scripts"` iframe 验证实际宿主隔离环境下 JSON 备份与导入，无页面 JavaScript 异常 |
| 实时刷新回归 | 活跃检测和待办调度操作每秒轮询；进度在2.5秒内更新；打开报告自动刷新，移组完成后刷新账户分组，隐藏暂停及恢复立即刷新 |
| 静态与构建 | Rust fmt、Clippy `-D warnings`、前端 TypeScript 检查和生产构建通过；Zig以glibc2.36为目标构建，最高导入GLIBC符号版本2.34 |
| 安装包校验 | 包摘要和全部资源摘要、相对资源路径、内置Python/NumPy离线校验及实际发布二进制冷启动解包/单次维护回调评分通过，内置18个候选模型 |

## 配置与副作用

保持插件 ID、状态 namespace、schemaVersion和schema不变。宿主更新原实例时复用状态代际，1.3缺少的新字段由反序列化默认值补齐，已保存规则与历史任务的配置快照不被新默认值覆盖。新建实例与原实例隔离，重装或迁移使用JSON备份导入。

导出已保存参数，导入文件先检查格式和大小，再由后端校验并补齐默认值，载入表单后需另行保存。宿主iframe禁止直接下载，提供可选中复制的完整JSON和复制按钮；可保存为 `modeltrace-health-config.json` 再导入。文件不含密钥值、账号凭据、历史或指纹库；跨平台迁移需重新核对Key、分组与账户ID。

确认降智后各操作独立持久化并执行。关闭调度仅发送 `enabled=false`，移组仅发送 `groupIds=[target]`，替换原有全部分组。每次核对冻结的账户身份，操作后校验目标状态；已到目标时跳过写入。失败不重新发探针；操作期间中断转为可手动重试的失败状态，避免不确定请求自动重放。待办操作不被历史清理或删除。

每个维护回调执行一项待办操作，实际应用仍受宿主维护间隔影响。模型请求较慢时可能只完成样本获取，把评分留到下一次回调；测试确认已保存样本且不会重发该探针。默认超时120秒与宿主预算限制继续沿用1.3。

## 复现

```bash
cargo test --manifest-path backend/Cargo.toml --locked
cargo clippy --manifest-path backend/Cargo.toml --locked -- -D warnings
.venv/bin/python -m unittest discover -s engine/tests -v
.venv/bin/python scripts/verify_artifact.py dist/personal.modeltrace-health-1.4.0-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/protocol_smoke.py --binary /path/to/extracted/bin/modeltrace-health -v
.venv/bin/python scripts/upgrade_smoke.py dist/personal.modeltrace-health-1.3.0-x86_64-unknown-linux-gnu.tar.gz dist/personal.modeltrace-health-1.4.0-x86_64-unknown-linux-gnu.tar.gz
```

浏览器复现依赖与命令见 [README](../README.md#开发与验证)，脚本为 `scripts/ui_fixture.py`、`scripts/ui_smoke.mjs`。截图：[账户列表](screenshots/accounts.png)、[设置](screenshots/settings.png)、[报告](screenshots/report.png)、[手机深色主题](screenshots/mobile.png)。

集成使用真实发布二进制与模拟宿主、账户、Key、模型流和GitHub响应；未连接真实RS管理服务或发送真实账户检测。宿主管理接口和实例升级状态复用机制已核对固定SDK对应的RS3.29.1及3.29.3源码。

性能实现、时区和运行时沿用1.3，性能对比见 [1.3验证记录](verification-v1.3.0.md)。本版未宣称额外的端到端检测提速。运行时SHA-256仍为 `a8e41e162360f8cbe7d352adbdd702ad93ce50a381fd00a90e6d4ca6cc52d980`。
