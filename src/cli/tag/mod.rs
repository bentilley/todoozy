pub mod list;

use super::args::Mode;
use super::error;

pub const USAGE: &str = r#"Manage tags

Usage: tdz tag <COMMAND>

Commands:
    list    List all tags with counts

Options:
    --help  Print help
"#;

pub enum TagCommand {
    List(list::TagListOptions),
}

pub use list::list;

pub fn parse_cmd(mut parser: lexopt::Parser) -> error::Result<Mode> {
    use lexopt::prelude::*;

    match parser.next()? {
        Some(Value(val)) if val == "list" => list::parse_opts(parser),
        Some(Long("help")) => Ok(Mode::Help(USAGE)),
        Some(Value(other)) => {
            Err(format!("unknown tag action '{}'", other.to_string_lossy()).into())
        }
        _ => Err("missing tag action (e.g., 'list')".into()),
    }
}
