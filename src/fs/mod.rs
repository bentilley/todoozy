mod walk;

pub use walk::{Walk, WalkConfig};

#[derive(Debug, PartialEq)]
pub enum FileType {
    Bash,
    C,
    Csharp,
    Css,
    Cpp,
    Dockerfile,
    Go,
    Html,
    Java,
    JavaScript,
    Kotlin,
    Ksh,
    Less,
    Makefile,
    Markdown,
    Php,
    Protobuf,
    Python,
    Ruby,
    Rust,
    Scss,
    Sql,
    Sh,
    Swift,
    Terraform,
    Todoozy,
    Typescript,
    YAML,
    Zsh,
}

impl FileType {
    /// Returns glob patterns for all supported file types.
    /// Used for filtering git diffs to only relevant files.
    /// Keep in sync with get_filetype_from_name below.
    pub fn supported_pathspecs() -> &'static [&'static str] {
        &[
            "*.bash",
            "*.c",
            "*.cc",
            "*.cs",
            "*.css",
            "*.cpp",
            "*.cxx",
            "*.dockerfile",
            "*.go",
            "*.h",
            "*.hh",
            "*.hpp",
            "*.htm",
            "*.html",
            "*.hxx",
            "*.java",
            "*.js",
            "*.jsx",
            "*.kt",
            "*.ksh",
            "*.kts",
            "*.less",
            "*.md",
            "*.mk",
            "*.php",
            "*.proto",
            "*.rb",
            "*.py",
            "*.rs",
            "*.scss",
            "*.sql",
            "*.sh",
            "*.swift",
            "*.tdz",
            "*.tf",
            "*.ts",
            "*.tsx",
            "*.yaml",
            "*.yml",
            "*.zsh",
            "Dockerfile",
            "Makefile",
            "makefile",
            "GNUmakefile",
            ".tdz",
        ]
    }
}

pub trait FileTypeAwarePath {
    fn get_filetype(&self) -> Option<FileType> {
        self.get_filetype_from_name()
            .or_else(|| self.get_filetype_from_contents())
    }
    fn get_filetype_from_name(&self) -> Option<FileType>;
    fn get_filetype_from_contents(&self) -> Option<FileType>;
}

impl FileTypeAwarePath for std::path::Path {
    fn get_filetype_from_name(&self) -> Option<FileType> {
        use FileType::*;
        match self.extension().and_then(std::ffi::OsStr::to_str) {
            Some("bash") => Some(Bash),
            Some("c") | Some("h") => Some(C),
            Some("cs") => Some(Csharp),
            Some("css") => Some(Css),
            Some("cc") | Some("cpp") | Some("cxx") | Some("hh") | Some("hpp") | Some("hxx") => {
                Some(Cpp)
            }
            Some("dockerfile") => Some(Dockerfile),
            Some("go") => Some(Go),
            Some("html") | Some("htm") => Some(Html),
            Some("java") => Some(Java),
            Some("js") | Some("jsx") => Some(JavaScript),
            Some("kt") | Some("kts") => Some(Kotlin),
            Some("ksh") => Some(Ksh),
            Some("less") => Some(Less),
            Some("md") => Some(Markdown),
            Some("mk") => Some(Makefile),
            Some("php") => Some(Php),
            Some("proto") => Some(Protobuf),
            Some("rb") => Some(Ruby),
            Some("py") => Some(Python),
            Some("rs") => Some(Rust),
            Some("scss") => Some(Scss),
            Some("sql") => Some(Sql),
            Some("sh") => Some(Sh),
            Some("swift") => Some(Swift),
            Some("tdz") => Some(Todoozy),
            Some("tf") => Some(Terraform),
            Some("ts") | Some("tsx") => Some(Typescript),
            Some("yaml") | Some("yml") => Some(YAML),
            Some("zsh") => Some(Zsh),
            _ => match self.file_name()?.to_str()? {
                "Dockerfile" => Some(Dockerfile),
                "Makefile" | "makefile" | "GNUmakefile" => Some(Makefile),
                ".tdz" => Some(Todoozy),
                _ => None,
            },
        }
    }

    fn get_filetype_from_contents(&self) -> Option<FileType> {
        use std::io::{BufRead, BufReader};
        use FileType::*;

        let file = std::fs::File::open(self).ok()?;
        let mut reader = BufReader::new(file);
        let mut first_line = String::new();
        reader.read_line(&mut first_line).ok()?;

        if !first_line.starts_with("#!") {
            return None;
        }

        // Match shebangs like #!/bin/bash, #!/usr/bin/env bash, etc.
        if first_line.contains("bash") {
            Some(Bash)
        } else if first_line.contains("zsh") {
            Some(Zsh)
        } else if first_line.contains("ksh") {
            Some(Ksh)
        } else if first_line.contains("/sh")
            || first_line.ends_with(" sh\n")
            || first_line.ends_with(" sh")
        {
            Some(Sh)
        } else {
            None
        }
    }
}

#[test]
fn test_get_filetype_from_name() {
    use std::path::Path;
    use FileType::*;
    assert_eq!(Path::new("dir/test.tdz").get_filetype_from_name(), Some(Todoozy));
    assert_eq!(Path::new("test.tdz").get_filetype_from_name(), Some(Todoozy));
    assert_eq!(Path::new("dir/.tdz").get_filetype_from_name(), Some(Todoozy));
    assert_eq!(Path::new("./.tdz").get_filetype_from_name(), Some(Todoozy));
    assert_eq!(Path::new(".tdz").get_filetype_from_name(), Some(Todoozy));
    assert_eq!(Path::new("test.rs").get_filetype_from_name(), Some(Rust));
    assert_eq!(Path::new("test.go").get_filetype_from_name(), Some(Go));
    assert_eq!(Path::new("test.md").get_filetype_from_name(), Some(Markdown));
    assert_eq!(Path::new("test.java").get_filetype_from_name(), Some(Java));
    assert_eq!(Path::new("test.kt").get_filetype_from_name(), Some(Kotlin));
    assert_eq!(Path::new("test.kts").get_filetype_from_name(), Some(Kotlin));
    assert_eq!(Path::new("test.rb").get_filetype_from_name(), Some(Ruby));
    assert_eq!(Path::new("test.py").get_filetype_from_name(), Some(Python));
    assert_eq!(Path::new("test.tf").get_filetype_from_name(), Some(Terraform));
    assert_eq!(Path::new("test.css").get_filetype_from_name(), Some(Css));
    assert_eq!(Path::new("test.scss").get_filetype_from_name(), Some(Scss));
    assert_eq!(Path::new("test.less").get_filetype_from_name(), Some(Less));
    assert_eq!(Path::new("test.yaml").get_filetype_from_name(), Some(YAML));
    assert_eq!(Path::new("test.yml").get_filetype_from_name(), Some(YAML));
    assert_eq!(Path::new("test.php").get_filetype_from_name(), Some(Php));
    assert_eq!(Path::new("test.proto").get_filetype_from_name(), Some(Protobuf));
    assert_eq!(Path::new("test.sql").get_filetype_from_name(), Some(Sql));
    assert_eq!(Path::new("test.mk").get_filetype_from_name(), Some(Makefile));
    assert_eq!(Path::new("Makefile").get_filetype_from_name(), Some(Makefile));
    assert_eq!(Path::new("makefile").get_filetype_from_name(), Some(Makefile));
    assert_eq!(Path::new("GNUmakefile").get_filetype_from_name(), Some(Makefile));
    assert_eq!(Path::new("Dockerfile").get_filetype_from_name(), Some(Dockerfile));
    assert_eq!(Path::new("test.dockerfile").get_filetype_from_name(), Some(Dockerfile));
    assert_eq!(Path::new("test.sh").get_filetype_from_name(), Some(Sh));
    assert_eq!(Path::new("test.swift").get_filetype_from_name(), Some(Swift));
    assert_eq!(Path::new("test.bash").get_filetype_from_name(), Some(Bash));
    assert_eq!(Path::new("test.zsh").get_filetype_from_name(), Some(Zsh));
    assert_eq!(Path::new("test.ksh").get_filetype_from_name(), Some(Ksh));
    assert_eq!(Path::new("test.ts").get_filetype_from_name(), Some(Typescript));
    assert_eq!(Path::new("test.tsx").get_filetype_from_name(), Some(Typescript));
    assert_eq!(Path::new("test.js").get_filetype_from_name(), Some(JavaScript));
    assert_eq!(Path::new("test.jsx").get_filetype_from_name(), Some(JavaScript));
    // C files
    assert_eq!(Path::new("test.c").get_filetype_from_name(), Some(C));
    assert_eq!(Path::new("test.h").get_filetype_from_name(), Some(C));
    // C++ files
    assert_eq!(Path::new("test.cc").get_filetype_from_name(), Some(Cpp));
    assert_eq!(Path::new("test.cpp").get_filetype_from_name(), Some(Cpp));
    assert_eq!(Path::new("test.cxx").get_filetype_from_name(), Some(Cpp));
    assert_eq!(Path::new("test.hh").get_filetype_from_name(), Some(Cpp));
    assert_eq!(Path::new("test.hpp").get_filetype_from_name(), Some(Cpp));
    assert_eq!(Path::new("test.hxx").get_filetype_from_name(), Some(Cpp));
    // C# files
    assert_eq!(Path::new("test.cs").get_filetype_from_name(), Some(Csharp));
    // HTML files
    assert_eq!(Path::new("test.html").get_filetype_from_name(), Some(Html));
    assert_eq!(Path::new("test.htm").get_filetype_from_name(), Some(Html));
    // Unknown extension returns None (no shebang fallback)
    assert_eq!(Path::new("test").get_filetype_from_name(), None);
    assert_eq!(Path::new("script").get_filetype_from_name(), None);
    assert_eq!(Path::new("test.unknown").get_filetype_from_name(), None);
}
