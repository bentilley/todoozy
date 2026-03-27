pub mod args;
pub mod config;
pub mod tag;
pub mod todo;
pub mod tui;

use self::tag::TagCommand;
use self::todo::TodoCommand;

pub enum Command {
    Tag(TagCommand),
    Todo(TodoCommand),
}
