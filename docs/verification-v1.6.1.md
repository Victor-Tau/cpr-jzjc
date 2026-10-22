# v1.6.1 验证记录

修复 RPC v5 WebSocket 载荷被拒绝，导致检测在首个元数据事件后提前关闭的问题。

适配 Codex Proxy RS `>=3.29.7,<3.30.0`，Linux x86_64，glibc 2.36+。

交付文件：`dist/personal.modeltrace-health-1.6.1-x86_64-unknown-linux-gnu.tar.gz`，30,720,975字节。

## 问题复现与修复

原请求记录显示 WebSocket 握手成功、上游返回 `codex.response.metadata` 后，插件立即关闭流；宿主将请求记为 `Cancelled / receiver_dropped`，插件报告正文与流数据均为0。

新增 RPC 回归在旧版二进制上复现失败。修复后同时支持宿主解析视图与原始 WebSocket 消息，复用现有 Responses 事件处理；按原始消息字节数计量，元数据不终止请求，完成快照不重复累计正文。明确失败、无效消息及超限消息继续拒绝，输出上限的有效部分结果继续评分。

## 构建约束

全部编译命令通过 `taskset -c 0` 绑定到一个 CPU 核心，同时设置 `CARGO_BUILD_JOBS=1`；发布构建另设 `RAYON_NUM_THREADS=1`。使用现有依赖缓存及私有 Python/NumPy 运行时，不重新下载运行时。

按本次发布要求，不执行 SHA-256 校验。跳过 `verify_artifact.py`、运行时重建中的摘要校验、指纹库摘要单元测试以及涉及摘要核对或在线库应用的集成用例。打包 CLI 仅生成宿主格式要求的资源摘要与摘要附件，不核对它们。

## 验证结果

| 验证 | 结果 |
| --- | --- |
| Rust单元测试 | 16项通过，跳过1项指纹库摘要测试 |
| Python评分单元测试 | 6项通过 |
| 静态与前端构建 | Rust fmt、Clippy `-D warnings`、TypeScript与Vite生产构建通过 |
| 最终安装包RPC集成 | 47项通过：2项新增WebSocket回归（含4个子场景）和45项原有流程；另外5项涉及摘要核对或在线库应用的用例未执行 |
| 1.6.0 → 1.6.1实际升级 | 配置、配置版本、多订阅定时计划、检测历史、请求诊断、指纹库元数据和提交去重保留，无探针重放；未核对摘要 |
| 安装包功能 | 协议v5、版本和资源路径确认通过；包内私有Python/NumPy由正式二进制实际解包，完成检测与评分 |
| 平台兼容 | Linux x86_64、glibc2.36目标构建；最高GLIBC导入符号版本2.34 |

单核发布构建耗时3分08秒；链接器仅提示旧优化参数被忽略，构建成功。没有在生产宿主安装插件或发送真实账号检测请求。

## 定向回归复现

```bash
taskset -c 0 env CARGO_BUILD_JOBS=1 cargo test --manifest-path backend/Cargo.toml --locked --offline -- --skip banks::tests
taskset -c 0 env CARGO_BUILD_JOBS=1 cargo clippy --manifest-path backend/Cargo.toml --locked --offline --all-targets -- -D warnings
taskset -c 0 .venv/bin/python scripts/protocol_smoke.py --binary /path/to/package/bin/modeltrace-health -v Integration.test_websocket_metadata_then_completion_with_or_without_parsed_data Integration.test_websocket_failure_is_rejected_and_output_limit_can_be_scored
```

其余 RPC 回归按名称排除 `test_binding_idempotency_and_completed_scoring`、`test_update_atomicity_rollback_and_running_snapshot`、`test_rejects_incompatible_bank_and_rule_conflict`、`test_partial_update_keeps_active_and_cleans_on_restart`、`test_update_survives_restart_and_is_used_by_new_tasks`，以避开摘要校验。升级验证比较配置、计划、报告、请求诊断和指纹库元数据，不核对摘要。

## 历史验证

[1.6.0](verification-v1.6.0.md)、[1.5.1](verification-v1.5.1.md)、[1.5.0](verification-v1.5.0.md)、[1.4.6](verification-v1.4.6.md)。
