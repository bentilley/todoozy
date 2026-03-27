pub mod edit;
pub mod get;
pub mod import;
pub mod list;
pub mod remove;

use super::args::Mode;
use super::error;

pub const USAGE: &str = r#"Manage todos

Usage: tdz todo <COMMAND>

Commands:
    list      List todos in compact table format
    get       Show full details for a specific todo
    import    Import untracked todos (assign IDs)
    edit      Open todo in $EDITOR at its file location
    remove    Delete a todo comment from its source file

Options:
    --help    Print help
"#;

pub enum TodoCommand {
    List(list::TodoListOptions),
    Get(get::TodoGetOptions),
    Import(import::TodoImportOptions),
    Edit(edit::TodoEditOptions),
    Remove(remove::TodoRemoveOptions),
}

pub use edit::edit;
pub use get::get;
pub use import::import;
pub use list::list;
pub use remove::remove;

pub fn parse_cmd(mut parser: lexopt::Parser) -> error::Result<Mode> {
    use lexopt::prelude::*;

    match parser.next()? {
        Some(Value(val)) if val == "list" => list::parse_opts(parser),
        Some(Value(val)) if val == "get" => get::parse_opts(parser),
        Some(Value(val)) if val == "import" => import::parse_opts(parser),
        Some(Value(val)) if val == "edit" => edit::parse_opts(parser),
        Some(Value(val)) if val == "remove" => remove::parse_opts(parser),
        Some(Long("help")) => Ok(Mode::Help(USAGE)),
        Some(Value(other)) => {
            Err(format!("unknown todo action '{}'", other.to_string_lossy()).into())
        }
        _ => Err("missing todo action (e.g., 'list')".into()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Table,
    Json,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "table" => Ok(OutputFormat::Table),
            "json" => Ok(OutputFormat::Json),
            other => Err(format!(
                "unknown format '{}', expected 'table' or 'json'",
                other
            )),
        }
    }
}
