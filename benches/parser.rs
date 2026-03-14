use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use todoozy::{parse_text, FileType};

// Sample files of varying complexity
const SMALL_TS: &str = r#"
// TODO first todo
const x = 1;
// TODO second todo
const y = 2;
"#;

const MEDIUM_TS: &str = r#"
// TODO (A) 2024-01-01 First todo with priority and date +project @context
//
// This is a multi-line description that spans
// several lines to test the parser's handling
// of continuation lines.

const someCode = "value";

/* TODO (B) Block comment todo

   With multiple lines of description
   and some indentation to parse.
 */

function example() {
    // TODO inline style todo
    return 42;
}

// TODO another line comment
// with continuation
const more = `template string`;

/* TODO final block comment */
"#;

fn generate_large_file(num_todos: usize) -> String {
    let mut content = String::new();
    for i in 0..num_todos {
        content.push_str(&format!(
            r#"
// TODO ({}) Task number {}
// Description for task {}.
const var{} = {};

"#,
            if i % 3 == 0 {
                "A"
            } else if i % 3 == 1 {
                "B"
            } else {
                "C"
            },
            i,
            i,
            i,
            i
        ));
    }
    content
}

fn bench_typescript_parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("typescript");

    // Small file
    group.throughput(Throughput::Bytes(SMALL_TS.len() as u64));
    group.bench_function("small", |b| {
        b.iter(|| parse_text(black_box(SMALL_TS), FileType::Typescript, None))
    });

    // Medium file
    group.throughput(Throughput::Bytes(MEDIUM_TS.len() as u64));
    group.bench_function("medium", |b| {
        b.iter(|| parse_text(black_box(MEDIUM_TS), FileType::Typescript, None))
    });

    // Large files with varying TODO counts
    for num_todos in [10, 50, 100, 500] {
        let large = generate_large_file(num_todos);
        group.throughput(Throughput::Bytes(large.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("large", format!("{}_todos", num_todos)),
            &large,
            |b, content| b.iter(|| parse_text(black_box(content), FileType::Typescript, None)),
        );
    }

    group.finish();
}

fn bench_rust_parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("rust");

    // Use the actual lang.rs file as a real-world benchmark
    let lang_rs = include_str!("../src/lang.rs");
    group.throughput(Throughput::Bytes(lang_rs.len() as u64));
    group.bench_function("lang.rs", |b| {
        b.iter(|| parse_text(black_box(lang_rs), FileType::Rust, None))
    });

    group.finish();
}

fn bench_python_parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("python");

    let sample = r#"
# TODO (A) Python todo with triple quotes
#
# Description here.

text = """
# TODO fake todo in string
# should be ignored
"""

# TODO real todo after string
def example():
    pass
"#;

    group.throughput(Throughput::Bytes(sample.len() as u64));
    group.bench_function("with_raw_strings", |b| {
        b.iter(|| parse_text(black_box(sample), FileType::Python, None))
    });

    group.finish();
}

fn bench_go_parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("go");

    let sample = r#"
// TODO (A) Go todo
//
// Description.

package main

/* TODO block comment in Go
   More description.
 */

func main() {
    raw := `
    // TODO fake todo in raw string
    `
}

// TODO final todo
"#;

    group.throughput(Throughput::Bytes(sample.len() as u64));
    group.bench_function("mixed_comments", |b| {
        b.iter(|| parse_text(black_box(sample), FileType::Go, None))
    });

    group.finish();
}

fn bench_no_todos(c: &mut Criterion) {
    let mut group = c.benchmark_group("no_todos");

    // File with no TODOs - tests early exit / scanning performance
    let no_todos = r#"
const x = 1;
const y = 2;
function example() {
    return x + y;
}
// Just a regular comment
/* Another comment */
const z = 3;
"#;

    group.throughput(Throughput::Bytes(no_todos.len() as u64));
    group.bench_function("typescript", |b| {
        b.iter(|| parse_text(black_box(no_todos), FileType::Typescript, None))
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_typescript_parser,
    bench_rust_parser,
    bench_python_parser,
    bench_go_parser,
    bench_no_todos,
);
criterion_main!(benches);
