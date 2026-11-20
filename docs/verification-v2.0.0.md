# v2.0.0 验证记录

适配 CPR `>=3.53.1,<3.54.0`，Linux x86_64 / ARM64（aarch64）、glibc 2.36+。SDK 与官方打包工具仍固定至 CPR v3.53.1 提交 `188a6817d141a8d9a1a0bceead4f07477d35276e`。

## 内核来源

- 评分代码来自 lm-detector 提交 `70159b7536a2ceb55c478c8c9472da81d3627e59` 的 `sdk/rust`：`analysis`、`challenge`、`codec`、`error`、`features`、`ranker` 与上游逐字一致（仅 `crate::` 改为 `super::`），`data.rs` 保留上游解码部分。
- 内置数据与 `https://lm.ikale.io/data/` 发布内容一致：分片下载解压后的 `shared_detector.bin` 与仓库文件 SHA-256 均为 `a55d4bb275b09b630d9deeeac5a119077869a40dc65132ffff0ab87e58b7c9e9`。

## 本地验证（x86_64）

- `cargo fmt --check`、`cargo clippy --all-targets -D warnings`、`cargo test`（27 项）通过；新增内置库解码、样本截断与拒绝、1/3 个样本评分、仅排名判定及旧报告兼容测试。
- RPC 回归 82 项：模拟宿主改为提供 lm-detector `bundle.json` 与 zstd 分片，覆盖在线更新、回退、格式不兼容和模型冲突；全部通过。
- 前端构建、API 分页测试和浏览器 UI 回归（3 个挑战的报告、置信度展示）通过。
- 发布包（x86_64，6.7 MB，1.28.0 为 30.8 MB，去掉了 Python 运行时归档）通过 `verify_artifact.py`：资源摘要、ELF 架构、包内无 Python 运行时、进程内完整检测流程。
- 跨版本升级 `upgrade_smoke.py`：以实际 1.28.0 发布包生成状态后升级到 2.0.0，配置、计划、历史和提交幂等保留，ModelTrace 旧库分块删除并切换到内置 lm-detector 库，升级时不重发模型请求。
- 性能（release 构建）：参考库解码约 33 ms，3 个回答评分约 7 ms。
- 发布附件由 CI 在 x86_64 与 ARM64 原生 runner 上构建并运行最终包 RPC 回归，摘要以 Release 中的 `.sha256` 为准。

## 历史验证

[1.28.0](verification-v1.28.0.md)、[1.27.0](verification-v1.27.0.md)、[1.26.0](verification-v1.26.0.md)、[1.25.0](verification-v1.25.0.md)、[1.24.0](verification-v1.24.0.md)、[1.23.0](verification-v1.23.0.md)、[1.22.0](verification-v1.22.0.md)、[1.21.0](verification-arm64.md)、[1.20.0](verification-v1.20.0.md)。
