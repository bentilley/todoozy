use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use todoozy::{testutils::generate_large_file, FileType, TodoParser};

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
            |b, content| {
                let parser = TodoParser::new("TODO");
                b.iter(|| parser.parse_text(black_box(content), FileType::Typescript))
            },
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
            |b, content| {
                let parser = TodoParser::new("TODO");
                b.iter(|| parser.parse_text(black_box(content), FileType::Rust))
            },
        );
    }

    group.finish();
}

fn bench_sh_parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("sh");

    let chunk = r##"VALUE="some value"
OTHER='literal'
export PATH="$HOME/bin:$PATH"

# TODO load configuration from environment
#
# Should read CONFIG_PATH and DATA_DIR from env vars
# and fall back to sensible defaults if not set.
process_data() {
    # TODO inline style todo
    local result=$(compute)
    echo "$result"
}

# Config section
msg="# TODO this is inside double quotes"
name='# TODO this is inside single quotes'

cat <<EOF
# TODO this is inside a here-doc
Some content here
EOF

configure() {
    local config_file="$1"
    # TODO add validation for config file format
    #
    # Check that the file contains valid key=value pairs
    # and warn on any unrecognized keys.
    if [ -f "$config_file" ]; then
        source "$config_file"
    fi
}
"##;

    for num_chunks in [10, 100, 1000] {
        let large: String = std::iter::repeat(chunk).take(num_chunks).collect();
        group.throughput(Throughput::Bytes(large.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_chunks", num_chunks)),
            &large,
            |b, content| {
                let parser = TodoParser::new("TODO");
                b.iter(|| parser.parse_text(black_box(content), FileType::Sh))
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_typescript_parser,
    bench_rust_parser,
    bench_sh_parser
);
criterion_main!(benches);
