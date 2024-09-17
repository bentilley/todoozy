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

pub fn num_digits(n: u32) -> u32 {
    if n == 0 {
        return 1;
    }
    ((n as f64).log10() + 1.0).floor() as u32
}

#[test]
fn test_num_digits() {
    assert_eq!(num_digits(0), 1);
    assert_eq!(num_digits(1), 1);
    assert_eq!(num_digits(9), 1);
    assert_eq!(num_digits(10), 2);
    assert_eq!(num_digits(99), 2);
    assert_eq!(num_digits(100), 3);
}
