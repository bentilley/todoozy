use ignore::{Walk, WalkBuilder};

pub fn get_files(to_exclude: &[String]) -> Walk {
    let mut exclude = ignore::overrides::OverrideBuilder::new("./");
    exclude.add("!.git/").unwrap();

    for e in to_exclude {
        exclude.add(&format!("!{}", e)).unwrap();
    }

    WalkBuilder::new("./")
        .hidden(false)
        .overrides(exclude.build().unwrap())
        .build()
}

#[derive(Debug, PartialEq)]
pub enum FileType {
    Go,
    Python,
    Rust,
    Todoozy,
    Terraform,
    YAML,
    Dockerfile,
    Makefile,
    Markdown,
    Protobuf,
}

pub fn get_filetype(filename: &str) -> Option<FileType> {
    use FileType::*;

    let path = std::path::Path::new(filename);

    let ft = match path.extension().and_then(std::ffi::OsStr::to_str) {
        Some("dockerfile") => Some(Dockerfile),
        Some("go") => Some(Go),
        Some("mk") => Some(Makefile),
        Some("md") => Some(Markdown),
        Some("proto") => Some(Protobuf),
        Some("py") => Some(Python),
        Some("rs") => Some(Rust),
        Some("tf") => Some(Terraform),
        Some("tdz") => Some(Todoozy),
        Some("yaml") | Some("yml") => Some(YAML),
        _ => None,
    };

    if ft.is_some() {
        return ft;
    }

    match path.file_name().and_then(std::ffi::OsStr::to_str) {
        Some(".tdz") => Some(Todoozy),
        Some("Dockerfile") => Some(Dockerfile),
        Some("Makefile") | Some("makefile") | Some("GNUmakefile") => Some(Makefile),
        _ => None,
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
    assert_eq!(get_filetype("test"), None);
}
