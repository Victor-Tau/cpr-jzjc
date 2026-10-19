# v1.4.6 验证记录

适配 Codex Proxy RS `>=3.29.1,<3.30.0`，Linux x86_64，glibc 2.36+。

交付文件：`dist/personal.modeltrace-health-1.4.6-x86_64-unknown-linux-gnu.tar.gz`，30,685,799 字节。

SHA-256：`9e8de7b40b212bde89ee342c39caffec9caa206227090041c2413dc10ef70367`。

## 改进内容

请求达到宿主预算或配置超时、输出 token 上限、流正常关闭却缺少完成事件时，保留已收到的正文，丢弃末尾可能未输出完整的数字，再交给原有样本校验：有效数字至少 `max(80, ceil(期望数量 × 55%))` 个、至多4096个。通过后保存样本并继续评分；评分延后不重发模型请求。只有推理内容、数字不足、明确上游失败、内容过滤和协议错误仍不接纳，管理员取消优先。

实时进度和请求记录分别显示正文、可见推理/推理摘要的 UTF-8 字节数，保留流数据总量；完成事件的快照不与增量重复计数。仅统计宿主实际返回的可见文本，无法从隐藏推理 token 推算字节；历史记录缺少分类统计时仅显示已有总量。

收到完成事件即结束接收，不再等待额外的流结束通知。预算充足时，在同一次维护回调内连续完成评分、关闭调度、移组及后续账户检测，每个任务每次回调最多推进一轮或一次失败尝试。后续账户按剩余预算执行，缩短窗口超时保留普通重试预算；完整窗口连续两次宿主预算超时仍提前终止。自动更新检查仅在空闲且有足够预算时执行。

账户操作也受当前维护预算限制，超时保存失败原因；重试先校验当前账户状态和分组，避免重复修改。插件 ID、状态 namespace 和 schema 不变，原实例升级保留配置、历史、自动计划及指纹库。

## 验证结果

| 验证 | 结果 |
| --- | --- |
| Rust 单元测试 | 13 项通过，含逐字节 UTF-8/CRLF SSE 分片、末尾数字截断、完成快照去重、推理隔离及上游失败拒绝 |
| Python 单元测试 | 6 项通过，原样本门槛和评分算法保持原值 |
| 调试二进制 RPC 集成 | 45 项全部通过，191.907 秒 |
| 发布二进制关键场景 | 7 项通过；其中组合终止用例改为等待实际采样后单独复测通过 |
| 真实版本升级 | 1.4.5→1.4.6 通过；配置、自动计划、报告和指纹库保留，无探针重放 |
| 安装包 | 包及全部资源摘要、离线 Python/NumPy 评分、实际插件冷启动解包与完整检测流程通过，内置18个候选模型 |
| 浏览器交互 | 25 组检查通过，无页面 JavaScript 异常；增加正文/推理分开显示及部分结果接纳标记断言 |
| 静态与构建 | Rust fmt、Clippy `-D warnings`、TypeScript 和 Vite 生产构建通过；Zig glibc 2.36 发布构建完成，最高导入 GLIBC 符号版本为 2.34 |

新增 RPC 场景验证：配置超时和宿主预算超时后接纳有效正文；有效样本持久化并在下次回调评分而不重发；仅推理时拒绝接纳；正常 EOF、输出上限、数字不足、上游失败、内容过滤及事件数量错误；完成事件到达但流未关闭时及时结束；同回调移组后继续第二个账户；慢移组预算内保存失败，重试校验现状而不重复修改。已有的冷启动、25秒回答、取消、来源隔离、存储失败和重启保护均通过。发布包并行校验时，准备步骤可能按设计先让出请求窗口；组合终止用例原先假定单次调度必定发起请求，现改为等待实际采样后断言，复测通过。

测试使用模拟宿主与模型流，没有向真实账户发送检测请求。浏览器运行实际前端构建，使用模拟宿主数据；Chromium 在沙箱外运行以避开环境对浏览器系统调用的限制。前端沿用锁定依赖，使用当前环境的 Node 22.22.1 构建。

Python/NumPy 运行时复用已验证的1.4.5发布包并刷新项目源码，校验全部源包资源摘要；运行时摘要未变：`a8e41e162360f8cbe7d352adbdd702ad93ce50a381fd00a90e6d4ca6cc52d980`。

## 复现

```bash
cargo test --manifest-path backend/Cargo.toml --locked
cargo clippy --manifest-path backend/Cargo.toml --locked --all-targets -- -D warnings
.venv/bin/python -m unittest discover -s engine/tests -v
.venv/bin/python scripts/protocol_smoke.py -v
.venv/bin/python scripts/verify_artifact.py dist/personal.modeltrace-health-1.4.6-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/upgrade_smoke.py dist/personal.modeltrace-health-1.4.5-x86_64-unknown-linux-gnu.tar.gz dist/personal.modeltrace-health-1.4.6-x86_64-unknown-linux-gnu.tar.gz
PLAYWRIGHT_BROWSERS_PATH=.cache/browser-bin node scripts/ui_smoke.mjs
```

历史验证：[1.4.5](verification-v1.4.5.md)、[1.4.3](verification-v1.4.3.md)、[1.4.2](verification-v1.4.2.md)、[1.4.1](verification-v1.4.1.md)、[1.4.0](verification-v1.4.0.md)、[1.3.0](verification-v1.3.0.md)。
