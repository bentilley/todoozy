// Git backend for VCS TODO history extraction
//
// This module implements the VcsBackend trait for git repositories using git2.
// It walks the commit history, retrieves full file contents from blobs, and
// compares TODOs between versions to detect creation/removal events.

use super::{
    error::{Error, Result},
    VcsBackend,
};
use crate::fs::FileTypeAwarePath;
use crate::todo::{parser::TodoParser, Todo, TodoIdentifier, Todos};
use chrono::{DateTime, TimeZone, Utc};
use git2::{Commit, Oid, Repository};
use std::collections::HashMap;
use std::path::Path;

/// Metadata extracted from a commit.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommitMetadata {
    pub sha: String,
    pub timestamp: DateTime<Utc>,
    pub author_name: String,
    pub author_email: String,
}

impl From<&Commit<'_>> for CommitMetadata {
    fn from(commit: &Commit) -> Self {
        let timestamp = Utc
            .timestamp_opt(commit.time().seconds(), 0)
            .single()
            .unwrap_or_else(Utc::now);
        let author = commit.author();
        Self {
            sha: commit.id().to_string(),
            timestamp,
            author_name: author.name().unwrap_or("Unknown").to_string(),
            author_email: author.email().unwrap_or("").to_string(),
        }
    }
}

/// Git-based VCS backend for extracting TODO lifecycle data.
pub struct GitBackend {
    repo: Repository,
    parser: TodoParser,
}

impl GitBackend {
    /// Open a git repository at the given path.
    ///
    /// The path can be anywhere within the repository; git2 will find the root.
    pub fn new(path: &Path, todo_token: &str) -> Result<Self> {
        let repo = Repository::discover(path).map_err(|e| {
            if e.code() == git2::ErrorCode::NotFound {
                Error::NotARepository
            } else {
                Error::from(e)
            }
        })?;
        Ok(GitBackend {
            repo,
            parser: TodoParser::new(todo_token),
        })
    }

    pub fn merge_batches(mut old: Todos, mut new: Todos, batch_timestamp: &DateTime<Utc>) -> Todos {
        // Track which IDs exist in new (for detecting completed todos)
        let other_ids: std::collections::HashSet<u32> = new.ids().collect();

        // Mark todos that exist in self but not in other as completed
        for (id, todo) in old.iter_mut() {
            if !other_ids.contains(id) && todo.completion_date.is_none() {
                todo.completion_date = Some(batch_timestamp.date_naive());
            }
        }

        // Merge in todos from other
        for (id, todo) in new.iter_mut() {
            if let Some(existing) = old.get(id) {
                // Preserve creation_date from existing todo
                if existing.creation_date.is_some() {
                    todo.creation_date = existing.creation_date;
                }
            }
            old.insert(id.clone(), todo.to_owned());
        }

        old
    }

    /// Walk all commits and extract TODO events.
    pub fn walk_commits_for_todos(&self) -> Result<Todos> {
        let mut revwalk = self.repo.revwalk()?;

        revwalk.push_head().map_err(|e| {
            if e.code() == git2::ErrorCode::UnbornBranch {
                return Error::GitError("repository has no commits".to_string());
            }
            Error::from(e)
        })?;

        revwalk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME | git2::Sort::REVERSE)?;

        let mut all_todos: Todos = Vec::new().into();
        let mut todos_by_commit: HashMap<CommitMetadata, Vec<Todo>> = HashMap::new();
        let mut commits_in_order: Vec<CommitMetadata> = Vec::new();

        for oid_result in revwalk {
            let oid = oid_result?;
            let commit = self.repo.find_commit(oid)?;
            let meta = CommitMetadata::from(&commit);
            let todos = self.new_todos_for_commit(&commit)?;
            commits_in_order.push(meta.clone());
            todos_by_commit.insert(meta.clone(), todos.clone());
            all_todos = Self::merge_batches(all_todos, todos.into(), &meta.timestamp);
        }

        for meta in &commits_in_order {
            let todos = &todos_by_commit[meta];
            println!(
                "Commit {} at {}: {} TODOs",
                meta.sha,
                meta.timestamp,
                todos.len()
            );
            for todo in todos {
                println!("  - {}", todo);
            }
            println!();
        }

        Ok(all_todos)
    }

    /// Parse TODOs from file content, returning a map of ID -> Todo for primary TODOs.
    fn parse_blob_from_oid(&self, oid: Oid, file_path: &str) -> Result<HashMap<u32, Todo>> {
        let blob = self.repo.find_blob(oid)?;
        let file_type = match Path::new(file_path).get_filetype_from_name() {
            Some(ft) => ft,
            // TODO (C) 2026-04-08 Error/warning here on missing file type so we can catch more +fix
            None => return Ok(HashMap::new()), // If we can't determine the file type, just return no TODOs
        };

        Ok(self
            .parser
            .parse_bytes(blob.content(), file_type)
            .into_iter()
            .filter_map(|mut todo| match &todo.id {
                Some(TodoIdentifier::Primary(id)) => {
                    todo.location.file_path = Some(file_path.to_string());
                    Some((*id, todo))
                }
                _ => None,
            })
            .collect())
    }

    /// Get all TODOs for a given commit by walking its tree and parsing blobs.
    fn new_todos_for_commit(&self, commit: &Commit) -> Result<Vec<Todo>> {
        let commit_tree = commit.tree()?;
        let mut all_todos = Vec::<Todo>::new();

        commit_tree.walk(git2::TreeWalkMode::PreOrder, |_, entry| {
            let file_path = entry.name().unwrap_or("").to_string();
            if let Ok(todos) = self.parse_blob_from_oid(entry.id(), &file_path) {
                all_todos.extend(todos.into_values());
            }
            git2::TreeWalkResult::Ok
        })?;

        let meta = CommitMetadata::from(commit);
        for t in &mut all_todos {
            t.creation_date = Some(meta.timestamp.date_naive());
        }

        Ok(all_todos)
    }
}

impl VcsBackend for GitBackend {
    fn get_all_todos(&self) -> Result<Todos> {
        self.walk_commits_for_todos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    /// Helper to create a test git repository.
    fn create_test_repo() -> (TempDir, Repository) {
        let dir = TempDir::new().expect("failed to create temp dir");

        Command::new("git")
            .args(["init"])
            .current_dir(dir.path())
            .output()
            .expect("failed to init repo");

        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(dir.path())
            .output()
            .expect("failed to set email");

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(dir.path())
            .output()
            .expect("failed to set name");

        Command::new("git")
            .args(["config", "commit.gpgsign", "false"])
            .current_dir(dir.path())
            .output()
            .expect("failed to disable gpg signing");

        let repo = Repository::open(dir.path()).expect("failed to open repo");
        (dir, repo)
    }

    /// Helper to commit a file.
    fn commit_file(dir: &Path, filename: &str, content: &str, message: &str) {
        let file_path = dir.join(filename);
        fs::write(&file_path, content).expect("failed to write file");

        Command::new("git")
            .args(["add", filename])
            .current_dir(dir)
            .output()
            .expect("failed to add file");

        Command::new("git")
            .args(["commit", "-m", message])
            .current_dir(dir)
            .output()
            .expect("failed to commit");
    }

    #[test]
    fn test_git_backend_not_a_repo() {
        let dir = TempDir::new().expect("failed to create temp dir");
        let result = GitBackend::new(dir.path(), "TODO");
        assert!(matches!(result, Err(Error::NotARepository)));
    }

    #[test]
    fn test_git_backend_empty_repo() {
        let (dir, _repo) = create_test_repo();
        let backend = GitBackend::new(dir.path(), "TODO").expect("failed to create backend");
        let result = backend.walk_commits_for_todos();
        assert!(result.is_err());
    }

    #[test]
    fn test_git_backend_detects_todo_creation() {
        let (dir, _repo) = create_test_repo();

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #1 Fix this bug\nfn main() {}",
            "Add TODO #1",
        );

        let backend = GitBackend::new(dir.path(), "TODO").expect("failed to create backend");
        let todos = backend.walk_commits_for_todos().expect("failed to scan");

        assert_eq!(todos.len(), 1);
        let todo = todos.get(&1).expect("TODO #1 should exist");
        assert_eq!(todo.id, Some(TodoIdentifier::Primary(1)));
        assert_eq!(todo.title, "Fix this bug");
        assert!(todo.creation_date.is_some());
        assert!(todo.completion_date.is_none()); // Not removed
    }

    #[test]
    fn test_git_backend_detects_todo_removal() {
        let (dir, _repo) = create_test_repo();

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #2 Fix this\nfn main() {}",
            "Add TODO",
        );

        commit_file(dir.path(), "main.rs", "fn main() {}", "Remove TODO");

        let backend = GitBackend::new(dir.path(), "TODO").expect("failed to create backend");
        let todos = backend.walk_commits_for_todos().expect("failed to scan");

        // The todo should still exist but with a completion_date set
        assert_eq!(todos.len(), 1);
        let todo = todos.get(&2).expect("TODO #2 should exist");
        assert!(todo.creation_date.is_some());
        assert!(todo.completion_date.is_some()); // Marked as removed
    }

    #[test]
    fn test_git_backend_multiple_todos() {
        let (dir, _repo) = create_test_repo();

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #10 First\n// TODO #20 Second\nfn main() {}",
            "Add multiple TODOs",
        );

        let backend = GitBackend::new(dir.path(), "TODO").expect("failed to create backend");
        let todos = backend.walk_commits_for_todos().expect("failed to scan");

        assert_eq!(todos.len(), 2);
        let ids: Vec<_> = todos.ids().collect();
        assert!(ids.contains(&10));
        assert!(ids.contains(&20));
    }

    #[test]
    fn test_git_backend_ignores_references() {
        let (dir, _repo) = create_test_repo();

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #5 Primary\n// TODO &5 Reference\nfn main() {}",
            "Add TODO with reference",
        );

        let backend = GitBackend::new(dir.path(), "TODO").expect("failed to create backend");
        let todos = backend.walk_commits_for_todos().expect("failed to scan");

        // Should only find the primary TODO, not the reference
        assert_eq!(todos.len(), 1);
        let todo = todos.get(&5).expect("TODO #5 should exist");
        assert_eq!(todo.id, Some(TodoIdentifier::Primary(5)));
    }

    #[test]
    fn test_git_backend_extracts_todo_content() {
        let (dir, _repo) = create_test_repo();

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #42 (A) Fix bug +urgent\nfn main() {}",
            "Add TODO",
        );

        let backend = GitBackend::new(dir.path(), "TODO").expect("failed to create backend");
        let todos = backend.walk_commits_for_todos().expect("failed to scan");

        assert_eq!(todos.len(), 1);
        let todo = todos.get(&42).expect("TODO #42 should exist");
        assert_eq!(todo.title, "Fix bug");
        assert_eq!(todo.priority, Some('A'));
        assert!(todo.tags.contains(&"urgent".to_string()));
    }
}
