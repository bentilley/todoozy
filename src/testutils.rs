/// Generate a large file with multiple TODO comments for benchmarking and profiling.
///
/// Each chunk contains 4 TODO comments in various formats (line comments, block comments).
/// The `code`, `func_def`, and `more_code` parameters are interspersed between TODOs.
pub fn generate_large_file(num_chunks: usize, code: &str, func_def: &str, more_code: &str) -> String {
    let mut content = String::new();
    for _ in 0..num_chunks {
        content.push_str(&format!(
            r#"
// TODO (A) 2024-01-01 Todo with priority and date +project @context
//
// This is a multi-line description that spans several lines to test the parser's handling of
// continuation lines.
{}

/* TODO (B) Block comment todo

   With multiple lines of description
   and some indentation to parse.
 */
{}

// TODO another line comment
// with continuation
{}

/* TODO final block comment */
"#,
            code, func_def, more_code
        ));
    }
    content
}
