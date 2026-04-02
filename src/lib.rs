pub mod error;
mod fs;
mod lang;
pub mod provider;
pub mod todo;

#[cfg(feature = "testutils")]
pub mod testutils;

pub use fs::{FileType, FileTypeAwarePath};
pub use todo::{parser::TodoParser, Todo, TodoIdentifier, Todos};
