mod list;

pub enum TodoCommand {
    List(self::list::TodoListOptions),
}

pub use self::list::{list, OutputFormat};

pub fn parse_cmd(mut parser: lexopt::Parser) -> Result<TodoCommand, lexopt::Error> {
    use lexopt::prelude::*;

    match parser.next()? {
        Some(Value(val)) if val == "list" => Ok(TodoCommand::List(list::parse_opts(parser)?)),
        Some(Value(other)) => Err(lexopt::Error::Custom(
            format!("unknown todo action '{}'", other.to_string_lossy()).into(),
        )),
        _ => Err(lexopt::Error::Custom(
            "missing todo action (e.g., 'list')".into(),
        )),
    }
}
