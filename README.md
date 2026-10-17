# 缓境 Huanj

使用 Rust 扫描开发依赖与构建目录，使用 Tauri 2 + React 展示和清理。首次启动默认扫描所有已挂载、可访问的磁盘；保存设置后，下次启动优先恢复已保存的扫描范围，点击“开始扫描”后运行。

## 功能

- Windows 的本地盘、可移动盘、已映射网络盘；macOS 数据卷和其他已挂载卷。
- 识别 `node_modules`、`target`、`.venv`，结合项目标识避免仅凭名称判断。
- 受控工作线程、目录去重、不跟随符号链接或 Windows reparse point。
- 先展示候选，再补充大小；实时进度、取消、筛选、搜索与虚拟列表。
- 完整统计后手动勾选，确认后移入回收站；逐项反馈错误。
- 中文界面、系统/浅色/深色主题、自定义扫描范围和线程数。
- 固定页面布局，只有结果列表和弹窗长内容滚动；默认 8 工作线程，可在设置中调整。

0.1.1 的实际 D 盘对照、独立大小抽查与布局回归见 [验证记录](docs/2026-10-17-performance-layout.md)。

0.1.2 默认以居中的 1080×680 普通窗口启动，不全屏、不最大化。顶部采用紧凑布局，结果行高为 64px，扫描结果占据剩余空间；窗口仍可自由调整大小。

0.1.3 支持保存扫描设置：在“调整范围 / 扫描设置”里选择自定义目录与线程数后点击“保存设置”，当前扫描类型一并保存。下次启动恢复这些设置，不自动开始扫描。切换全盘并保存也会保留自定义目录列表，之后可再切回。

桌面设置存放在系统应用配置目录中的 `scan-settings.json`，与 WebView 缓存分开，并沿用应用标识跨版本保存。离线或暂未挂载的位置不会被自动删除；扫描时仍检查路径可访问性。保存失败、内容损坏会明确提示，不在启动时自动覆盖原文件。

## 开发

需要 Rust stable、Node.js 22+。Windows 还需要 MSVC C++ Build Tools 和 WebView2；macOS 需要 Xcode Command Line Tools。

界面运行要求较新的系统 WebView：Windows 使用 WebView2；macOS 11+ 需要系统 WebKit 更新到支持 Safari 15.4 特性的版本（例如 macOS 12.3 或更新系统），以支持原生 dialog 与动态视口。相关能力见 [WebKit 15.4 发布说明](https://webkit.org/blog/12445/new-webkit-features-in-safari-15-4/)。目前尚未在 Mac 实机验证。

```sh
npm ci
npm run tauri dev
```

仅查看页面：`npm run dev`，访问终端显示的本地地址。普通浏览器不能扫描本机；界面会明确显示预览模式。

## 构建

Windows 推荐使用安装包，安装后会创建“缓境 Huanj”开始菜单入口，可搜索中文名或英文名启动，并提供系统卸载入口：

```sh
npm run build:installer
```

安装包生成在 Cargo target 目录的 `release/bundle/nsis/`。使用当前用户安装模式。直接运行便携版 exe 不会自动创建开始菜单入口；需要被系统或第三方启动器发现时，应使用安装包。

```sh
npm run tauri build -- --no-bundle
```

产物位于 Cargo 配置的 target 目录下 `release/huanj.exe`（Windows）或 `release/huanj`（macOS）。如果没有全局 target 配置，则在项目的 `target/release` 中。

生成安装包使用 `npm run tauri build`。Windows 支持 MSI/NSIS；Mac 的应用打包和签名需要在 macOS 上完成。项目没有包含发布签名证书。

应用显示名称为“缓境 Huanj”，应用标识仍是 `dev.huanj.cleaner`，原有已保存扫描设置继续沿用。

`.github/workflows/check.yml` 提供 Windows x64、macOS Apple Silicon、macOS Intel 的测试与构建配置。配置不等于执行成功；本地 Windows 验证结果不能替代 Mac 构建或实机验收。

## 检查与基准

```sh
cargo test -p huanj-core
cargo test -p huanj --lib preferences::tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
npm run build
npm run test:ui
cargo run -p huanj-core --release --example benchmark
```

基准会在临时目录创建 120 个项目和 24,040 个目标文件，创建过程不计入扫描耗时。每次验证目标数、文件数、总字节数与预期一致，再报告耗时。临时目录只包含基准自己生成的数据。

布局回归在本地使用系统安装的 Google Chrome，在 CI 使用独立 Chromium。测试三个窗口尺寸、固定底栏、列表独立滚动和设置弹窗；不访问或删除真实磁盘文件。运行 `node scripts/audit-sizes.mjs <目录>` 可以用独立 Node.js 遍历核对逻辑大小。

Windows 可显式执行回收站集成检查：`cargo test -p huanj-core recycles_only_generated_fixture -- --ignored --nocapture`。该测试只将新生成的一份测试文件移入真实回收站，默认测试不会执行它。

## 统计口径与边界

显示值是普通文件的逻辑字节总和，使用 KiB/MiB/GiB（1024 进制），不等同于文件系统实际占用。链接不跟随、不计入大小；硬链接按路径计数，可能重复计算底层存储。

权限不足、文件读取失败或取消中的统计显示为不完整，不能清理。扫描正在变化的目录不具备文件系统快照语义，结果反映扫描期间观察到的文件元数据；清理前会重新验证目录与项目标识。

“修改日期”是目录修改时间，不是最后访问或最后使用时间。支持 workspace 的常规 `target`，不自动解析任意 Cargo `target-dir`。Python 仅识别含 `pyvenv.cfg` 的 `.venv`。

默认跳过系统目录、回收站、`.git`、链接及 macOS 应用包等范围。扫描所有磁盘不代表能够绕过系统权限、挂载状态或系统目录排除规则。

单盘失败会记录后继续。扫描按磁盘顺序进行，每个磁盘内部并发；慢盘或网络文件系统的阻塞调用可能延迟当前磁盘结束和取消，不能承诺立即中断系统 I/O。

清理仅接受本次扫描登记的候选项。Windows 使用 `IFileOperation` 回收标志和进度回调拒绝不可回收的删除；macOS 使用系统回收站适配。回收失败不主动改用永久删除。外置盘/网络盘不支持回收时会失败。

移入回收站不立即释放空间，需要之后由用户自行清空回收站。清理依赖后通常要重新安装或编译，虚拟环境中也可能含有用户手动保存的内容。

## 项目结构

```text
crates/cleaner-core/   独立 Rust 扫描、统计与清理引擎
src-tauri/            桌面进程、会话注册、命令和原生对话框
src/                  React 页面、筛选、虚拟列表与交互
docs/                 设计、实施与验证说明
```
