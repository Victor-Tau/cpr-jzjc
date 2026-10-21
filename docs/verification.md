# v1.5.1 验证记录

适配 Codex Proxy RS `>=3.29.7,<3.30.0`，Linux x86_64，glibc 2.36+。

交付文件：`dist/personal.modeltrace-health-1.5.1-x86_64-unknown-linux-gnu.tar.gz`，30,709,120字节。

SHA-256：`2f0d7a151596c751f7e66ca2e136d585a232d73ca304abe3b89f8a5089083bec`。

## 适配与验证范围

[宿主3.29.7发布说明](https://github.com/zyycn/codex-proxy-rs/releases/tag/v3.29.7)要求插件升级至RPC v5并使用新SDK重新编译。SDK固定到该版提交 `78bc57c5cf24b01a38e985cdd739bb4ae89ce11f`，打包CLI也从同一提交构建；包内 `package.protocolVersion=5`，最低宿主版本为3.29.7。指纹库下载请求改用字节形式的HTTP头，模拟宿主返回新版 `http_version` 与 `trailers`，覆盖检查、下载、应用和回退。

UI包更新至0.5.0，下载包已核对官方SHA-256：`fc81653484c4494e7ef319d0884d045259ddaacd0ffbf4b7831e18010c74e163`。使用Rust1.99.1、Node24.22.0与pnpm10.32.1构建，Rust及前端依赖均有锁文件，冻结依赖安装通过。

移除1.5.0新增的费用组件、报告汇总、费用API、前端轮询及维护回调中的同步。回归测试禁止新版插件访问宿主使用统计接口，并检查任务列表、进度与报告不再返回费用字段。旧历史中的费用字段可被忽略，原配置、评分、任务记录和请求ID继续保留。旧历史评分概率按项目现有升级比较规则允许JSON浮点往返的微小差异。

## 验证结果

| 验证 | 结果 |
| --- | --- |
| Rust单元测试 | 13项通过 |
| Python评分单元测试 | 6项通过 |
| 静态与前端构建 | Rust fmt、Clippy `-D warnings`、TypeScript、Vite生产构建与pnpm冻结依赖安装通过 |
| 费用移除、旧历史与库更新回归 | 3项通过；费用API不可用，不查询使用统计，旧费用字段不妨碍报告恢复 |
| 最终安装包RPC集成 | 48项分批全部通过；覆盖约29秒请求窗口、部分结果接纳、评分延后、手动/自动队列、取消、自动操作、重启与指纹库更新 |
| 1.5.0 → 1.5.1实际升级 | 配置、配置版本、自动计划、检测历史、请求记录、指纹库与提交去重保留，无探针重放 |
| 安装包 | 包及全部资源SHA-256、协议v5清单、内置Python/NumPy离线评分、Rust实际解包和完整检测通过，内置18个模型 |
| 平台兼容 | 正式二进制目标为Linux x86_64、glibc2.36；最高GLIBC导入符号版本2.34 |
| 浏览器交互 | 26组交互通过，无页面JavaScript异常；账户、历史与报告均无费用显示，未发送费用同步请求 |

测试使用实际插件进程与模拟宿主、模型流和状态存储，没有调用真实账户。最终发布验证从完整安装包目录启动二进制；因环境单条命令限300秒，已完成的34项和其余14项分批记录。Python/NumPy依赖未变，运行时复用已校验的1.5.0安装包，摘要为 `f57f34194ad4dd313bb0942d23d213970b9bba96c1cd4145a94b07db8d4ec37e`。

## 复现

```bash
cargo test --manifest-path backend/Cargo.toml --locked
cargo clippy --manifest-path backend/Cargo.toml --locked --all-targets -- -D warnings
.venv/bin/python -m unittest discover -s engine/tests -v
pnpm --dir frontend install --frozen-lockfile
pnpm --dir frontend build
.venv/bin/python scripts/protocol_smoke.py --binary /path/to/package/bin/modeltrace-health -v
.venv/bin/python scripts/verify_artifact.py dist/personal.modeltrace-health-1.5.1-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/upgrade_smoke.py dist/personal.modeltrace-health-1.5.0-x86_64-unknown-linux-gnu.tar.gz dist/personal.modeltrace-health-1.5.1-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/ui_fixture.py --binary /path/to/package/bin/modeltrace-health
PLAYWRIGHT_BROWSERS_PATH=.cache/browser-bin node scripts/ui_smoke.mjs
```

界面截图：[账户](screenshots/accounts.png)、[配置](screenshots/settings.png)、[报告](screenshots/report.png)、[移动端](screenshots/mobile.png)、[历史](screenshots/history.png)。

历史验证：[1.5.0](verification-v1.5.0.md)、[1.4.6](verification-v1.4.6.md)、[1.4.5](verification-v1.4.5.md)、[1.4.3](verification-v1.4.3.md)、[1.4.2](verification-v1.4.2.md)、[1.4.1](verification-v1.4.1.md)、[1.4.0](verification-v1.4.0.md)、[1.3.0](verification-v1.3.0.md)。
