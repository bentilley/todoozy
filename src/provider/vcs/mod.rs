// VCS interface for extracting todo history (TODO #64)
//
// This module provides a VCS abstraction layer that extracts TODO lifecycle data
// (creation/completion dates, authors) from git history. The VCS becomes the source
// of truth for dates rather than in-comment fields.

use std::path::Path;

pub mod error;
pub mod git;

use super::Provider;
use crate::todo::{Todo, Todos};
use error::Result;
use std::collections::HashSet;

pub use git::CommitMetadata;

/// Trait for VCS backends that can extract TODO lifecycle data.
///
/// This trait allows for different VCS implementations (git, hg, svn)
/// to provide TODO history information. Each implementation defines its
/// own metadata type via the associated `Meta` type.
pub trait VcsBackend: Send {
    /// Scan the entire VCS history for all TODOs
    ///
    /// This is useful for building a complete cache of TODO history.
    fn get_all_todos(&self) -> Result<Todos>;

    fn get_todo_for_version(&self, id: u32, version: String) -> Result<Todo>;

    fn get_todos_for_version(&self, id: &[u32], version: String) -> Result<Todos>;

    fn get_all_todos_for_version(&self, version: String) -> Result<Todos>;

    fn hydrate_todo(&self, todo: &mut Todo) -> Result<()> {
        self.hydrate_todos(&mut [todo])
    }

    fn hydrate_todos(&self, todos: &mut [&mut Todo]) -> Result<()>;

    fn get_all_ids(&self) -> Result<HashSet<u32>>;

    fn get_ids_for_version(&self, version: String) -> Result<HashSet<u32>>;

    fn get_max_id(&self) -> Result<u32> {
        Ok(self.get_all_ids()?.into_iter().max().unwrap_or(0))
    }

    fn get_most_recent_version(&self) -> Result<String>;
}

pub struct VcsProvider<B: VcsBackend> {
    backend: B,
}

impl<B: VcsBackend> VcsProvider<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }
}

impl VcsProvider<git::GitBackend> {
    pub fn from_repo_path(
        repo_path: &Path,
        todo_token: &str,
        history_start: Option<String>,
    ) -> Result<Self> {
        Ok(Self::new(git::GitBackend::new(
            repo_path,
            todo_token,
            history_start,
        )?))
    }
}

impl<B: VcsBackend> Provider for VcsProvider<B> {
    fn get_todos(&self) -> super::Result<crate::todo::Todos> {
        Ok(self.backend.get_all_todos()?)
    }

    fn get_todo(&self, id: u32) -> super::Result<Option<Todo>> {
        Ok(self.get_todos()?.get(&id).cloned())
    }
}
