# v1.8.2 验证记录

账户检测列表默认每页条数从 20 改为 100，仍保留 10、20、50、100 条选项。版本号同步更新至插件清单、前后端包信息和 Cargo 锁文件；依赖及运行时不变。

适配 Codex Proxy RS `>=3.32.0,<3.33.0`，Linux x86_64、glibc 2.36+，使用 CPR 3.32.0 SDK、RPC v6。

## 单核构建

前端和 Rust 发布构建均通过 `taskset -c 0` 绑定 CPU 0。Rust 设置 `CARGO_BUILD_JOBS=1`、`RAYON_NUM_THREADS=1`；前端设置 `UV_THREADPOOL_SIZE=1`、`RAYON_NUM_THREADS=1`。使用已有依赖缓存及私有 Python/NumPy 运行时。

```bash
taskset -c 0 pnpm --dir frontend build
taskset -c 0 env CARGO_BUILD_JOBS=1 RAYON_NUM_THREADS=1 \
  cargo zigbuild --manifest-path backend/Cargo.toml --locked --offline \
  --release --target x86_64-unknown-linux-gnu.2.36
```

按用户要求，不执行任何 SHA-256 校验。未运行安装包摘要校验脚本、运行时重建或刷新脚本、摘要相关测试。官方打包工具仅生成安装格式需要的资源摘要与摘要附件，不执行摘要比对。

## 验证结果

- 前端类型检查和生产构建通过，Vite 处理 2,158 个模块。
- Rust 格式检查通过；单核发布编译通过，耗时 3 分 18 秒，仅显示既有链接器优化参数弃用提示。
- 安装包结构检查通过：版本 1.8.2、RPC v6、宿主兼容范围正确，共 8 个文件；包内网页资源与本次前端构建内容逐字节一致，未计算或比对摘要。
- 交付安装包 `dist/personal.modeltrace-health-1.8.2-x86_64-unknown-linux-gnu.tar.gz`，30,764,131 字节。
- 浏览器确认初始显示「100 条/页」，43 个模拟账户全部显示在第一页，随后切换到 20 条/页验证既有分页及筛选流程。
- 既有浏览器交互回归通过，涵盖结果筛选、组合筛选、跨筛选选择、批量检测与删除、配置、报告、进度、移动端、深浅主题及宿主隔离页面，页面错误为 0。浏览器指纹库更新流程使用模拟响应，不执行真实摘要校验。

本次修改为前端默认值调整，未运行无关的完整 Rust、Python 和长请求 RPC 测试。

## 历史验证

[1.8.1](verification-v1.8.1.md)、[1.8.0](verification-v1.8.0.md)。
