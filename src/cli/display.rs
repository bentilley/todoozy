const MAX_LOCATION_WIDTH: usize = 16;

pub fn truncate_path(path: &str) -> String {
    let p = match path.strip_prefix("./") {
        Some(p) => p,
        None => path,
    };
    let mut abbrev = String::new();
    let mut length = p.len();
    let mut parts = p.split('/').peekable();

    while length > MAX_LOCATION_WIDTH {
        match parts.next() {
            Some(part) => {
                match parts.peek() {
                    Some(_) => {
                        abbrev.push(part.chars().next().unwrap());
                        abbrev.push('/');
                        length -= part.len() - 1;
                    }
                    None => {
                        abbrev.push_str(&part);
                        break;
                    }
                };
            }
            None => break,
        };
    }
    let p = parts.collect::<Vec<&str>>().join("/");
    abbrev.push_str(&p);

    if abbrev.len() > MAX_LOCATION_WIDTH {
        return format!(
            "...{}",
            abbrev[abbrev.len() - (MAX_LOCATION_WIDTH - 3)..].to_string()
        );
    }

    abbrev
}
