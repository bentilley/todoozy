pub mod error;
pub mod fs;
pub mod vcs;

pub use fs::FileSystemProvider;

use error::Result;

use crate::todo::{Todo, Todos};

pub trait Provider {
    fn get_todos(&self) -> Result<Todos>;
    fn get_todo(&self, id: u32) -> Result<Option<Todo>>;
}
