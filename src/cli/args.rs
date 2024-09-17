use crate::cli::Command;
use todoozy::todo::filter;
use todoozy::todo::sort;

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

            Short('f') | Long("filter") => {
                args.filter = match filter::parse_str(parser.value()?.parse()?) {
                    Ok(f) => Some(f),
                    Err(e) => panic!("{}", e),
                };
            }

            Short('s') | Long("sort") => {
                args.sorter = match sort::parse_str(parser.value()?.parse()?) {
                    Ok(s) => Some(s),
                    Err(e) => panic!("{}", e),
                };
            }

            // TODO #16 (Z) 2024-09-17 Make list-projects, etc. positioned arg-like commands
            //
            // Maybe check the precedent before hand, but it feels like these might fit more
            // naturally as positioned arguments, e.g. `tdz list-projects`, which could then also
            // take their own arguments if required. +improvement
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
