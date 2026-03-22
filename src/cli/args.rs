use crate::cli::Command;
use todoozy::todo::filter;
use todoozy::todo::sort;

// TODO #54 (D) 2026-03-22 Implement `tdz todo` subcommands +cli
//
// Add subcommand support for todo operations:
//
// - `tdz todo list` - list todos in compact table format
//   - supports `--limit <n>` to cap number of results
//   - supports `-f/--filter` and `-s/--sort` (existing logic)
//   - supports `--format <table|json>` (default: table)
//   - table columns: ID, PRI, LOCATION (file:line), TITLE, PROJECTS
//
// - `tdz todo get <id>` - show full details for a specific todo
//   - all metadata: id, priority, dates, projects, contexts, key:values
//   - full description text
//   - file location
//
// - `tdz todo import <id>` - import a specific untracked todo (assign ID)
// - `tdz todo import-all` - import all untracked todos
// - `tdz todo edit <id>` - open $EDITOR at todo's file:line
// - `tdz todo remove <id>` - delete the TODO comment from source file
//
// This replaces --import-all flag (breaking change).
// Default `tdz` (no subcommand) still launches TUI.

// TODO #55 (D) 2026-03-22 Implement `tdz project` subcommands +cli
//
// - `tdz project list` - list all +project tags found in todos
//
// This replaces --list-projects flag (breaking change).

// TODO #56 (D) 2026-03-22 Implement `tdz context` subcommands +cli
//
// - `tdz context list` - list all `@context` tags found in todos
//
// This replaces --list-contexts flag (breaking change).

// TODO #57 (D) 2026-03-22 Implement `tdz summary` command +cli
//
// Show summary statistics for the codebase:
// - total todo count
// - breakdown by priority
// - breakdown by project
// - breakdown by context
// - maybe: tracked vs untracked count

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

pub enum Mode {
    Cli(Command),
    TUI(Args),
}

pub struct Args {
    pub exclude: Vec<String>,
    pub filter: Option<Box<dyn filter::Filter>>,
    pub sorter: Option<Box<dyn sort::Sorter>>,
}

impl Args {
    pub fn new() -> Args {
        Args {
            exclude: Vec::new(),
            filter: None,
            sorter: None,
        }
    }

    pub fn apply(&mut self, config: &mut crate::cli::config::Config) {
        config.exclude.append(&mut self.exclude.clone());
        if let Some(f) = self.filter.take() {
            config.filter = Some(f);
        }
        if let Some(s) = self.sorter.take() {
            config.sorter = Some(s);
        }
    }
}

const USAGE: &str = r#"Todos as code manager

Usage: tdz [OPTIONS]

Options:
    -E, --exclude <PATH<,PATH>>  Files or directories to exclude from search
    -f, --filter <FILTER>        Filter which todos to display
    -s, --sort <SORT>            How to sort the todos
    --list-projects              List all projects
    --list-contexts              List all contexts
    --import-all                 Import all todos
    --help                       Print help
    "#;

pub fn parse_args() -> Result<Mode, lexopt::Error> {
    use lexopt::prelude::*;

    let mut args = Args::new();
    let mut parser = lexopt::Parser::from_env();

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

            // TODO #52 (A) 2026-03-22 A way to unset the filter via the command line
            //
            // Currently, if you set a filter in the json config file, then want to unset it I'm
            // not sure there's a way. Passing `--filter ""` results in this panic (which should be
            // changed to a more descriptive error message).
            Short('f') | Long("filter") => {
                args.filter = match filter::parse_str(parser.value()?.parse()?) {
                    Ok(f) => Some(f),
                    Err(e) => panic!("{}", e),
                };
            }

            // TODO #53 (A) 2026-03-22 A way to unset the sort via the command line
            //
            // Currently, if you set a sort in the json config file, then want to unset it I'm
            // not sure there's a way. Passing `--sort ""` results in this panic (which should be
            // changed to a more descriptive error message).
            Short('s') | Long("sort") => {
                args.sorter = match sort::parse_str(parser.value()?.parse()?) {
                    Ok(s) => Some(s),
                    Err(e) => panic!("{}", e),
                };
            }

            Long("list-projects") => return Ok(Mode::Cli(Command::ListProjects)),
            Long("list-contexts") => return Ok(Mode::Cli(Command::ListContexts)),
            Long("import-all") => return Ok(Mode::Cli(Command::ImportAll)),

            Long("help") => {
                println!("{}", USAGE);
                std::process::exit(0);
            }
            _ => return Err(arg.unexpected()),
        }
    }

    Ok(Mode::TUI(args))
}
