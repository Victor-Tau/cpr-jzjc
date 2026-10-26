# v1.8.1 验证记录

本版新增账户与检测历史的检测结果筛选：降智、未降智、其他。账户按最新记录、历史按每条记录分类；仅已完成且结论为 `degraded` / `no_match` 的记录进入前两类，其余状态进入「其他」。账户页暂不支持的提供方也进入「其他」。

适配 Codex Proxy RS `>=3.32.0,<3.33.0`，Linux x86_64、glibc 2.36+。沿用 1.8.0 的 CPR 3.32.0 SDK 与打包工具、RPC v6、180 秒维护预算及私有状态 schema。

## 单核构建

前端生产构建和 Rust 发布构建均通过 `taskset -c 0` 绑定 CPU 0；设置 `CARGO_BUILD_JOBS=1`、`RAYON_NUM_THREADS=1`、`UV_THREADPOOL_SIZE=1`、`OPENBLAS_NUM_THREADS=1`、`OMP_NUM_THREADS=1`，Rust release 的 `codegen-units=1`。使用 Rust 1.99.1、Node 24.22.0，依赖锁文件固定，无依赖变更。

```bash
taskset -c 0 pnpm --dir frontend build
taskset -c 0 env CARGO_BUILD_JOBS=1 RAYON_NUM_THREADS=1 \
  cargo zigbuild --manifest-path backend/Cargo.toml --locked --offline \
  --release --target x86_64-unknown-linux-gnu.2.36
```

复用工作区中已由 1.8.0 验证的 Python/NumPy 运行时，评分代码与依赖没有变更。Zig 链接仍显示原有优化参数弃用提示，构建成功。

交付文件：`dist/personal.modeltrace-health-1.8.1-x86_64-unknown-linux-gnu.tar.gz`，30,762,346 字节。

SHA-256：`c94dc7a620d1e71549f367f21f36ef817fc17e0db06d0961cfdd3fa3c526f6b5`。

## 验证结果

| 验证 | 结果 |
| --- | --- |
| 前端类型检查与生产构建 | 通过，Vite 处理 2,158 个模块 |
| Rust 格式与单元测试 | `cargo fmt --check`、17 项单元测试通过 |
| Python 评分单元测试 | 6 项通过 |
| 安装包校验 | 包和全部资源摘要、RPC v6、宿主兼容范围、18 个模型离线校验、实际插件解包和评分通过 |
| 发布包 RPC 回归 | 6 项通过，覆盖注册查询、配置导入预览、历史批量删除保护、3 天保留、绑定与提交幂等、批内并行 |
| 升级回归 | 1.8.0 → 1.8.1 通过，配置、计划、历史、指纹库保留，无探针重发 |
| 新增浏览器回归 | 三类筛选、最新记录优先、未检测和不支持账户、未完成或失败记录的旧结论、组合搜索/订阅/状态/分组、切换后返回第一页、跨筛选保留选择、队列与降智快捷筛选、清除筛选、实时分类变化、历史搜索与全选通过 |
| 既有浏览器回归 | 检测、批量提交与删除、配置、报告、进度、取消、移动布局、深浅主题、宿主隔离页面通过；页面错误为 0 |

本次使用实际发布包二进制与模拟宿主、生产前端构建进行验证，不发送真实账户请求，未在生产宿主部署。未重跑与本次前端筛选无关的全部长请求 RPC 用例；上版完整回归及预算验证见 [1.8.0](verification-v1.8.0.md)。

可用以下入口复现（环境需具备 README 中的依赖）：

```bash
.venv/bin/python scripts/verify_artifact.py dist/personal.modeltrace-health-1.8.1-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/upgrade_smoke.py dist/personal.modeltrace-health-1.8.0-x86_64-unknown-linux-gnu.tar.gz dist/personal.modeltrace-health-1.8.1-x86_64-unknown-linux-gnu.tar.gz
.venv/bin/python scripts/ui_fixture.py --binary .cache/release-v1.8.1/bin/modeltrace-health
PLAYWRIGHT_BROWSERS_PATH=.cache/browser-bin node scripts/ui_smoke.mjs
```

浏览器筛选截图保存在 `.cache/result-filter-accounts.png` 和 `.cache/result-filter-history.png`，详细执行日志为 `.cache/*v1.8.1*.log`。

## 历史验证

[1.8.0](verification-v1.8.0.md)、[1.7.3](verification-v1.7.3.md)、[1.7.2](verification-v1.7.2.md)、[1.7.1](verification-v1.7.1.md)。
