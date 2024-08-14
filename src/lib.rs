mod constants;
mod lang;
mod parse;
mod todo;

pub use todo::Todo;

pub fn parse_file(file_path: &str) -> Vec<todo::Todo> {
    match get_extension_from_filename(file_path) {
        Some("tdz") => parse_raw(lang::tdz::extract_todos(file_path), file_path),
        Some("rs") => parse_raw(lang::rust::extract_todos(file_path), file_path),
        _ => {
            // eprintln!("[{}]: Unsupported file type", file_path);
            Vec::new()
        }
    }
}

fn get_extension_from_filename(filename: &str) -> Option<&str> {
    if filename.ends_with(".tdz") {
        return Some("tdz");
    }
    std::path::Path::new(filename)
        .extension()
        .and_then(std::ffi::OsStr::to_str)
}

#[test]
fn test_get_extension_from_filename() {
    assert_eq!(get_extension_from_filename("dir/test.tdz"), Some("tdz"));
    assert_eq!(get_extension_from_filename("test.tdz"), Some("tdz"));
    assert_eq!(get_extension_from_filename("dir/.tdz"), Some("tdz"));
    assert_eq!(get_extension_from_filename("./.tdz"), Some("tdz"));
    assert_eq!(get_extension_from_filename(".tdz"), Some("tdz"));
    assert_eq!(get_extension_from_filename("test.rs"), Some("rs"));
    assert_eq!(get_extension_from_filename("test"), None);
}

fn parse_raw(raw_todos: Vec<(u32, String)>, file_path: &str) -> Vec<todo::Todo> {
    let mut todos = Vec::<todo::Todo>::new();
    for (i, raw) in raw_todos {
        match parse::todo(&raw) {
            Ok((_, mut t)) => {
                t.file = Some(file_path.to_owned());
                t.line_number = Some(i as usize);
                todos.push(t)
            }
            Err(err) => eprintln!("Error: {}", err),
        }
    }
    todos
}
