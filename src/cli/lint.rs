use super::args::Mode;
use super::config;
use super::error;
use todoozy::provider::{FileSystemProvider, Provider};
use todoozy::todo::LinkingWarning;

pub const USAGE: &str = r#"Validate todo structure

Usage: tdz lint [OPTIONS]

Options:
    --fix   Auto-fix duplicate IDs by reindexing
    --help  Print help

Checks for:
    - Duplicate IDs (same #id used multiple places)
    - Orphan references (&id with no matching #id)

Exit codes:
    0  No issues found
    1  Issues found (or --fix made changes)
"#;

pub struct LintOptions {
    pub fix: bool,
}

impl Default for LintOptions {
    fn default() -> Self {
        Self { fix: false }
    }
}

pub fn parse_opts(mut parser: lexopt::Parser) -> error::Result<Mode> {
    use super::args::Command;
    use lexopt::prelude::*;

    let mut opts = LintOptions::default();

    while let Some(arg) = parser.next()? {
        match arg {
            Long("fix") => opts.fix = true,
            Long("help") => return Ok(Mode::Help(USAGE)),
            _ => return Err(arg.unexpected().into()),
        }
    }

    Ok(Mode::Cli(Command::Lint(opts)))
}

pub fn lint(conf: &config::Config, opts: &LintOptions) -> error::Result<()> {
    let todos = FileSystemProvider::new(&conf.get_todo_token(), conf.exclude.clone()).get_todos()?;
    let warnings = todos.warnings();

    if warnings.is_empty() {
        return Ok(());
    }

    if opts.fix {
        lint_fix(conf, &todos)
    } else {
        lint_report(warnings)
    }
}

fn lint_report(warnings: &[LinkingWarning]) -> error::Result<()> {
    for warning in warnings {
        eprintln!("{}", warning);
    }

    std::process::exit(1);
}

fn lint_fix(conf: &config::Config, todos: &todoozy::todo::Todos) -> error::Result<()> {
    let mut max_id = todos.get_max_id();
    let mut fixed_count = 0;
    let mut remaining_issues = 0;

    for warning in todos.warnings() {
        match warning {
            LinkingWarning::DuplicatePrimary {
                id,
                duplicate_location,
                first_location: _,
            } => {
                // Load the duplicate todo from its location
                let parser =
                    todoozy::todo::parser::TodoParser::new(&conf.get_todo_token());
                match duplicate_location.load(&parser) {
                    Ok(mut todo) => {
                        // Set the file path on the loaded todo's location
                        // (the parser doesn't preserve the file path)
                        todo.location.file_path = duplicate_location.file_path.clone();
                        todo.location.start_line_num = duplicate_location.start_line_num;
                        todo.location.end_line_num = duplicate_location.end_line_num;

                        max_id += 1;
                        let new_id = max_id;
                        match todo.rewrite_id(new_id) {
                            Ok(()) => {
                                eprintln!(
                                    "Fixed: #{} -> #{} at {}",
                                    id, new_id, duplicate_location
                                );
                                fixed_count += 1;
                            }
                            Err(e) => {
                                eprintln!(
                                    "Error fixing #{} at {}: {}",
                                    id, duplicate_location, e
                                );
                                remaining_issues += 1;
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "Error loading #{} at {}: {}",
                            id, duplicate_location, e
                        );
                        remaining_issues += 1;
                    }
                }
            }
            LinkingWarning::OrphanReference { id, location } => {
                // Orphan references cannot be auto-fixed
                eprintln!(
                    "Cannot fix: orphan reference &{} at {} (no matching #{})",
                    id, location, id
                );
                remaining_issues += 1;
            }
        }
    }

    if fixed_count > 0 {
        eprintln!("\nFixed {} duplicate ID(s)", fixed_count);
    }

    if remaining_issues > 0 {
        eprintln!("{} issue(s) remain", remaining_issues);
        std::process::exit(1);
    }

    Ok(())
}

// TODO #98 (B) 2026-04-19 Add lint tests once `std::process::exit` calls removed
//
// depends:97 Once the calls are removed then we're free to add some tests for the lint_* functions
// here.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_opts_no_args() {
        // Parser receives args after "lint" subcommand
        let parser = lexopt::Parser::from_iter(["dummy"]);
        let result = parse_opts(parser);
        if let Ok(Mode::Cli(super::super::args::Command::Lint(opts))) = result {
            assert!(!opts.fix);
        } else {
            panic!("expected Ok(Cli(Lint))");
        }
    }

    #[test]
    fn parse_opts_fix_flag() {
        let parser = lexopt::Parser::from_iter(["dummy", "--fix"]);
        let result = parse_opts(parser);
        if let Ok(Mode::Cli(super::super::args::Command::Lint(opts))) = result {
            assert!(opts.fix);
        } else {
            panic!("expected Ok(Cli(Lint))");
        }
    }

    #[test]
    fn parse_opts_help_flag() {
        let parser = lexopt::Parser::from_iter(["dummy", "--help"]);
        let result = parse_opts(parser);
        assert!(matches!(result, Ok(Mode::Help(_))));
    }
}
