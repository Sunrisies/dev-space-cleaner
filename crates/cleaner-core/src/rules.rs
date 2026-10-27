//! Project-only rules. Name checks precede metadata reads; no project code is executed.
use crate::platform::is_link;
use serde::{Deserialize, Serialize};
use std::{fs, io::Read, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Node,
    Rust,
    Go,
    Python,
    Android,
    Java,
    Dotnet,
    Cpp,
    Swift,
    Dart,
    Php,
    Ruby,
    Scala,
    Elixir,
    Haskell,
    Ocaml,
    Zig,
}

impl Kind {
    pub fn from_id(id: &str) -> Option<Self> {
        serde_json::from_value(serde_json::Value::String(id.into())).ok()
    }

    pub fn all() -> Vec<Self> {
        let catalog: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(include_str!("../../../src/toolchains.json"))
                .expect("embedded toolchain catalog");
        catalog
            .keys()
            .map(|id| Self::from_id(id).expect("catalog kind"))
            .collect()
    }
}

pub(crate) struct Recognition {
    pub kind: Kind,
    pub rule: &'static str,
    pub evidence: &'static str,
}

// Reject links in every relative marker component, not just its final component.
fn metadata(base: &Path, relative: &str) -> Option<fs::Metadata> {
    let mut path = base.to_path_buf();
    let mut found = None;
    for component in Path::new(relative).components() {
        path.push(component);
        let meta = fs::symlink_metadata(&path).ok()?;
        if is_link(&meta) {
            return None;
        }
        found = Some(meta);
    }
    found
}
fn file(base: &Path, relative: &str) -> bool {
    metadata(base, relative).is_some_and(|m| m.is_file())
}
fn dir(base: &Path, relative: &str) -> bool {
    metadata(base, relative).is_some_and(|m| m.is_dir())
}
fn text(base: &Path, relative: &str) -> String {
    if !file(base, relative) {
        return String::new();
    }
    let mut result = String::new();
    if let Ok(f) = fs::File::open(base.join(relative)) {
        let _ = f.take(131_072).read_to_string(&mut result);
    }
    result
}
fn any_file(base: &Path, names: &[&str]) -> bool {
    names.iter().any(|n| file(base, n))
}
fn any_dir(base: &Path, names: &[&str]) -> bool {
    names.iter().any(|n| dir(base, n))
}
fn children_match(base: &Path, predicate: impl Fn(&str, &Path) -> bool) -> bool {
    fs::read_dir(base).ok().is_some_and(|entries| {
        entries.take(4096).filter_map(Result::ok).any(|e| {
            e.metadata().is_ok_and(|m| !is_link(&m))
                && e.file_name()
                    .to_str()
                    .is_some_and(|name| predicate(name, &e.path()))
        })
    })
}
fn project_extension(base: &Path, extensions: &[&str]) -> bool {
    children_match(base, |name, _| {
        extensions.iter().any(|ext| name.ends_with(ext)) && file(base, name)
    })
}
fn gradle(base: &Path) -> bool {
    any_file(base, &["build.gradle", "build.gradle.kts"])
}
fn android(base: &Path) -> bool {
    any_file(
        base,
        &[
            "src/main/AndroidManifest.xml",
            "app/src/main/AndroidManifest.xml",
        ],
    ) || ["build.gradle", "build.gradle.kts"].iter().any(|n| {
        let s = text(base, n);
        s.contains("com.android.") || s.contains("libs.plugins.android")
    })
}
fn gradle_kind(base: &Path) -> Kind {
    if android(base) {
        Kind::Android
    } else {
        Kind::Java
    }
}
fn python(base: &Path) -> bool {
    any_file(
        base,
        &[
            "pyproject.toml",
            "setup.py",
            "setup.cfg",
            "requirements.txt",
            "Pipfile",
        ],
    )
}
fn cache_tag(base: &Path) -> bool {
    text(base, "CACHEDIR.TAG").starts_with("Signature: 8a477f597d28d172789f06886806bc55")
}
fn dotnet_outputs(base: &Path) -> bool {
    // Inspect only configuration/framework levels, not an unbounded output tree.
    ["Debug", "Release"].iter().any(|config| {
        if !dir(base, config) {
            return false;
        }
        let output = base.join(config);
        let generated = |folder: &Path| {
            children_match(folder, |n, _| {
                (n.ends_with(".deps.json") || n.ends_with(".runtimeconfig.json")) && file(folder, n)
            })
        };
        generated(&output) || children_match(&output, |n, p| dir(&output, n) && generated(p))
    })
}
fn cmake_project(base: &Path) -> bool {
    let cache = text(base, "CMakeCache.txt");
    let Some(source) = cache
        .lines()
        .find_map(|line| line.strip_prefix("CMAKE_HOME_DIRECTORY:INTERNAL="))
    else {
        return false;
    };
    let source = Path::new(source.trim());
    source.is_absolute()
        && file(source, "CMakeLists.txt")
        && fs::canonicalize(source)
            .ok()
            .zip(fs::canonicalize(base).ok())
            .is_some_and(|(a, b)| a != b)
}

pub(crate) fn identify(path: &Path) -> Option<Recognition> {
    let parent = path.parent()?;
    let name = path.file_name()?.to_str()?;
    let (kind, rule, evidence) = match name {
        "node_modules" if file(parent, "package.json") => {
            (Kind::Node, "node-modules", "父目录包含 package.json")
        }
        "node_modules" if file(path, ".package-lock.json") => (
            Kind::Node,
            "node-npm",
            "包含 npm 安装标识 .package-lock.json",
        ),
        ".next"
            if file(parent, "package.json") && file(path, "BUILD_ID") && dir(path, "server") =>
        {
            (
                Kind::Node,
                "next-build",
                "Next.js BUILD_ID 与 server 构建目录",
            )
        }
        ".nuxt"
            if file(parent, "package.json")
                && any_file(
                    parent,
                    &["nuxt.config.ts", "nuxt.config.js", "nuxt.config.mjs"],
                )
                && file(path, "nuxt.d.ts") =>
        {
            (Kind::Node, "nuxt-build", "Nuxt 配置与生成的 nuxt.d.ts")
        }
        "target" if file(parent, "Cargo.toml") => {
            (Kind::Rust, "cargo-project", "父目录包含 Cargo.toml")
        }
        "target" if file(path, ".rustc_info.json") => {
            (Kind::Rust, "cargo-output", "包含 .rustc_info.json 构建标识")
        }
        "target"
            if file(parent, "pom.xml")
                && any_dir(
                    path,
                    &[
                        "classes",
                        "test-classes",
                        "maven-status",
                        "maven-archiver",
                        "surefire-reports",
                    ],
                ) =>
        {
            (Kind::Java, "maven-target", "pom.xml 与 Maven 生成目录")
        }
        "target"
            if (file(parent, "build.sbt")
                || (parent.file_name().is_some_and(|n| n == "project")
                    && parent.parent().is_some_and(|p| file(p, "build.sbt"))))
                && (any_dir(path, &["streams", "global-logging"])
                    || children_match(path, |n, _| n.starts_with("scala-") && dir(path, n))) =>
        {
            (Kind::Scala, "sbt-target", "sbt 项目定义与生成目录")
        }
        // Virtual environments may use any directory name, so their unique marker is checked below
        // only for the conventional names. Arbitrary names require explicit future rules.
        ".venv" | "venv" | "env" if file(path, "pyvenv.cfg") => (
            Kind::Python,
            "python-venv",
            "包含 pyvenv.cfg；虚拟环境需重新安装依赖",
        ),
        "__pycache__" if children_match(path, |n, _| n.ends_with(".pyc") && file(path, n)) => (
            Kind::Python,
            "python-bytecode",
            "Python 字节码目录包含 .pyc 文件",
        ),
        ".pytest_cache" | ".mypy_cache" | ".ruff_cache" if python(parent) && cache_tag(path) => (
            Kind::Python,
            "python-tool-cache",
            "Python 项目与标准 CACHEDIR.TAG 缓存标识",
        ),
        "build"
            if gradle(parent)
                && any_dir(
                    path,
                    &[
                        "classes",
                        "outputs",
                        "intermediates",
                        "tmp",
                        "generated",
                        "reports",
                        "test-results",
                        "kotlin",
                    ],
                ) =>
        {
            (
                gradle_kind(parent),
                "gradle-build",
                "Gradle 构建脚本与生成目录",
            )
        }
        ".gradle"
            if (gradle(parent)
                || any_file(parent, &["settings.gradle", "settings.gradle.kts"]))
                && file(path, "buildOutputCleanup/cache.properties") =>
        {
            (
                gradle_kind(parent),
                "gradle-project-cache",
                "Gradle 项目配置与 buildOutputCleanup/cache.properties",
            )
        }
        ".cxx"
            if gradle(parent)
                && android(parent)
                && any_dir(path, &["Debug", "Release", "RelWithDebInfo", "MinSizeRel"]) =>
        {
            (
                Kind::Android,
                "android-cxx",
                "Android Gradle 项目与本地构建配置目录",
            )
        }
        "obj"
            if project_extension(parent, &[".csproj", ".fsproj", ".vbproj"])
                && (file(path, "project.assets.json")
                    || children_match(path, |n, _| {
                        n.ends_with(".nuget.g.props") && file(path, n)
                    })) =>
        {
            (Kind::Dotnet, "dotnet-obj", ".NET 项目与 NuGet 生成标识")
        }
        "bin"
            if project_extension(parent, &[".csproj", ".fsproj", ".vbproj"])
                && dotnet_outputs(path) =>
        {
            (
                Kind::Dotnet,
                "dotnet-bin",
                ".NET 项目与生成的 deps/runtimeconfig 清单",
            )
        }
        "CMakeFiles"
            if file(parent, "CMakeCache.txt")
                && !file(parent, "CMakeLists.txt")
                && file(path, "cmake.check_cache")
                && cmake_project(parent) =>
        {
            (
                Kind::Cpp,
                "cmake-internals",
                "CMakeCache.txt 与 cmake.check_cache；保留外层构建配置",
            )
        }
        "meson-logs"
            if file(parent, "meson-private/coredata.dat")
                && file(path, "meson-log.txt")
                && file(parent, "meson-info/meson-info.json")
                && !file(parent, "meson.build") =>
        {
            (
                Kind::Cpp,
                "meson-logs",
                "Meson 构建标识与生成日志；保留 coredata 构建配置",
            )
        }
        ".build" if file(parent, "Package.swift") && file(path, "workspace-state.json") => (
            Kind::Swift,
            "swiftpm-build",
            "Package.swift 与 SwiftPM workspace-state.json",
        ),
        ".dart_tool" if file(parent, "pubspec.yaml") && file(path, "package_config.json") => (
            Kind::Dart,
            "dart-tool",
            "pubspec.yaml 与生成的 package_config.json",
        ),
        "build"
            if text(parent, "pubspec.yaml").contains("flutter:")
                && file(parent, ".metadata")
                && any_dir(
                    path,
                    &[
                        "flutter_assets",
                        "app",
                        "windows",
                        "macos",
                        "ios",
                        "web",
                        "linux",
                        "native_assets",
                    ],
                ) =>
        {
            (
                Kind::Dart,
                "flutter-build",
                "Flutter 项目标识与平台构建目录",
            )
        }
        "vendor"
            if file(parent, "composer.json")
                && any_file(path, &["composer/installed.json", "composer/installed.php"]) =>
        {
            (Kind::Php, "composer-vendor", "Composer 项目与安装清单")
        }
        "bundle"
            if parent.file_name().is_some_and(|n| n == "vendor")
                && parent.parent().is_some_and(|p| file(p, "Gemfile"))
                && dir(path, "ruby")
                && children_match(&path.join("ruby"), |_, p| dir(p, "specifications")) =>
        {
            (
                Kind::Ruby,
                "bundler-vendor",
                "Gemfile 与 vendor/bundle/ruby 的 gem 安装结构",
            )
        }
        "_build" if file(parent, "mix.exs") && children_match(path, |_, p| dir(p, "lib")) => {
            (Kind::Elixir, "mix-build", "mix.exs 与 Mix lib 产物")
        }
        "_build" if file(parent, "rebar.config") && children_match(path, |_, p| dir(p, "lib")) => (
            Kind::Elixir,
            "rebar-build",
            "rebar.config 与 Rebar3 lib 产物",
        ),
        "_build"
            if any_file(parent, &["dune-project", "dune-workspace"])
                && file(path, "log")
                && dir(path, "default") =>
        {
            (Kind::Ocaml, "dune-build", "Dune 项目与 log/default 产物")
        }
        ".stack-work" if file(parent, "stack.yaml") && dir(path, "dist") => {
            (Kind::Haskell, "stack-work", "stack.yaml 与 Stack dist 产物")
        }
        "dist-newstyle"
            if (file(parent, "cabal.project") || project_extension(parent, &[".cabal"]))
                && any_dir(path, &["cache", "build"]) =>
        {
            (
                Kind::Haskell,
                "cabal-build",
                "Cabal 项目与 dist-newstyle 生成结构",
            )
        }
        ".zig-cache" | "zig-cache"
            if file(parent, "build.zig") && any_dir(path, &["h", "o", "z"]) =>
        {
            (
                Kind::Zig,
                "zig-cache",
                "build.zig 与 Zig 缓存结构；保留 zig-out",
            )
        }
        _ => return None,
    };
    Some(Recognition {
        kind,
        rule,
        evidence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ScanEvent, ScanOptions, scan};
    use std::sync::{Arc, Mutex, atomic::AtomicBool};

    struct Case {
        folder: &'static str,
        kind: Kind,
        rule: &'static str,
        files: Vec<(&'static str, &'static str)>,
        dirs: Vec<&'static str>,
    }
    fn cases() -> Vec<Case> {
        use Kind::*;
        vec![
            Case {
                folder: "node_modules",
                kind: Node,
                rule: "node-modules",
                files: vec![("package.json", "{}")],
                dirs: vec![],
            },
            Case {
                folder: "node_modules",
                kind: Node,
                rule: "node-npm",
                files: vec![("node_modules/.package-lock.json", "{}")],
                dirs: vec![],
            },
            Case {
                folder: ".next",
                kind: Node,
                rule: "next-build",
                files: vec![("package.json", "{}"), (".next/BUILD_ID", "id")],
                dirs: vec![".next/server"],
            },
            Case {
                folder: ".nuxt",
                kind: Node,
                rule: "nuxt-build",
                files: vec![
                    ("package.json", "{}"),
                    ("nuxt.config.ts", "export default {}"),
                    (".nuxt/nuxt.d.ts", "// generated"),
                ],
                dirs: vec![],
            },
            Case {
                folder: "target",
                kind: Rust,
                rule: "cargo-project",
                files: vec![("Cargo.toml", "[workspace]")],
                dirs: vec![],
            },
            Case {
                folder: "target",
                kind: Rust,
                rule: "cargo-output",
                files: vec![("target/.rustc_info.json", "{}")],
                dirs: vec![],
            },
            Case {
                folder: ".venv",
                kind: Python,
                rule: "python-venv",
                files: vec![(".venv/pyvenv.cfg", "home = python")],
                dirs: vec![],
            },
            Case {
                folder: "venv",
                kind: Python,
                rule: "python-venv",
                files: vec![("venv/pyvenv.cfg", "home = python")],
                dirs: vec![],
            },
            Case {
                folder: "env",
                kind: Python,
                rule: "python-venv",
                files: vec![("env/pyvenv.cfg", "home = python")],
                dirs: vec![],
            },
            Case {
                folder: "__pycache__",
                kind: Python,
                rule: "python-bytecode",
                files: vec![("__pycache__/app.cpython-313.pyc", "bytecode")],
                dirs: vec![],
            },
            Case {
                folder: ".pytest_cache",
                kind: Python,
                rule: "python-tool-cache",
                files: vec![
                    ("pyproject.toml", "[project]"),
                    (
                        ".pytest_cache/CACHEDIR.TAG",
                        "Signature: 8a477f597d28d172789f06886806bc55",
                    ),
                ],
                dirs: vec![],
            },
            Case {
                folder: ".mypy_cache",
                kind: Python,
                rule: "python-tool-cache",
                files: vec![
                    ("pyproject.toml", "[project]"),
                    (
                        ".mypy_cache/CACHEDIR.TAG",
                        "Signature: 8a477f597d28d172789f06886806bc55",
                    ),
                ],
                dirs: vec![],
            },
            Case {
                folder: ".ruff_cache",
                kind: Python,
                rule: "python-tool-cache",
                files: vec![
                    ("pyproject.toml", "[project]"),
                    (
                        ".ruff_cache/CACHEDIR.TAG",
                        "Signature: 8a477f597d28d172789f06886806bc55",
                    ),
                ],
                dirs: vec![],
            },
            Case {
                folder: "target",
                kind: Java,
                rule: "maven-target",
                files: vec![("pom.xml", "<project/>")],
                dirs: vec!["target/maven-status"],
            },
            Case {
                folder: "build",
                kind: Java,
                rule: "gradle-build",
                files: vec![("build.gradle", "plugins { id 'java' }")],
                dirs: vec!["build/classes"],
            },
            Case {
                folder: "build",
                kind: Android,
                rule: "gradle-build",
                files: vec![(
                    "build.gradle.kts",
                    "plugins { id(\"com.android.application\") }",
                )],
                dirs: vec!["build/intermediates"],
            },
            Case {
                folder: ".gradle",
                kind: Java,
                rule: "gradle-project-cache",
                files: vec![
                    ("settings.gradle", "rootProject.name = 'demo'"),
                    (
                        ".gradle/buildOutputCleanup/cache.properties",
                        "gradle.version=8",
                    ),
                ],
                dirs: vec![],
            },
            Case {
                folder: ".gradle",
                kind: Android,
                rule: "gradle-project-cache",
                files: vec![
                    (
                        "build.gradle.kts",
                        "plugins { id(\"com.android.application\") }",
                    ),
                    (
                        ".gradle/buildOutputCleanup/cache.properties",
                        "gradle.version=8",
                    ),
                ],
                dirs: vec![],
            },
            Case {
                folder: ".cxx",
                kind: Android,
                rule: "android-cxx",
                files: vec![(
                    "build.gradle.kts",
                    "plugins { id(\"com.android.library\") }",
                )],
                dirs: vec![".cxx/Debug"],
            },
            Case {
                folder: "obj",
                kind: Dotnet,
                rule: "dotnet-obj",
                files: vec![
                    ("Demo.csproj", "<Project/ >"),
                    ("obj/project.assets.json", "{}"),
                ],
                dirs: vec![],
            },
            Case {
                folder: "bin",
                kind: Dotnet,
                rule: "dotnet-bin",
                files: vec![
                    ("Demo.fsproj", "<Project/>"),
                    ("bin/Debug/net8.0/Demo.deps.json", "{}"),
                ],
                dirs: vec![],
            },
            Case {
                folder: "build/CMakeFiles",
                kind: Cpp,
                rule: "cmake-internals",
                files: vec![
                    ("CMakeLists.txt", "project(demo)"),
                    ("build/CMakeFiles/cmake.check_cache", "# generated"),
                ],
                dirs: vec![],
            },
            Case {
                folder: "build/meson-logs",
                kind: Cpp,
                rule: "meson-logs",
                files: vec![
                    ("build/meson-private/coredata.dat", "data"),
                    ("build/meson-info/meson-info.json", "{}"),
                    ("build/meson-logs/meson-log.txt", "build log"),
                ],
                dirs: vec![],
            },
            Case {
                folder: ".build",
                kind: Swift,
                rule: "swiftpm-build",
                files: vec![
                    ("Package.swift", "// swift-tools-version: 6.0"),
                    (".build/workspace-state.json", "{}"),
                ],
                dirs: vec![],
            },
            Case {
                folder: ".dart_tool",
                kind: Dart,
                rule: "dart-tool",
                files: vec![
                    ("pubspec.yaml", "name: demo"),
                    (".dart_tool/package_config.json", "{}"),
                ],
                dirs: vec![],
            },
            Case {
                folder: "build",
                kind: Dart,
                rule: "flutter-build",
                files: vec![
                    ("pubspec.yaml", "flutter:\n  uses-material-design: true"),
                    (".metadata", "project_type: app"),
                ],
                dirs: vec!["build/app"],
            },
            Case {
                folder: "vendor",
                kind: Php,
                rule: "composer-vendor",
                files: vec![
                    ("composer.json", "{}"),
                    ("vendor/composer/installed.json", "[]"),
                ],
                dirs: vec![],
            },
            Case {
                folder: "vendor/bundle",
                kind: Ruby,
                rule: "bundler-vendor",
                files: vec![("Gemfile", "source 'https://rubygems.org'")],
                dirs: vec!["vendor/bundle/ruby/3.3.0/specifications"],
            },
            Case {
                folder: "target",
                kind: Scala,
                rule: "sbt-target",
                files: vec![("build.sbt", "scalaVersion := \"3.3.0\"")],
                dirs: vec!["target/streams"],
            },
            Case {
                folder: "_build",
                kind: Elixir,
                rule: "mix-build",
                files: vec![("mix.exs", "defmodule Demo.MixProject do end")],
                dirs: vec!["_build/dev/lib"],
            },
            Case {
                folder: "_build",
                kind: Elixir,
                rule: "rebar-build",
                files: vec![("rebar.config", "{erl_opts, [debug_info]}.")],
                dirs: vec!["_build/default/lib"],
            },
            Case {
                folder: ".stack-work",
                kind: Haskell,
                rule: "stack-work",
                files: vec![("stack.yaml", "resolver: lts-22.0")],
                dirs: vec![".stack-work/dist"],
            },
            Case {
                folder: "dist-newstyle",
                kind: Haskell,
                rule: "cabal-build",
                files: vec![("demo.cabal", "name: demo")],
                dirs: vec!["dist-newstyle/cache"],
            },
            Case {
                folder: "_build",
                kind: Ocaml,
                rule: "dune-build",
                files: vec![
                    ("dune-project", "(lang dune 3.0)"),
                    ("_build/log", "# dune"),
                ],
                dirs: vec!["_build/default"],
            },
            Case {
                folder: ".zig-cache",
                kind: Zig,
                rule: "zig-cache",
                files: vec![("build.zig", "pub fn build() void {}")],
                dirs: vec![".zig-cache/h"],
            },
            Case {
                folder: "zig-cache",
                kind: Zig,
                rule: "zig-cache",
                files: vec![("build.zig", "pub fn build() void {}")],
                dirs: vec!["zig-cache/o"],
            },
        ]
    }
    fn write(base: &Path, relative: &str, data: impl AsRef<[u8]>) {
        let path = base.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, data).unwrap();
    }
    fn fixture(base: &Path, case: &Case) {
        fs::create_dir_all(base.join(case.folder)).unwrap();
        for (name, data) in &case.files {
            write(base, name, data);
        }
        for name in &case.dirs {
            fs::create_dir_all(base.join(name)).unwrap();
        }
        if case.rule == "cmake-internals" {
            write(
                base,
                "build/CMakeCache.txt",
                format!("CMAKE_HOME_DIRECTORY:INTERNAL={}\n", base.display()),
            );
        }
        write(base, &format!("{}/payload", case.folder), [7u8; 317]);
    }
    fn logical_bytes(path: &Path) -> u64 {
        fs::read_dir(path)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    logical_bytes(&entry.path())
                } else {
                    fs::metadata(entry.path()).unwrap().len()
                }
            })
            .sum()
    }
    #[test]
    fn all_rules_measure_exactly_and_revalidate_evidence() {
        for case in cases() {
            let temp = tempfile::tempdir().unwrap();
            fixture(temp.path(), &case);
            let path = temp.path().join(case.folder);
            let recognized = identify(&path).unwrap_or_else(|| panic!("{}", case.rule));
            assert_eq!((recognized.kind, recognized.rule), (case.kind, case.rule));
            let items = Mutex::new(Vec::new());
            let summary = scan(
                ScanOptions {
                    roots: vec![temp.path().into()],
                    kinds: Kind::all(),
                    workers: 4,
                },
                Arc::new(AtomicBool::new(false)),
                |event| {
                    if let ScanEvent::Sized(item) = event {
                        items.lock().unwrap().push(item);
                    }
                },
            );
            let items = items.into_inner().unwrap();
            assert_eq!(items.len(), 1, "{}", case.rule);
            assert_eq!(items[0].bytes, logical_bytes(&path), "{}", case.rule);
            assert_eq!(summary.progress.bytes, items[0].bytes);
            assert!(crate::cleaner::validate(&items[0]).is_ok(), "{}", case.rule);
            // Each file marker is required for this fixture; replacing it by an unrelated
            // file must make the registered candidate ineligible for cleanup.
            for (marker, content) in &case.files {
                fs::remove_file(temp.path().join(marker)).unwrap();
                assert!(
                    crate::cleaner::validate(&items[0]).is_err(),
                    "{} missing {marker}",
                    case.rule
                );
                write(temp.path(), marker, content);
            }
            for marker in &case.dirs {
                fs::remove_dir(temp.path().join(marker)).unwrap();
                assert!(
                    crate::cleaner::validate(&items[0]).is_err(),
                    "{} missing {marker}",
                    case.rule
                );
                fs::create_dir_all(temp.path().join(marker)).unwrap();
            }
        }
    }
    #[test]
    fn ordinary_same_names_and_wrong_cache_tags_are_not_candidates() {
        let temp = tempfile::tempdir().unwrap();
        for name in [
            "build",
            "target",
            "bin",
            "obj",
            "vendor",
            "bundle",
            "_build",
            ".gradle",
            ".cxx",
            ".build",
            ".dart_tool",
            ".stack-work",
            "dist-newstyle",
            ".zig-cache",
            "zig-cache",
            ".next",
            ".nuxt",
            "CMakeFiles",
            "meson-private",
            "meson-logs",
            "__pycache__",
            "env",
            "venv",
            ".venv",
        ] {
            let path = temp.path().join(name);
            fs::create_dir_all(&path).unwrap();
            write(&path, "source.txt", "keep me");
            assert!(identify(&path).is_none(), "{name}");
        }
        write(temp.path(), "pyproject.toml", "[project]");
        write(temp.path(), ".pytest_cache/CACHEDIR.TAG", "not a cache tag");
        assert!(identify(&temp.path().join(".pytest_cache")).is_none());
        write(temp.path(), "build.gradle", "plugins { id 'java' }");
        assert!(identify(&temp.path().join("build")).is_none());
        write(temp.path(), "Demo.csproj", "<Project/>");
        assert!(identify(&temp.path().join("bin")).is_none());
        assert!(identify(&temp.path().join("obj")).is_none());
    }
    #[test]
    fn shared_names_and_classification_changes_are_handled_deterministically() {
        let temp = tempfile::tempdir().unwrap();
        write(temp.path(), "pom.xml", "<project/>");
        fs::create_dir_all(temp.path().join("target/classes")).unwrap();
        assert_eq!(
            identify(&temp.path().join("target")).unwrap().kind,
            Kind::Java
        );
        write(temp.path(), "Cargo.toml", "[workspace]");
        assert_eq!(
            identify(&temp.path().join("target")).unwrap().kind,
            Kind::Rust
        );
        write(temp.path(), "build.gradle", "plugins { id 'java' }");
        fs::create_dir_all(temp.path().join("build/classes")).unwrap();
        assert_eq!(
            identify(&temp.path().join("build")).unwrap().kind,
            Kind::Java
        );
        write(
            temp.path(),
            "build.gradle",
            "plugins { id 'com.android.application' }",
        );
        assert_eq!(
            identify(&temp.path().join("build")).unwrap().kind,
            Kind::Android
        );
    }
    #[test]
    fn catalog_ids_round_trip_and_unknown_types_are_rejected() {
        let all = Kind::all();
        assert_eq!(all.len(), 17);
        for kind in all {
            let id = serde_json::to_value(kind).unwrap();
            assert_eq!(Kind::from_id(id.as_str().unwrap()), Some(kind));
        }
        assert!(Kind::from_id("build").is_none());
        assert_eq!(Kind::from_id("go"), Some(Kind::Go));
    }
    #[test]
    fn dotnet_outputs_do_not_depend_on_other_cleanup_candidates() {
        let temp = tempfile::tempdir().unwrap();
        write(temp.path(), "Demo.csproj", "<Project/>");
        write(temp.path(), "obj/project.assets.json", "{}");
        write(temp.path(), "bin/Debug/net8.0/Demo.deps.json", "{}");
        assert!(identify(&temp.path().join("bin")).is_some());
        fs::remove_file(temp.path().join("obj/project.assets.json")).unwrap();
        assert!(identify(&temp.path().join("bin")).is_some());
    }
    #[test]
    fn nested_generated_directories_are_not_counted_twice_even_when_disabled() {
        let temp = tempfile::tempdir().unwrap();
        write(temp.path(), "package.json", "{}");
        write(temp.path(), "node_modules/pkg/pom.xml", "<project/>");
        write(
            temp.path(),
            "node_modules/pkg/target/classes/demo.class",
            [0u8; 321],
        );
        for kinds in [Kind::all(), vec![Kind::Java]] {
            let summary = scan(
                ScanOptions {
                    roots: vec![temp.path().into()],
                    kinds: kinds.clone(),
                    workers: 8,
                },
                Arc::new(AtomicBool::new(false)),
                |_| {},
            );
            assert_eq!(
                summary.progress.found,
                u64::from(kinds.contains(&Kind::Node))
            );
            assert_eq!(
                summary.progress.bytes,
                if kinds.contains(&Kind::Node) { 331 } else { 0 }
            );
        }
    }
    #[cfg(unix)]
    #[test]
    fn linked_marker_components_are_not_followed() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        write(temp.path(), "project/composer.json", "{}");
        write(temp.path(), "outside/installed.json", "[]");
        fs::create_dir_all(temp.path().join("project/vendor")).unwrap();
        symlink(
            temp.path().join("outside"),
            temp.path().join("project/vendor/composer"),
        )
        .unwrap();
        assert!(identify(&temp.path().join("project/vendor")).is_none());
        write(temp.path(), "project/mix.exs", "defmodule Demo do end");
        fs::create_dir_all(temp.path().join("outside/lib")).unwrap();
        fs::create_dir_all(temp.path().join("project/_build")).unwrap();
        symlink(
            temp.path().join("outside"),
            temp.path().join("project/_build/dev"),
        )
        .unwrap();
        assert!(identify(&temp.path().join("project/_build")).is_none());
    }
}
