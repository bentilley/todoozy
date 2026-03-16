use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use todoozy::{parse_text, testutils::generate_large_file, FileType};

fn bench_typescript_parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("typescript");

    // Large files with varying TODO counts - each chunk
    for num_chunks in [10, 100, 1000] {
        let large = generate_large_file(
            num_chunks,
            r#"const someCode = "value";
const otherCode = 42;"#,
            r#"function example() {
    // TODO inline style todo
    const x = compute();
    return x + 1;
}"#,
            r#"interface Config {
    name: string;
    value: number;
}

const config: Config = {
    name: "",
    value: 0,
};

export default config;"#,
        );
        group.throughput(Throughput::Bytes(large.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_chunks", num_chunks)),
            &large,
            |b, content| b.iter(|| parse_text(black_box(content), FileType::Typescript, None)),
        );
    }

    group.finish();
}

fn bench_rust_parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("rust");

    for num_chunks in [10, 100, 1000] {
        let large = generate_large_file(
            num_chunks,
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
        group.throughput(Throughput::Bytes(large.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_chunks", num_chunks)),
            &large,
            |b, content| b.iter(|| parse_text(black_box(content), FileType::Rust, None)),
        );
    }

    group.finish();
}

criterion_group!(benches, bench_typescript_parser, bench_rust_parser);
criterion_main!(benches);
