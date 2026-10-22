# v1.6.0 验证记录

检测历史批量删除、`api/delete` 批量接口与定时订阅多选。

适配 Codex Proxy RS `>=3.29.7,<3.30.0`，Linux x86_64，glibc 2.36+。

交付文件：`dist/personal.modeltrace-health-1.6.0-x86_64-unknown-linux-gnu.tar.gz`，30,720,417字节。

SHA-256：`0d663a980c8e000fb1a3a4728a081a00a8f8aa4a4b9738d32ff85341a14ac5ea`。

## 验证范围

- 检测历史新增选择列和「全选筛选结果」，按当前搜索与结论筛选选择全部可删除记录；跨页选择保留，可一次删除所选记录。
- 正在排队、检测中或等待关闭调度、移组的记录不可选择；批量删除跳过此类记录并返回 `deleted`、`skipped`。单条删除与旧版 `{id}` 请求保持兼容。
- 批量删除只删除历史与报告，不恢复已停用账户，不改变账户分组或调度状态；配置、自动计划、指纹库和提交去重状态均不受影响。
- 自动定时检测的订阅类型改为多选，按账户订阅类型匹配任一选中项；留空表示全部。旧配置的单一订阅值可继续运行，界面载入时自动转换为多选。
- 插件 ID 不变；新增的定时订阅多选字段向后兼容旧配置与状态。从 1.5.1 更新保留现有配置、自动计划、指纹库和检测历史。

使用 Rust 1.99.1、Node 24.22.0 与 pnpm 10.32.1 构建，发布二进制通过 `CARGO_BUILD_JOBS=1` 单核编译。

## 验证结果

| 验证 | 结果 |
| --- | --- |
| Rust单元测试 | 14项通过 |
| Python评分单元测试 | 6项通过 |
| 静态与前端构建 | Rust fmt、Clippy `-D warnings`、TypeScript、Vite生产构建与pnpm冻结依赖安装通过 |
| 最终安装包RPC集成 | 50项全部通过；新增批量删除、活动记录跳过、旧版单条删除兼容与多订阅定时筛选回归 |
| 1.5.1 → 1.6.0实际升级 | 配置、配置版本、自动计划、检测历史、请求记录、指纹库与提交去重保留，无探针重放 |
| 安装包 | 包及全部资源SHA-256、协议v5清单、内置Python/NumPy离线评分、Rust实际解包和完整检测通过，内置18个模型 |
| 平台兼容 | 正式二进制目标为Linux x86_64、glibc2.36；最高GLIBC导入符号版本2.34 |
| 浏览器交互 | 28组交互通过，新增历史全选、批量删除与定时订阅多选，无页面JavaScript异常 |

## 复现

```bash
cargo test --manifest-path backend/Cargo.toml --locked
cargo clippy --manifest-path backend/Cargo.toml --locked --all-targets -- -D warnings
.venv/bin/python -m unittest discover -s engine/tests -v
pnpm --dir frontend install --frozen-lockfile
pnpm --dir frontend build
.venv/bin/python scripts/protocol_smoke.py --binary /path/to/package/bin/modeltrace-health -v
.venv/bin/python scripts/verify_artifact.py dist/personal.modeltrace-health-1.6.0-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/upgrade_smoke.py dist/personal.modeltrace-health-1.5.1-x86_64-unknown-linux-gnu.tar.gz dist/personal.modeltrace-health-1.6.0-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/ui_fixture.py --binary /path/to/package/bin/modeltrace-health
PLAYWRIGHT_BROWSERS_PATH=.cache/browser-bin node scripts/ui_smoke.mjs
```

界面截图：[账户](screenshots/accounts.png)、[配置](screenshots/settings.png)、[报告](screenshots/report.png)、[移动端](screenshots/mobile.png)、[历史批量选择](screenshots/history.png)。

历史验证：[1.5.1](verification-v1.5.1.md)、[1.5.0](verification-v1.5.0.md)、[1.4.6](verification-v1.4.6.md)、[1.4.5](verification-v1.4.5.md)、[1.4.3](verification-v1.4.3.md)、[1.4.2](verification-v1.4.2.md)、[1.4.1](verification-v1.4.1.md)、[1.4.0](verification-v1.4.0.md)、[1.3.0](verification-v1.3.0.md)。
