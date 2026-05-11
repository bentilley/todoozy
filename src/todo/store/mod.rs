mod error;
mod local;

use super::Todo;
use error::Result;

pub use local::SqliteStore;

pub struct Query {}

pub trait Store {
    fn get_todo(&self, id: u32) -> Option<Todo>;
    fn set_todo(&self, todo: &Todo) -> Result<u32>;
    fn update_todo(&self, id: u32, todo: Todo) -> Result<()>;

    fn get_todos(&self) -> Vec<Todo>;
    fn query_todos(&self, query: &Query) -> Vec<Todo>;
}
