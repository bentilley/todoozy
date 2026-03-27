mod edit;
mod get;
mod import;
mod list;
mod remove;

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

pub fn parse_cmd(mut parser: lexopt::Parser) -> Result<TodoCommand, lexopt::Error> {
    use lexopt::prelude::*;

    match parser.next()? {
        Some(Value(val)) if val == "list" => Ok(TodoCommand::List(list::parse_opts(parser)?)),
        Some(Value(val)) if val == "get" => Ok(TodoCommand::Get(get::parse_opts(parser)?)),
        Some(Value(val)) if val == "import" => {
            Ok(TodoCommand::Import(import::parse_opts(parser)?))
        }
        Some(Value(val)) if val == "edit" => Ok(TodoCommand::Edit(edit::parse_opts(parser)?)),
        Some(Value(val)) if val == "remove" => {
            Ok(TodoCommand::Remove(remove::parse_opts(parser)?))
        }
        Some(Value(other)) => Err(lexopt::Error::Custom(
            format!("unknown todo action '{}'", other.to_string_lossy()).into(),
        )),
        _ => Err(lexopt::Error::Custom(
            "missing todo action (e.g., 'list')".into(),
        )),
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

