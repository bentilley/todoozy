mod list;

pub enum TagCommand {
    List(list::TagListOptions),
}

pub use list::list;

pub fn parse_cmd(mut parser: lexopt::Parser) -> Result<TagCommand, lexopt::Error> {
    use lexopt::prelude::*;

    match parser.next()? {
        Some(Value(val)) if val == "list" => Ok(TagCommand::List(list::parse_opts(parser)?)),
        Some(Value(other)) => Err(lexopt::Error::Custom(
            format!("unknown tag action '{}'", other.to_string_lossy()).into(),
        )),
        _ => Err(lexopt::Error::Custom(
            "missing tag action (e.g., 'list')".into(),
        )),
    }
}
