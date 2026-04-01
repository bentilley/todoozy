use criterion::{black_box, criterion_group, criterion_main, Criterion};
use todoozy::todo::syntax::todo;

fn bench_todo_parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("todo_parser");

    // Small TODO: just title
    let small = "Fix the bug in the parser";
    group.bench_function("small_title_only", |b| {
        b.iter(|| todo(black_box(small)))
    });

    // Medium TODO: title with projects and contexts
    let medium = "Fix the bug in the parser +bugfix @urgent";
    group.bench_function("medium_with_tags", |b| {
        b.iter(|| todo(black_box(medium)))
    });

    // Large TODO: all features - id, priority, dates, title, projects, contexts, metadata, description
    let large = r#"#123 (A) 2026-03-25 2026-03-20 Fix the critical authentication bug +security +auth @urgent @backend due:2026-04-01 assigned:alice

This is a detailed description of the bug that needs to be fixed.

The authentication flow fails when:
- User has special characters in password
- Session token expires mid-request
- Multiple concurrent login attempts

Steps to reproduce:
1. Create user with password containing '&' or '='
2. Attempt to login
3. Observe 500 error

See also: https://example.com/issue/123 +followup"#;
    group.bench_function("large_full_features", |b| {
        b.iter(|| todo(black_box(large)))
    });

    // Stress test: many projects and contexts
    let many_tags = "Refactor module +tag1 +tag2 +tag3 +tag4 +tag5 +tag6 +tag7 +tag8 @ctx1 @ctx2 @ctx3 @ctx4 @ctx5 @ctx6 @ctx7 @ctx8";
    group.bench_function("many_tags", |b| {
        b.iter(|| todo(black_box(many_tags)))
    });

    // Stress test: long description with code block
    let code_block = r##"#42 (B) 2026-03-25 Parse code blocks correctly +parser

This TODO contains a code block that should be parsed correctly:

```
fn example() {
    let x = 42;
    let y = "hello:world";  // colon in string
    println!("{}: {}", x, y);
}
```

The parser should handle the colons inside the code block without treating them as metadata."##;
    group.bench_function("with_code_block", |b| {
        b.iter(|| todo(black_box(code_block)))
    });

    group.finish();
}

criterion_group!(benches, bench_todo_parser);
criterion_main!(benches);
