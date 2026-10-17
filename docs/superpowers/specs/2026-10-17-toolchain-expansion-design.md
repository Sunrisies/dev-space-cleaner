# 常见开发工具链扩展方案

## 目标与方案选择

保留 Windows/macOS、默认所有可访问磁盘、保存扫描范围、逐项大小、分类汇总、目录菜单和仅回收站清理的行为。

三种可选方案：只增加 Android/Java，改动少但后续仍需反复扩展；采用内置工具链规则目录，覆盖常用生态并统一扫描/清理校验（推荐）；允许用户任意输入目录名作为清理规则，灵活但难以验证，不纳入本次。

用户已确认范围为项目依赖和构建产物。全局缓存单独选择，默认关闭。本轮实现项目规则和分类目录；全局清理规则另行实现，在实际可用前不提供可开启的空开关。下表是识别范围，不表示同名目录都可清理；具体条目必须通过对应项目和产物标识校验。

## 工具链目录

| 分类 | 拟识别范围 | 主要约束 |
|---|---|---|
| JavaScript/TypeScript | node_modules；Next/Nuxt 等明确框架生成目录 | package.json、安装标识或对应框架配置；不泛匹配 dist |
| Rust | target | Cargo.toml 或 Rust 构建标识 |
| Python | 虚拟环境、__pycache__、pytest/mypy/ruff 缓存 | pyvenv.cfg 或工具特有标识；虚拟环境明确标为依赖环境 |
| Android | Gradle 项目缓存、模块 build、.cxx | Android 插件/项目证据，支持多模块；不包含 SDK/NDK/模拟器 |
| Java/Kotlin | Maven target、Gradle build/.gradle | pom.xml、Gradle 配置及生成标识；不得与 Android 重复计数 |
| C#/F#/VB .NET | bin、obj、SDK 集中产物目录 | 项目文件和对应生成结构，不能仅凭 bin/obj 名称 |
| C/C++ | CMake/Meson 的明确生成目录 | 排除源码树；缓存包含本地配置时提示重配影响；不泛匹配 build |
| Go | 全局构建缓存（单独开关） | 无通用项目缓存目录；不清理源码 vendor、安装的 bin |
| Swift/Objective-C | SwiftPM .build；Xcode DerivedData（单独开关） | Swift 包/工具特有结构；不清理归档、签名和设备支持文件 |
| Dart/Flutter | .dart_tool、Flutter build | pubspec 和工具生成标识 |
| PHP | Composer vendor | composer.json 与 Composer 安装标识；不泛匹配 vendor |
| Ruby | Bundler 安装的 vendor/bundle | Gemfile 与 Bundler 安装结构；保留 .bundle 配置和本地源码依赖 |
| Scala | sbt target | build.sbt/project 构建定义；与 Maven/Rust 区分 |
| Elixir/Erlang | Mix _build、Rebar3 _build | mix.exs/rebar.config 与工具生成结构 |
| Haskell | Stack .stack-work、Cabal dist-newstyle | stack.yaml/cabal.project/包定义与生成结构 |
| OCaml | Dune _build | dune-project/工作区定义与生成结构 |
| Zig | .zig-cache、兼容旧版本 zig-cache | build.zig 或工具标识；zig-out 不默认纳入 |
| R/Julia/Perl/Lua/Nim | 列为后续规则候选 | 本轮不按语言名假定存在统一安全缓存目录，不提供空壳支持 |

## 全局缓存边界

后续/独立开关可包括 npm/pnpm/Yarn、pip/uv、Gradle、NuGet、Go、Dart、Composer、Bundler、Xcode 的可验证缓存子目录。扫描已知位置及环境变量配置，不执行项目脚本，不按目录名全盘猜测。

Maven 本地仓库、Cargo registry/git、Go 模块下载仓库等可能包含离线或本地资源，单列为需要额外说明的下载仓库，不混入默认项目清理。不删除整个 .gradle、.m2、.cargo，也不删除 JDK、SDK、工具链和配置文件。

## 实现约束

- 核心维护稳定的分类 ID 与规则 ID；规则定义名称预筛选、项目证据、产物证据、支持平台、作用范围和恢复说明。前端从统一目录获得类型信息。
- 一次遍历完成全部已选类型识别，先按文件名筛选，再做必要标识读取；共享/缓存项目上下文，保持有界并发。默认规则不执行 Gradle 等脚本。
- 候选携带具体规则 ID，删除前按同一规则重验。只清理已完整测量、身份和路径校验通过的候选，保留现有链接保护和回收站限制。
- 同一路径只计一次，父候选与子候选不能同时计入可清理总量。分类优先级确定，并能解释识别证据；无法确定归属时不猜测。
- 新增 Python 小缓存可显著增加候选数，继续采用虚拟列表和节流推送；不逐文件发送 UI 事件。
- 类型选择放入可搜索列表，主页面显示已选数量；分类汇总列表内部滚动，继续保持主页面无外部滚动条。
- 保存配置采用向后兼容迁移。旧版三类选择保留；升级不悄悄启用额外清理范围；新安装使用已验证的项目规则默认集合。

## 验证与验收

每条实际启用的规则须有正例、同名源码/普通目录反例、缺失标识和链接反例。测试混合语言、多模块、嵌套、重复根、旧设置迁移及清理前标识变化。无法可靠识别的规则维持关闭，不以支持列表代替实现。

用生成的数据集校验逐项字节数、分类合计和总计；对同一静态数据集在启用旧三类规则时比较结果及扫描时间，再单独报告新增规则成本。磁盘变化和系统缓存影响单独说明，不宣称任何环境绝对最快。不得清理真实用户项目作为测试。

UI 验证小窗口类型选择、分类汇总、结果筛选和目录菜单，确保仅列表/弹窗内容滚动。Windows 本机验证；macOS 需 CI 和对应系统实际验证，报告未覆盖项。

## 官方依据

- Gradle 项目目录和用户目录：https://docs.gradle.org/current/userguide/directory_layout.html
- Maven 标准目录：https://maven.apache.org/guides/introduction/introduction-to-the-standard-directory-layout.html
- .NET 产物目录：https://learn.microsoft.com/en-us/dotnet/core/sdk/artifacts-output
- Flutter clean：https://docs.flutter.dev/reference/flutter-cli
- Swift 命令：https://docs.swift.org/latest/documentation/userdocs/commands/
- CMake 缓存包含用户配置：https://cmake.org/cmake/help/book/mastering-cmake/chapter/CMake%20Cache.html
- Go 构建和模块缓存：https://go.dev/cmd/go/ 、https://go.dev/ref/mod
- Bundler 部署及本地依赖：https://bundler.io/guides/deploying.html 、https://bundler.io/guides/gemfile.html
- sbt clean：https://www.scala-sbt.org/1.x/docs/Command-Line-Reference.html
- Mix clean：https://mix.hexdocs.pm/Mix.Tasks.Clean.html
- Stack clean：https://docs.haskellstack.org/en/latest/commands/clean_command/
- Dune：https://dune.readthedocs.io/en/stable/reference/cli.html
- Zig 构建系统：https://ziglang.org/learn/build-system/

扫描范围已于 2026-10-17 经用户确认。本文待最终审阅；尚未修改应用代码或发布版本。
