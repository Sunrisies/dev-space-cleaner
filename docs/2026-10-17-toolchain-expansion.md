# 0.2.0 常见工具链扩展与验证

## 实际交付

项目扫描从三类扩展为 16 类：Node.js（JS/TS）、Rust、Python、Android、Java/Kotlin、.NET（C#/F#/VB）、C/C++、Swift/Objective-C、Dart/Flutter、PHP、Ruby、Scala、Elixir/Erlang、Haskell、OCaml、Zig。实际目录清单见 README 和 src/toolchains.json。

分类 ID 与旧版兼容。前端展示与设置验证使用统一 JSON 目录，Rust 类型目录从同一文件核对。扫描和清理共用 rules.rs，候选保存规则 ID，删除前同时验证类型和原规则，项目标识变化时拒绝操作。

扫描继续使用单次遍历、有界线程池与节流事件。识别出依赖/生成目录后不再产生嵌套候选，已禁用类型的生成目录也不再深入识别，避免重复计量其内部依赖。每次清理仍限当前会话中的完整测量结果，仅尝试系统回收站。

类型选择位于设置弹窗，支持搜索语言、工具链和目录名，支持选择或取消当前搜索结果。左侧汇总列表独立滚动，结果使用类型下拉筛选；每行展示实际目录名，不再把一种语言固定成单个目录。仍保留逐项大小、每类数量/大小和右击打开目录。

未选择的类型标为“未扫描”，与扫描后 0 B 区分。本次扫描的类型集合单独保留，扫描结束后调整下一次的类型选择，不会改写本次结果的状态。

旧设置的类型选择、目录和线程数原样保留。升级后需要在设置中选择新增类型并保存；无保存设置的新安装默认启用 16 类项目规则。不会自动开始扫描，也不会自动勾选清理项。

## 规则边界

- 全局缓存不属于本次实现；没有开放一个实际上不生效的全局缓存开关。Go、Xcode DerivedData 和 npm/pip/Gradle 等全局缓存另行实现。
- 不删除整个 .gradle 用户目录、.m2、.cargo、SDK/NDK/JDK、模拟器、签名和工具安装。
- CMake 只清理有 CMakeCache、源项目和生成标识支持的 CMakeFiles；保留 CMakeCache.txt。Meson 只识别生成日志目录，保留 coredata 构建配置。
- .NET 识别项目旁常规 bin/obj，bin 使用自己的生成清单，不依赖另一个待清理 obj 的存在。尚不识别集中 artifacts 和任意自定义输出布局。
- 不运行 Gradle、Maven、项目脚本或构建命令，不推断所有自定义输出路径。项目和产物证据不足时不会显示候选。
- 保留逻辑字节统计口径，不能把汇总理解为回收后立即释放的真实磁盘空间。

## 测试

Windows 本机：Rust workspace 测试 24 项通过，3 项真实回收站测试继续默认忽略，本次没有删除真实项目目录。新增规则测试覆盖 36 组目录样例，包括逐项精确字节、标识文件/目录消失、普通同名目录、混合工具链分类、嵌套去重和 .NET 清理顺序。Unix 链接标识反例已加入，等待对应系统运行。

UI 14 项通过，覆盖五种窗口尺寸（含 960×570）、无外层滚动、类型搜索/持久化、旧三类配置保持、分类合计、实际目录名、右击打开和失败反馈。截图来自模拟 IPC 的生成样例，不是真实用户扫描截图：

- screenshots/toolchain-results.png
- screenshots/toolchain-settings.png

前端生产构建、Rust fmt 和 workspace clippy（所有 targets，警告视为错误）通过。macOS 只有跨平台实现和 CI 配置，本次未执行 Mac 构建/实机验证。

## D 盘只读对照

八线程、相同 D 盘，旧版扫描器与新版扫描器顺序运行；测量阶段没有并行执行构建/测试任务。系统缓存、杀毒软件和文件变化仍可能影响结果，单组对照不是普遍性能保证。

| 指标 | 原三类扫描器 | 扩展扫描器（全部项目类型） |
|---|---:|---:|
| 耗时 | 33.584 秒 | 35.242 秒 |
| 已检查目录 | 564,898 | 564,898 |
| 识别目录 | 638 | 2,482 |
| 统计文件 | 2,628,141 | 2,786,379 |
| 逻辑字节 | 71,929,082,046 | 102,715,134,105 |
| 读取错误 | 0 | 0 |
| 跳过链接 | 38,317 | 38,317 |

本组耗时增加约 4.9%，新增 1,844 个候选。634 个共有路径的类型、字节数、文件数、完整状态、错误与链接计数全部相同；另外 4 个旧候选被新识别的外层虚拟环境覆盖，没有独立重复计数。未发现未覆盖的旧候选。

新扫描实际发现 7 类：Node.js 604、Rust 16、Python 1,707、Android 65、Java/Kotlin 80、Dart/Flutter 5、PHP 5。其他类型由生成样例验证，不冒充真实磁盘验证。

独立 Node.js lstat/BigInt 脚本对这 7 类各抽查最大的一个完整目录，全部字节数、文件数、跳过链接数和错误数与 Rust 扫描一致，抽查无读取错误。这是样本核对，不等同于对所有新候选作独立全量验证。

初轮旧扫描器与构建活动有重叠，耗时 65.366 秒；该轮不用于性能结论。初轮全类型结果与最终顺序测量结果一致。

原始 JSONL 含个人项目路径，保留在忽略的 qa-fixture/results 中，不提交。复现工具：

```text
cargo run -p huanj-core --release --example scan_roots -- --workers 8 D:\
cargo run -p huanj-core --release --example scan_roots -- --legacy-types --workers 8 D:\
node scripts/report-expansion.mjs before.jsonl after.jsonl [audit.jsonl]
node scripts/audit-sizes.mjs --scan after.jsonl
```

--legacy-types 使用新版引擎的 Node/Rust/Python 规则，因此包含新增 Python 等子规则；真正旧版对照使用修改前保存的扫描器二进制。

## Windows 发布与升级

0.2.0 NSIS 安装包构建成功，已复制到 releases/huanj-0.2.0-setup.exe。检测到旧程序未运行后完成当前用户静默升级，安装器退出码 0，安装后 exe 产品版本为 0.2.0。升级前后已存在的 scan-settings.json 的 SHA-256 相同，开始菜单快捷方式和 dev.huanj.cleaner 系统搜索入口保留。没有强制结束用户进程。

Windows 本机已安装验证；没有生成或安装 macOS 包。
