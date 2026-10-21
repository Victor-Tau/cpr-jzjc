# v1.5.0 验证记录

适配 Codex Proxy RS `>=3.29.1,<3.30.0`，Linux x86_64，glibc 2.36+。

交付文件：`dist/personal.modeltrace-health-1.5.0-x86_64-unknown-linux-gnu.tar.gz`，30,713,791字节。

SHA-256：`db310584affaee05a31ae2c05552ed74509caf5870889c17970cde3d543086ed`。

## 改进与验证范围

30秒维护回调中的请求窗口上限调整为约29秒。指纹库在请求前预加载，已校验样本可在剩余约300毫秒以上时尝试评分；评分按实际维护截止时间终止，样本保留并在下次优先处理，不重复请求。关闭模型流最多等待100毫秒。RPC回归使用28.7秒返回的完整回答，断言同次回调完成评分、只发起一次探针，实际请求窗口大于28.5秒且不超过29秒。另有保存样本耗尽剩余预算时延后评分的回归。

费用接口已核对固定SDK对应RS3.29.1与本地RS3.29.3源码：按模型执行返回的请求ID读取 `/api/admin/usage/records/detail?id=…`，使用 `costs[].currency`、`costs[].estimatedAmount` 与 `costCoverage`，不自行估算价格。按任务累计全部请求，包括失败重试和复测；按请求ID去重，按币种分别使用十进制字符串精确相加。未知费用保持待同步，已知零费用与不计费记录可正确显示；运行中的平台记录继续等待结算。费用持久化在各次尝试内，保留30秒查询冷却，升级时补齐默认值。查询有独立预算，进度和报告通过缓存即时读取。

账户列表、历史和报告的费用均与结果同行，前端移除推理KB显示，将未命中结果改为「未降智」。

## 验证结果

| 验证 | 结果 |
| --- | --- |
| Rust 单元测试 | 14项通过，含十进制金额精度、未知费用、请求去重与旧字段兼容 |
| Python 单元测试 | 6项通过，样本门槛和原评分算法保持原值 |
| 调试二进制 RPC 集成 | 50项全部通过，219.597秒；最终补充3项费用回归通过，1.923秒；含28.7秒返回当次评分、预算耗尽保留样本、失败请求费用累计、跨重启补全、零费用、多币种、慢费用查询不影响结果 |
| 最终发布二进制 | 7项关键RPC场景通过，58.949秒；含28.7秒返回当次评分、宿主预算超时部分结果、费用结算与预算耗尽延后评分 |
| 1.4.6 → 1.5.0 升级 | 配置、版本、自动计划、历史及指纹库保留，提交去重有效，无探针重放 |
| 安装包 | 包和全部资源SHA-256、内置Python/NumPy离线评分、实际插件解包与完整检测通过，内置18个模型；最高GLIBC导入符号版本2.34 |
| 浏览器交互 | 原有25组交互及新增文案、推理字段移除、费用同行右侧布局断言通过，无页面JavaScript异常 |
| 静态与前端构建 | Rust fmt、Clippy `-D warnings`、TypeScript及Vite生产构建通过 |

测试使用实际插件二进制与模拟宿主、模型流和使用统计，没有向真实账户发送检测请求。浏览器运行实际前端构建与确定性宿主样本；当前环境使用Node22.22.1构建。Python/NumPy依赖未变，运行时复用已校验的1.4.6安装包并刷新评分程序，摘要为 `f57f34194ad4dd313bb0942d23d213970b9bba96c1cd4145a94b07db8d4ec37e`。

## 复现

```bash
cargo test --manifest-path backend/Cargo.toml --locked
cargo clippy --manifest-path backend/Cargo.toml --locked --all-targets -- -D warnings
.venv/bin/python -m unittest discover -s engine/tests -v
.venv/bin/python scripts/protocol_smoke.py -v
.venv/bin/python scripts/verify_artifact.py dist/personal.modeltrace-health-1.5.0-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/upgrade_smoke.py dist/personal.modeltrace-health-1.4.6-x86_64-unknown-linux-gnu.tar.gz dist/personal.modeltrace-health-1.5.0-x86_64-unknown-linux-gnu.tar.gz
PLAYWRIGHT_BROWSERS_PATH=.cache/browser-bin node scripts/ui_smoke.mjs
```

历史验证：[1.4.6](verification-v1.4.6.md)、[1.4.5](verification-v1.4.5.md)、[1.4.3](verification-v1.4.3.md)、[1.4.2](verification-v1.4.2.md)、[1.4.1](verification-v1.4.1.md)、[1.4.0](verification-v1.4.0.md)、[1.3.0](verification-v1.3.0.md)。
