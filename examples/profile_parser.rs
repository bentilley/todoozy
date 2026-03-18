use std::hint::black_box;
use todoozy::{parse_text, testutils::generate_large_file, FileType};

fn main() {
    let content = generate_large_file(
        1000,
        r#"let value: i32 = 42;
let text: &str = "hello";"#,
        r#"fn example() -> i32 {
    // TODO inline style todo
    let x = compute();
    x + 1
}"#,
        r#"struct Config {
    name: String,
    value: i32,
}

impl Config {
    fn new() -> Self {
        Self { name: String::new(), value: 0 }
    }
}"#,
    );

    println!("Generated file: {} bytes", content.len());
    println!("Running 1000 iterations...");

    for _ in 0..1000 {
        let result = parse_text(black_box(&content), FileType::Rust, None);
        black_box(result);
    }

    println!("Done");
}
