// VCS interface for extracting todo history (TODO #64)
//
// This module provides a VCS abstraction layer that extracts TODO lifecycle data
// (creation/completion dates, authors) from git history. The VCS becomes the source
// of truth for dates rather than in-comment fields.

use std::fmt;
use std::path::Path;

pub mod cache;
pub mod error;
pub mod git;

use super::Provider;
use crate::todo::{Todo, Todos};
use error::Result;

pub use git::CommitMetadata;

/// A single event in a TODO's lifecycle (creation or removal).
#[derive(Debug, Clone)]
pub struct TodoEvent<M> {
    pub event_type: EventType,
    pub meta: M,
    // pub commit_sha: String,
    // pub timestamp: DateTime<Utc>,
    // pub author_name: String,
    // pub author_email: String,
    pub todo: Todo,
}

/// The type of TODO event.
#[derive(Debug, Clone, PartialEq)]
pub enum EventType {
    Created,
    Removed,
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventType::Created => write!(f, "created"),
            EventType::Removed => write!(f, "removed"),
        }
    }
}

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

    fn get_all_historical_ids(&self) -> Result<Vec<u32>> {
        Ok(self
            .get_all_todos()?
            .iter()
            .filter_map(|todo| match todo.id {
                Some(crate::todo::TodoIdentifier::Primary(id)) => Some(id),
                _ => None,
            })
            .collect())
    }

    fn get_max_historical_id(&self) -> Result<u32> {
        Ok(self.get_all_historical_ids()?.into_iter().max().unwrap_or(0))
    }
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
    pub fn from_repo_path(repo_path: &Path, todo_token: &str) -> Result<Self> {
        Ok(Self::new(git::GitBackend::new(repo_path, todo_token)?))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_display() {
        assert_eq!(format!("{}", EventType::Created), "created");
        assert_eq!(format!("{}", EventType::Removed), "removed");
    }
}
