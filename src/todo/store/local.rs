use super::error::Result;
use super::Store;
use crate::todo::Todo;

// TODO #99 (A) Complete the LocalStore +feat
//
// Local backend for todo data. Todos are imported into the Store.
// - Use sqlite for persistence
//
// Store Sqlite database in the .git dir in the repo root, so all
// worktrees use the same file.
//
// Keeps todos in a single 'todo' table with columns for all of the 
// properties of the Todo struct.
pub struct LocalStore {
    // db: sqlite
}

impl LocalStore {
    pub fn new() -> Self {
        Self {}
    }
}

impl Store for LocalStore {
    fn get_todo(&self, _id: u32) -> Option<super::Todo> {
        None
    }

    fn set_todo(&self, _todo: &Todo) -> Result<u32> {
        Ok(0)
    }

    fn update_todo(&self, _id: u32, _todo: super::Todo) -> Result<()> {
        Ok(())
    }

    fn get_todos(&self) -> Vec<Todo> {
        vec![]
    }

    fn query_todos(&self, _query: &super::Query) -> Vec<super::Todo> {
        vec![]
    }
}
