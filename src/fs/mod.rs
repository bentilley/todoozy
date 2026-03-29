mod walk;

pub use walk::{Walk, WalkConfig};

#[derive(Debug, PartialEq)]
pub enum FileType {
    Bash,
    Dockerfile,
    Go,
    Ksh,
    Makefile,
    Markdown,
    Protobuf,
    Python,
    Rust,
    Sh,
    Terraform,
    Todoozy,
    Typescript,
    YAML,
    Zsh,
}

pub fn get_filetype(filename: &str) -> Option<FileType> {
    use FileType::*;

    let path = std::path::Path::new(filename);

    let ft = match path.extension().and_then(std::ffi::OsStr::to_str) {
        Some("bash") => Some(Bash),
        Some("dockerfile") => Some(Dockerfile),
        Some("go") => Some(Go),
        Some("ksh") => Some(Ksh),
        Some("md") => Some(Markdown),
        Some("mk") => Some(Makefile),
        Some("proto") => Some(Protobuf),
        Some("py") => Some(Python),
        Some("rs") => Some(Rust),
        Some("sh") => Some(Sh),
        Some("tdz") => Some(Todoozy),
        Some("tf") => Some(Terraform),
        Some("ts") | Some("tsx") => Some(Typescript),
        Some("yaml") | Some("yml") => Some(YAML),
        Some("zsh") => Some(Zsh),
        _ => None,
    };

    if ft.is_some() {
        return ft;
    }

    match path.file_name().and_then(std::ffi::OsStr::to_str) {
        Some(".tdz") => Some(Todoozy),
        Some("Dockerfile") => Some(Dockerfile),
        Some("Makefile") | Some("makefile") | Some("GNUmakefile") => Some(Makefile),
        _ => get_filetype_from_shebang(filename),
    }
}

fn get_filetype_from_shebang(filename: &str) -> Option<FileType> {
    use std::io::{BufRead, BufReader};
    use FileType::*;

    let file = std::fs::File::open(filename).ok()?;
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

#[test]
fn test_get_extension_from_filename() {
    use FileType::*;
    assert_eq!(get_filetype("dir/test.tdz"), Some(Todoozy));
    assert_eq!(get_filetype("test.tdz"), Some(Todoozy));
    assert_eq!(get_filetype("dir/.tdz"), Some(Todoozy));
    assert_eq!(get_filetype("./.tdz"), Some(Todoozy));
    assert_eq!(get_filetype(".tdz"), Some(Todoozy));
    assert_eq!(get_filetype("test.rs"), Some(Rust));
    assert_eq!(get_filetype("test.go"), Some(Go));
    assert_eq!(get_filetype("test.md"), Some(Markdown));
    assert_eq!(get_filetype("test.py"), Some(Python));
    assert_eq!(get_filetype("test.tf"), Some(Terraform));
    assert_eq!(get_filetype("test.yaml"), Some(YAML));
    assert_eq!(get_filetype("test.yml"), Some(YAML));
    assert_eq!(get_filetype("test.proto"), Some(Protobuf));
    assert_eq!(get_filetype("test.mk"), Some(Makefile));
    assert_eq!(get_filetype("Makefile"), Some(Makefile));
    assert_eq!(get_filetype("makefile"), Some(Makefile));
    assert_eq!(get_filetype("GNUmakefile"), Some(Makefile));
    assert_eq!(get_filetype("Dockerfile"), Some(Dockerfile));
    assert_eq!(get_filetype("test.dockerfile"), Some(Dockerfile));
    assert_eq!(get_filetype("test.sh"), Some(Sh));
    assert_eq!(get_filetype("test.bash"), Some(Bash));
    assert_eq!(get_filetype("test.zsh"), Some(Zsh));
    assert_eq!(get_filetype("test.ksh"), Some(Ksh));
    assert_eq!(get_filetype("test"), None);
}
