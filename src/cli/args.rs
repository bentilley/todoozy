use super::error;
use super::lint;
use super::tag;
use super::todo;
use todoozy::todo::filter;
use todoozy::todo::sort;

//TODO #77 (E) 2026-03-31 Should some of the cli commands take file paths
//
// I feel like it would be good to be able to pass specific files to tdz commands, and possibly
// even the TUI - i.e. `tdz file.rs` or `tdz dir` would start the TUI showing only todos in those
// files / directories. Def useful for testing, or seeing how a todo you just wrote looks. Also,
// useful if you want to just see todos for a specific package in a monorepo. Needs some scoping
// first +idea.

// TODO #57 (D) 2026-03-22 Implement `tdz summary` command +cli
//
// Show summary statistics for the codebase:
// - total todo count
// - breakdown by priority
// - breakdown by tag
// - maybe: tracked vs untracked count

// TODO #66 (D) 2026-03-22 Implement `tdz lint` command +cli +ids
//
// Validation command for CI/hooks. Checks for:
// - Duplicate IDs (same #id used in multiple places)
// - Orphan references (&id with no matching #id primary)
// - Other structural issues as needed
//
// Usage:
//   tdz lint              # report issues, exit 1 if any found
//   tdz lint --fix        # auto-fix duplicates by reindexing
//
// The --fix flag reassigns duplicate IDs to next available:
// - Keeps first occurrence's ID
// - Reassigns subsequent occurrences
// - Updates files in place
// - Reports what changed
//
// Designed for CI integration - non-zero exit code on errors.

// TODO #67 (D) 2026-03-22 Implement `tdz cache build` command +cli +ids
//
// Build cache of all TODO IDs ever used in git history.
//
// Usage:
//   tdz cache build       # crawl git history, cache used IDs
//   tdz cache clear       # clear the cache
//
// The cache is used by `tdz todo import` to determine next available ID:
//   next_id = max(all_ids_ever_used) + 1
//
// Cache is stored in local state (not git) keyed by commit SHA, so:
// - Incremental: only scans new commits
// - Accurate: knows IDs from all branches/history
// - Fast: cached results reused
//
// See also: TODO for moving _num_todos to local state in config.rs

// TODO #63 (E) 2026-03-22 Implement `tdz file convert` command +cli +tdz
//
// Convert a .tdz file into a source file with TODOs as comments.
//
// Usage: tdz file convert thing.py.tdz
//
// This would:
// 1. Parse TODOs from thing.py.tdz
// 2. Determine target language from filename (thing.py → Python)
// 3. Generate thing.py with each TODO formatted as language-appropriate
//    comments (using the language's syntax rules)
// 4. User can then edit the generated file to add implementation
//
// Useful for scaffolding new files from TODO specifications.

pub enum Command {
    Lint(lint::LintOptions),
    Tag(tag::TagCommand),
    Todo(todo::TodoCommand),
}

pub enum Mode {
    Cli(Command),
    Help(&'static str),
    TUI(TuiOptions),
}

const USAGE: &str = r#"Todos as code manager

Usage: tdz [OPTIONS] [COMMAND]

Commands:
    lint    Validate todo structure
    todo    Manage todos (list, get, import, edit, remove)
    tag     Manage tags

Options:
    -E, --exclude <PATH<,PATH>>  Files or directories to exclude from search
    -f, --filter <FILTER>        Filter which todos to display
    -s, --sort <SORT>            How to sort the todos
    --help                       Print help
"#;

pub fn parse_args(mut parser: lexopt::Parser) -> error::Result<Mode> {
    match detect_subcommand(&mut parser) {
        Some(cmd) if cmd == "lint" => lint::parse_opts(parser),
        Some(cmd) if cmd == "tag" => tag::parse_cmd(parser),
        Some(cmd) if cmd == "todo" => todo::parse_cmd(parser),
        Some(other) => Err(format!("unknown subcommand '{}'", other).into()),
        None => parse_tui_args(parser),
    }
}

/// Peeks at the first argument to see if it's a subcommand (positional, not a flag).
/// If it is, consumes it and returns the subcommand name.
/// If it's a flag or there are no args, returns None without consuming anything.
fn detect_subcommand(parser: &mut lexopt::Parser) -> Option<String> {
    let mut raw = parser.try_raw_args()?;
    let arg = raw.peek()?;

    if arg.to_string_lossy().starts_with('-') {
        return None;
    }

    let cmd = arg.to_string_lossy().into_owned();
    raw.next(); // consume the argument
    Some(cmd)
}

/// Represents a command-line flag that can override a config file value.
pub enum Override<T> {
    /// Flag was not passed; leave config value unchanged.
    NotSet,
    /// Flag was passed with empty string; explicitly unset the config value.
    Unset,
    /// Flag was passed with a value; set the config to this value.
    Value(T),
}

impl<T> Default for Override<T> {
    fn default() -> Self {
        Override::NotSet
    }
}

pub struct TuiOptions {
    pub exclude: Vec<String>,
    pub filter: Override<Box<dyn filter::Filter>>,
    pub sorter: Override<Box<dyn sort::Sorter>>,
}

impl TuiOptions {
    pub fn new() -> TuiOptions {
        TuiOptions {
            exclude: Vec::new(),
            filter: Override::NotSet,
            sorter: Override::NotSet,
        }
    }

    pub fn apply(&mut self, config: &mut crate::cli::config::Config) {
        config.exclude.append(&mut self.exclude.clone());
        match std::mem::take(&mut self.filter) {
            Override::NotSet => {}
            Override::Unset => config.filter = None,
            Override::Value(f) => config.filter = Some(f),
        }
        match std::mem::take(&mut self.sorter) {
            Override::NotSet => {}
            Override::Unset => config.sorter = None,
            Override::Value(s) => config.sorter = Some(s),
        }
    }
}

fn parse_tui_args(mut parser: lexopt::Parser) -> error::Result<Mode> {
    use lexopt::prelude::*;
    let mut args = TuiOptions::new();
    while let Some(arg) = parser.next()? {
        match arg {
            // TODO #7 (Z) 2024-08-05 Implement a .tdzignore file +idea
            //
            // This would allow users to specify a list of directories or files to exclude without
            // having to pass them as arguments in every tdz call.
            //
            // Unsure if we need exclude atm, now that the todo comment parsing logic is tighter.
            // Needs more data from use in the field!
            Short('E') | Long("exclude") => {
                let e: String = parser.value()?.parse()?;
                args.exclude
                    .append(&mut e.split(',').map(String::from).collect());
            }
            Short('f') | Long("filter") => {
                let value: String = parser.value()?.parse()?;
                if value.is_empty() {
                    args.filter = Override::Unset;
                } else {
                    args.filter = match value.parse() {
                        Ok(f) => Override::Value(f),
                        Err(e) => return Err(format!("invalid filter '{}': {}", value, e).into()),
                    };
                }
            }
            Short('s') | Long("sort") => {
                let value: String = parser.value()?.parse()?;
                if value.is_empty() {
                    args.sorter = Override::Unset;
                } else {
                    args.sorter = match value.parse() {
                        Ok(s) => Override::Value(s),
                        Err(e) => return Err(format!("invalid sort '{}': {}", value, e).into()),
                    };
                }
            }
            Long("help") => return Ok(Mode::Help(USAGE)),
            _ => return Err(arg.unexpected().into()),
        }
    }
    Ok(Mode::TUI(args))
}

#[cfg(test)]
mod tests {
    use super::tag::TagCommand;
    use super::todo::{OutputFormat, TodoCommand};
    use super::*;

    #[test]
    fn no_args_returns_tui_mode() {
        let result = parse_args(lexopt::Parser::from_iter(["tdz"]));
        assert!(matches!(result, Ok(Mode::TUI(_))));
    }

    #[test]
    fn exclude_single_path() {
        let result = parse_args(lexopt::Parser::from_iter(["tdz", "-E", "foo"]));
        if let Ok(Mode::TUI(args)) = result {
            assert_eq!(args.exclude, vec!["foo"]);
        } else {
            panic!("expected Ok(TUI)");
        }
    }

    #[test]
    fn exclude_multiple_paths_comma_separated() {
        let result = parse_args(lexopt::Parser::from_iter([
            "tdz",
            "--exclude",
            "foo,bar,baz",
        ]));
        if let Ok(Mode::TUI(args)) = result {
            assert_eq!(args.exclude, vec!["foo", "bar", "baz"]);
        } else {
            panic!("expected Ok(TUI)");
        }
    }

    #[test]
    fn filter_valid_value() {
        let result = parse_args(lexopt::Parser::from_iter(["tdz", "-f", "priority=A"]));
        if let Ok(Mode::TUI(args)) = result {
            assert!(matches!(args.filter, Override::Value(_)));
        } else {
            panic!("expected Ok(TUI)");
        }
    }

    #[test]
    fn filter_empty_string_sets_unset() {
        let result = parse_args(lexopt::Parser::from_iter(["tdz", "--filter", ""]));
        if let Ok(Mode::TUI(args)) = result {
            assert!(matches!(args.filter, Override::Unset));
        } else {
            panic!("expected Ok(TUI)");
        }
    }

    #[test]
    fn sort_valid_value() {
        let result = parse_args(lexopt::Parser::from_iter(["tdz", "-s", "priority:asc"]));
        if let Ok(Mode::TUI(args)) = result {
            assert!(matches!(args.sorter, Override::Value(_)));
        } else {
            panic!("expected Ok(TUI)");
        }
    }

    #[test]
    fn sort_empty_string_sets_unset() {
        let result = parse_args(lexopt::Parser::from_iter(["tdz", "--sort", ""]));
        if let Ok(Mode::TUI(args)) = result {
            assert!(matches!(args.sorter, Override::Unset));
        } else {
            panic!("expected Ok(TUI)");
        }
    }

    #[test]
    fn unknown_flag_returns_error() {
        let result = parse_args(lexopt::Parser::from_iter(["tdz", "--unknown"]));
        assert!(result.is_err());
    }

    #[test]
    fn todo_list_returns_cli_mode() {
        let result = parse_args(lexopt::Parser::from_iter(["tdz", "todo", "list"]));
        assert!(matches!(
            result,
            Ok(Mode::Cli(Command::Todo(
                TodoCommand::List(_)
            )))
        ));
    }

    #[test]
    fn todo_list_limit_long_flag() {
        let result = parse_args(lexopt::Parser::from_iter([
            "tdz", "todo", "list", "--limit", "10",
        ]));
        if let Ok(Mode::Cli(Command::Todo(TodoCommand::List(opts)))) = result {
            assert_eq!(opts.limit, Some(10));
        } else {
            panic!("expected Ok(Cli(Todo(List)))");
        }
    }

    #[test]
    fn todo_list_limit_short_flag() {
        let result = parse_args(lexopt::Parser::from_iter([
            "tdz", "todo", "list", "-n", "5",
        ]));
        if let Ok(Mode::Cli(Command::Todo(TodoCommand::List(opts)))) = result {
            assert_eq!(opts.limit, Some(5));
        } else {
            panic!("expected Ok(Cli(Todo(List)))");
        }
    }

    #[test]
    fn todo_list_format_json() {
        let result = parse_args(lexopt::Parser::from_iter([
            "tdz", "todo", "list", "--format", "json",
        ]));
        if let Ok(Mode::Cli(Command::Todo(TodoCommand::List(opts)))) = result {
            assert_eq!(opts.format, OutputFormat::Json);
        } else {
            panic!("expected Ok(Cli(Todo(List)))");
        }
    }

    #[test]
    fn todo_list_format_raw() {
        let result = parse_args(lexopt::Parser::from_iter([
            "tdz", "todo", "list", "--format", "raw",
        ]));
        if let Ok(Mode::Cli(Command::Todo(TodoCommand::List(opts)))) = result {
            assert_eq!(opts.format, OutputFormat::Raw);
        } else {
            panic!("expected Ok(Cli(Todo(List)))");
        }
    }

    #[test]
    fn todo_list_limit_and_format() {
        let result = parse_args(lexopt::Parser::from_iter([
            "tdz", "todo", "list", "--limit", "5", "--format", "raw",
        ]));
        if let Ok(Mode::Cli(Command::Todo(TodoCommand::List(opts)))) = result {
            assert_eq!(opts.limit, Some(5));
            assert_eq!(opts.format, OutputFormat::Raw);
        } else {
            panic!("expected Ok(Cli(Todo(List)))");
        }
    }

    #[test]
    fn todo_list_filter_long_flag() {
        let result = parse_args(lexopt::Parser::from_iter([
            "tdz",
            "todo",
            "list",
            "--filter",
            "priority=A",
        ]));
        if let Ok(Mode::Cli(Command::Todo(TodoCommand::List(opts)))) = result {
            assert!(opts.filter.is_some());
        } else {
            panic!("expected Ok(Cli(Todo(List)))");
        }
    }

    #[test]
    fn todo_list_filter_short_flag() {
        let result = parse_args(lexopt::Parser::from_iter([
            "tdz",
            "todo",
            "list",
            "-f",
            "priority=A",
        ]));
        if let Ok(Mode::Cli(Command::Todo(TodoCommand::List(opts)))) = result {
            assert!(opts.filter.is_some());
        } else {
            panic!("expected Ok(Cli(Todo(List)))");
        }
    }

    #[test]
    fn todo_list_sort_long_flag() {
        let result = parse_args(lexopt::Parser::from_iter([
            "tdz",
            "todo",
            "list",
            "--sort",
            "priority:asc",
        ]));
        if let Ok(Mode::Cli(Command::Todo(TodoCommand::List(opts)))) = result {
            assert!(opts.sorter.is_some());
        } else {
            panic!("expected Ok(Cli(Todo(List)))");
        }
    }

    #[test]
    fn todo_list_sort_short_flag() {
        let result = parse_args(lexopt::Parser::from_iter([
            "tdz",
            "todo",
            "list",
            "-s",
            "priority:asc",
        ]));
        if let Ok(Mode::Cli(Command::Todo(TodoCommand::List(opts)))) = result {
            assert!(opts.sorter.is_some());
        } else {
            panic!("expected Ok(Cli(Todo(List)))");
        }
    }

    #[test]
    fn todo_list_filter_and_sort() {
        let result = parse_args(lexopt::Parser::from_iter([
            "tdz",
            "todo",
            "list",
            "--filter",
            "priority=A",
            "--sort",
            "priority:asc",
        ]));
        if let Ok(Mode::Cli(Command::Todo(TodoCommand::List(opts)))) = result {
            assert!(opts.filter.is_some());
            assert!(opts.sorter.is_some());
        } else {
            panic!("expected Ok(Cli(Todo(List)))");
        }
    }

    #[test]
    fn todo_list_all_flags() {
        let result = parse_args(lexopt::Parser::from_iter([
            "tdz",
            "todo",
            "list",
            "--limit",
            "5",
            "--filter",
            "priority=A",
            "--format",
            "json",
        ]));
        if let Ok(Mode::Cli(Command::Todo(TodoCommand::List(opts)))) = result {
            assert_eq!(opts.limit, Some(5));
            assert!(opts.filter.is_some());
            assert_eq!(opts.format, OutputFormat::Json);
        } else {
            panic!("expected Ok(Cli(Todo(List)))");
        }
    }

    #[test]
    fn todo_get_basic() {
        let result = parse_args(lexopt::Parser::from_iter(["tdz", "todo", "get", "54"]));
        if let Ok(Mode::Cli(Command::Todo(TodoCommand::Get(opts)))) = result {
            assert_eq!(opts.id, 54);
            assert_eq!(opts.format, OutputFormat::Raw);
        } else {
            panic!("expected Ok(Cli(Todo(Get)))");
        }
    }

    #[test]
    fn todo_get_with_format_json() {
        let result = parse_args(lexopt::Parser::from_iter([
            "tdz", "todo", "get", "54", "--format", "json",
        ]));
        if let Ok(Mode::Cli(Command::Todo(TodoCommand::Get(opts)))) = result {
            assert_eq!(opts.id, 54);
            assert_eq!(opts.format, OutputFormat::Json);
        } else {
            panic!("expected Ok(Cli(Todo(Get)))");
        }
    }

    #[test]
    fn todo_get_with_format_raw() {
        let result = parse_args(lexopt::Parser::from_iter([
            "tdz", "todo", "get", "42", "--format", "raw",
        ]));
        if let Ok(Mode::Cli(Command::Todo(TodoCommand::Get(opts)))) = result {
            assert_eq!(opts.id, 42);
            assert_eq!(opts.format, OutputFormat::Raw);
        } else {
            panic!("expected Ok(Cli(Todo(Get)))");
        }
    }

    #[test]
    fn todo_get_format_before_id() {
        let result = parse_args(lexopt::Parser::from_iter([
            "tdz", "todo", "get", "--format", "json", "54",
        ]));
        if let Ok(Mode::Cli(Command::Todo(TodoCommand::Get(opts)))) = result {
            assert_eq!(opts.id, 54);
            assert_eq!(opts.format, OutputFormat::Json);
        } else {
            panic!("expected Ok(Cli(Todo(Get)))");
        }
    }

    #[test]
    fn todo_get_missing_id_returns_error() {
        let result = parse_args(lexopt::Parser::from_iter(["tdz", "todo", "get"]));
        assert!(result.is_err());
    }

    #[test]
    fn todo_get_invalid_id_returns_error() {
        let result = parse_args(lexopt::Parser::from_iter(["tdz", "todo", "get", "abc"]));
        assert!(result.is_err());
    }

    #[test]
    fn tag_list_returns_cli_mode() {
        let result = parse_args(lexopt::Parser::from_iter(["tdz", "tag", "list"]));
        assert!(matches!(
            result,
            Ok(Mode::Cli(Command::Tag(TagCommand::List(
                _
            ))))
        ));
    }

    #[test]
    fn tag_list_limit_long_flag() {
        let result = parse_args(lexopt::Parser::from_iter([
            "tdz", "tag", "list", "--limit", "10",
        ]));
        if let Ok(Mode::Cli(Command::Tag(TagCommand::List(opts)))) = result {
            assert_eq!(opts.limit, Some(10));
        } else {
            panic!("expected Ok(Cli(Tag(List)))");
        }
    }

    #[test]
    fn tag_list_limit_short_flag() {
        let result = parse_args(lexopt::Parser::from_iter(["tdz", "tag", "list", "-n", "5"]));
        if let Ok(Mode::Cli(Command::Tag(TagCommand::List(opts)))) = result {
            assert_eq!(opts.limit, Some(5));
        } else {
            panic!("expected Ok(Cli(Tag(List)))");
        }
    }

    #[test]
    fn tag_list_format_json() {
        let result = parse_args(lexopt::Parser::from_iter([
            "tdz", "tag", "list", "--format", "json",
        ]));
        if let Ok(Mode::Cli(Command::Tag(TagCommand::List(opts)))) = result {
            assert_eq!(format!("{:?}", opts.format), "Json");
        } else {
            panic!("expected Ok(Cli(Tag(List)))");
        }
    }

    #[test]
    fn tag_list_format_raw() {
        let result = parse_args(lexopt::Parser::from_iter([
            "tdz", "tag", "list", "--format", "raw",
        ]));
        if let Ok(Mode::Cli(Command::Tag(TagCommand::List(opts)))) = result {
            assert_eq!(format!("{:?}", opts.format), "Raw");
        } else {
            panic!("expected Ok(Cli(Tag(List)))");
        }
    }

    #[test]
    fn tag_list_sort_name() {
        let result = parse_args(lexopt::Parser::from_iter([
            "tdz", "tag", "list", "--sort", "name",
        ]));
        if let Ok(Mode::Cli(Command::Tag(TagCommand::List(opts)))) = result {
            assert_eq!(format!("{:?}", opts.sort), "Name");
        } else {
            panic!("expected Ok(Cli(Tag(List)))");
        }
    }

    #[test]
    fn tag_list_sort_count() {
        let result = parse_args(lexopt::Parser::from_iter([
            "tdz", "tag", "list", "--sort", "count",
        ]));
        if let Ok(Mode::Cli(Command::Tag(TagCommand::List(opts)))) = result {
            assert_eq!(format!("{:?}", opts.sort), "Count");
        } else {
            panic!("expected Ok(Cli(Tag(List)))");
        }
    }

    #[test]
    fn tag_list_all_flags() {
        let result = parse_args(lexopt::Parser::from_iter([
            "tdz", "tag", "list", "--limit", "5", "--format", "json", "--sort", "count",
        ]));
        if let Ok(Mode::Cli(Command::Tag(TagCommand::List(opts)))) = result {
            assert_eq!(opts.limit, Some(5));
            assert_eq!(format!("{:?}", opts.format), "Json");
            assert_eq!(format!("{:?}", opts.sort), "Count");
        } else {
            panic!("expected Ok(Cli(Tag(List)))");
        }
    }
}
