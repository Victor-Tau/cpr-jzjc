# v2.1.0 验证记录

适配 CPR `>=3.53.1,<3.54.0`，Linux x86_64 / ARM64（aarch64）、glibc 2.36+。SDK 与官方打包工具固定至 CPR v3.53.1 提交 `188a6817d141a8d9a1a0bceead4f07477d35276e`；检测内核与内置数据与 2.0.0 相同。

## 本地验证（x86_64）

- `cargo fmt --check`、`cargo clippy --all-targets -D warnings`、`cargo test`（28 项）通过；新增「与目标模型不匹配」判定（有/无置信度）、目标模型须在库内的保存校验，以及旧配置默认值测试。
- RPC 回归 84 项通过，新增：
  - 移回正常：替换为正常分组、不改变调度状态、已在分组时不重复写入、检测中拒绝、分组不存在时保存被拒。
  - 与目标模型不匹配：目标不在库内时保存被拒；别名映射后按最高候选判定。
- 浏览器 UI 回归通过；截图确认「移回正常」位于「检测」右侧，判定规则切换与正常分组设置显示正常。
- `scripts/package.sh` 与 `verify_artifact.py` 通过（发布包 6.7 MB）。
- 跨版本升级 `upgrade_smoke.py`：2.0.0 实际发布包 → 2.1.0，配置、计划、历史和指纹库保留，不重发模型请求。
- 发布附件由 CI 在 x86_64 与 ARM64 原生 runner 上构建并运行最终包 RPC 回归，摘要以 Release 中的 `.sha256` 为准。

## 历史验证

[2.0.0](verification-v2.0.0.md)、[1.28.0](verification-v1.28.0.md)、[1.27.0](verification-v1.27.0.md)、[1.26.0](verification-v1.26.0.md)、[1.25.0](verification-v1.25.0.md)、[1.24.0](verification-v1.24.0.md)、[1.23.0](verification-v1.23.0.md)、[1.22.0](verification-v1.22.0.md)、[1.21.0](verification-arm64.md)、[1.20.0](verification-v1.20.0.md)。
