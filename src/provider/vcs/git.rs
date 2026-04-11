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
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use git2::{Commit, Oid, Repository};
use rayon::prelude::*;
use rusqlite::Connection;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

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

/// Location of a TODO in a specific file at a specific commit.
#[derive(Debug, Clone)]
pub struct TodoLocation {
    pub todo_id: u32,
    pub file_path: String,
    pub start_line: u32,
    pub end_line: u32,
}

/// Lifecycle data for a TODO extracted from the cache.
#[derive(Debug, Clone)]
pub struct TodoLifecycle {
    pub todo_id: u32,
    pub creation_date: Option<NaiveDate>,
    pub completion_date: Option<NaiveDate>,
}

/// SQLite-based persistent cache for TODO history tracking.
pub struct TodoCache {
    conn: Connection,
}

impl TodoCache {
    /// Open (or create) the cache database for the given repository.
    pub fn open(repo: &Repository) -> Result<Self> {
        let db_path = Self::get_db_path(repo)?;
        let conn = Connection::open(&db_path)?;
        let cache = TodoCache { conn };
        cache.init_schema()?;
        Ok(cache)
    }

    /// Get the path to the cache database.
    /// Uses commondir() to share cache across worktrees.
    fn get_db_path(repo: &Repository) -> Result<PathBuf> {
        // commondir() returns the shared .git directory across all worktrees
        let git_dir = repo.commondir();
        let todoozy_dir = git_dir.join("todoozy");
        std::fs::create_dir_all(&todoozy_dir)?;
        Ok(todoozy_dir.join("cache.db"))
    }

    /// Initialize the database schema.
    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS commits (
                sha TEXT PRIMARY KEY,
                timestamp INTEGER NOT NULL,
                author_name TEXT NOT NULL,
                author_email TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_commits_timestamp ON commits(timestamp);

            CREATE TABLE IF NOT EXISTS todo_locations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                commit_sha TEXT NOT NULL REFERENCES commits(sha),
                todo_id INTEGER NOT NULL,
                file_path TEXT NOT NULL,
                start_line INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                UNIQUE(commit_sha, todo_id, file_path, start_line)
            );
            CREATE INDEX IF NOT EXISTS idx_locations_commit ON todo_locations(commit_sha);
            CREATE INDEX IF NOT EXISTS idx_locations_todo ON todo_locations(todo_id);
            ",
        )?;
        Ok(())
    }

    /// Get the set of commit SHAs that have already been parsed.
    pub fn get_parsed_commits(&self) -> Result<HashSet<String>> {
        let mut stmt = self.conn.prepare("SELECT sha FROM commits")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        let mut commits = HashSet::new();
        for sha in rows {
            commits.insert(sha?);
        }
        Ok(commits)
    }

    /// Insert a commit and its TODO locations into the cache.
    pub fn insert_commit(&mut self, meta: &CommitMetadata, todos: &[Todo]) -> Result<()> {
        let tx = self.conn.transaction()?;

        // Insert commit record
        tx.execute(
            "INSERT OR IGNORE INTO commits (sha, timestamp, author_name, author_email) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                &meta.sha,
                meta.timestamp.timestamp(),
                &meta.author_name,
                &meta.author_email
            ],
        )?;

        // Insert location records for each TODO
        {
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO todo_locations (commit_sha, todo_id, file_path, start_line, end_line) VALUES (?1, ?2, ?3, ?4, ?5)"
            )?;

            for todo in todos {
                if let Some(TodoIdentifier::Primary(id)) = &todo.id {
                    let file_path = todo.location.file_path.as_deref().unwrap_or("");
                    let start_line = todo.location.start_line_num as u32;
                    let end_line = todo.location.end_line_num as u32;
                    stmt.execute(rusqlite::params![
                        &meta.sha, id, file_path, start_line, end_line
                    ])?;
                }
            }
        }

        tx.commit()?;
        Ok(())
    }

    /// Insert multiple commits and their TODO locations in a single transaction.
    pub fn insert_commits_batch(&mut self, commits: &[(CommitMetadata, Vec<Todo>)]) -> Result<()> {
        if commits.is_empty() {
            return Ok(());
        }

        let tx = self.conn.transaction()?;

        {
            let mut commit_stmt = tx.prepare(
                "INSERT OR IGNORE INTO commits (sha, timestamp, author_name, author_email) VALUES (?1, ?2, ?3, ?4)"
            )?;
            let mut location_stmt = tx.prepare(
                "INSERT OR IGNORE INTO todo_locations (commit_sha, todo_id, file_path, start_line, end_line) VALUES (?1, ?2, ?3, ?4, ?5)"
            )?;

            for (meta, todos) in commits {
                commit_stmt.execute(rusqlite::params![
                    &meta.sha,
                    meta.timestamp.timestamp(),
                    &meta.author_name,
                    &meta.author_email
                ])?;

                for todo in todos {
                    if let Some(TodoIdentifier::Primary(id)) = &todo.id {
                        let file_path = todo.location.file_path.as_deref().unwrap_or("");
                        let start_line = todo.location.start_line_num as u32;
                        let end_line = todo.location.end_line_num as u32;
                        location_stmt.execute(rusqlite::params![
                            &meta.sha, id, file_path, start_line, end_line
                        ])?;
                    }
                }
            }
        }

        tx.commit()?;
        Ok(())
    }

    /// Get lifecycle data for a specific TODO.
    pub fn get_todo_lifecycle(&self, todo_id: u32, head_sha: &str) -> Result<TodoLifecycle> {
        // Get creation date (oldest commit containing this todo)
        let creation_timestamp: Option<i64> = self
            .conn
            .query_row(
                "SELECT MIN(c.timestamp) FROM commits c
                 JOIN todo_locations l ON l.commit_sha = c.sha
                 WHERE l.todo_id = ?1",
                [todo_id],
                |row| row.get(0),
            )
            .ok();

        // Check if todo exists in HEAD
        let exists_in_head: bool = self
            .conn
            .query_row(
                "SELECT 1 FROM todo_locations WHERE commit_sha = ?1 AND todo_id = ?2 LIMIT 1",
                rusqlite::params![head_sha, todo_id],
                |_| Ok(true),
            )
            .unwrap_or(false);

        // Get completion date (last commit where it existed, only if NOT in HEAD)
        let completion_timestamp: Option<i64> = if !exists_in_head {
            self.conn
                .query_row(
                    "SELECT MAX(c.timestamp) FROM commits c
                     JOIN todo_locations l ON l.commit_sha = c.sha
                     WHERE l.todo_id = ?1",
                    [todo_id],
                    |row| row.get(0),
                )
                .ok()
        } else {
            None
        };

        let creation_date = creation_timestamp
            .and_then(|ts| Utc.timestamp_opt(ts, 0).single())
            .map(|dt| dt.date_naive());

        let completion_date = completion_timestamp
            .and_then(|ts| Utc.timestamp_opt(ts, 0).single())
            .map(|dt| dt.date_naive());

        Ok(TodoLifecycle {
            todo_id,
            creation_date,
            completion_date,
        })
    }

    /// Get all TODO IDs and locations for a specific commit.
    pub fn get_todos_at_commit(&self, commit_sha: &str) -> Result<Vec<TodoLocation>> {
        let mut stmt = self.conn.prepare(
            "SELECT todo_id, file_path, start_line, end_line FROM todo_locations WHERE commit_sha = ?1",
        )?;
        let rows = stmt.query_map([commit_sha], |row| {
            Ok(TodoLocation {
                todo_id: row.get(0)?,
                file_path: row.get(1)?,
                start_line: row.get(2)?,
                end_line: row.get(3)?,
            })
        })?;
        let mut locations = Vec::new();
        for loc in rows {
            locations.push(loc?);
        }
        Ok(locations)
    }

    /// Get all unique TODO IDs that have ever existed in the repository.
    pub fn get_all_todo_ids(&self) -> Result<HashSet<u32>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT todo_id FROM todo_locations")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        let mut ids = HashSet::new();
        for id in rows {
            ids.insert(id?);
        }
        Ok(ids)
    }
}

/// Result of parsing a single commit - used for parallel processing.
struct ParsedCommit {
    meta: CommitMetadata,
    todos: Vec<Todo>,
}

/// Git-based VCS backend for extracting TODO lifecycle data.
pub struct GitBackend {
    repo: Repository,
    repo_path: PathBuf,
    parser: TodoParser,
    cache: RefCell<TodoCache>,
}

impl GitBackend {
    pub fn new(path: &Path, todo_token: &str) -> Result<Self> {
        let repo = Repository::discover(path).map_err(|e| {
            if e.code() == git2::ErrorCode::NotFound {
                Error::NotARepository
            } else {
                Error::from(e)
            }
        })?;

        // Store the repo path for parallel workers to open their own connections
        let repo_path = repo.workdir().unwrap_or_else(|| repo.path()).to_path_buf();

        let cache = RefCell::new(TodoCache::open(&repo)?);

        Ok(GitBackend {
            repo,
            repo_path,
            parser: TodoParser::new(todo_token),
            cache,
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

    /// Walk commits incrementally with parallel parsing, skipping already-cached commits.
    fn walk_commits_for_todos(&self) -> Result<Todos> {
        // Phase 1: Collect unparsed commit OIDs (single-threaded revwalk)
        let unparsed_oids = self.collect_unparsed_oids()?;

        if !unparsed_oids.is_empty() {
            // Phase 2: Parse commits in parallel
            // Each worker opens its own Repository (git2 isn't Send)
            let repo_path = &self.repo_path;
            let parser = &self.parser;

            let results: Vec<Result<ParsedCommit>> = unparsed_oids
                .par_iter()
                .map(|oid| {
                    let thread_repo = Repository::open(repo_path)?;
                    Self::parse_commit_standalone(&thread_repo, parser, *oid)
                })
                .collect();

            // Phase 3: Collect results and handle errors
            let mut parsed_commits: Vec<(CommitMetadata, Vec<Todo>)> =
                Vec::with_capacity(results.len());
            for result in results {
                let parsed = result?;
                parsed_commits.push((parsed.meta, parsed.todos));
            }

            // Phase 4: Batch insert to cache
            eprintln!("Caching {} new commits", parsed_commits.len());
            self.cache
                .borrow_mut()
                .insert_commits_batch(&parsed_commits)?;
        }

        // Build todos with lifecycle data from cache
        self.build_todos_with_lifecycle()
    }

    /// Collect all commit OIDs that haven't been parsed yet.
    fn collect_unparsed_oids(&self) -> Result<Vec<Oid>> {
        let parsed = self.cache.borrow().get_parsed_commits()?;
        let mut revwalk = self.repo.revwalk()?;

        revwalk.push_head().map_err(|e| {
            if e.code() == git2::ErrorCode::UnbornBranch {
                return Error::GitError("repository has no commits".to_string());
            }
            Error::from(e)
        })?;

        revwalk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME | git2::Sort::REVERSE)?;

        let mut unparsed = Vec::new();
        for oid_result in revwalk {
            let oid = oid_result?;
            if !parsed.contains(&oid.to_string()) {
                unparsed.push(oid);
            }
        }

        Ok(unparsed)
    }

    /// Parse a single commit - called in parallel in walk_commits_for_todos.
    fn parse_commit_standalone(
        repo: &Repository,
        parser: &TodoParser,
        oid: Oid,
    ) -> Result<ParsedCommit> {
        let commit = repo.find_commit(oid)?;
        let meta = CommitMetadata::from(&commit);

        let commit_tree = commit.tree()?;
        let mut todos = Vec::new();

        commit_tree.walk(git2::TreeWalkMode::PreOrder, |_, entry| {
            let file_path = entry.name().unwrap_or("").to_string();
            if let Some(ft) = Path::new(&file_path).get_filetype_from_name() {
                if let Ok(blob) = repo.find_blob(entry.id()) {
                    let parsed = parser.parse_bytes(blob.content(), ft);
                    for mut todo in parsed {
                        if let Some(TodoIdentifier::Primary(_)) = &todo.id {
                            todo.location.file_path = Some(file_path.clone());
                            todo.creation_date = Some(meta.timestamp.date_naive());
                            todos.push(todo);
                        }
                    }
                }
            }
            git2::TreeWalkResult::Ok
        })?;

        Ok(ParsedCommit { meta, todos })
    }

    /// Build the final Todos collection using lifecycle data from the cache.
    fn build_todos_with_lifecycle(&self) -> Result<Todos> {
        let head = self.repo.head()?;
        let head_commit = head.peel_to_commit()?;
        let head_sha = head_commit.id().to_string();

        // Get all TODO IDs from the cache
        let all_ids = self.cache.borrow().get_all_todo_ids()?;

        // Get current worktree TODOs (for content)
        let mut current_todos: HashMap<u32, Todo> = self
            .new_todos_for_commit(&head_commit)?
            .into_iter()
            .filter_map(|todo| {
                if let Some(TodoIdentifier::Primary(id)) = &todo.id {
                    Some((*id, todo))
                } else {
                    None
                }
            })
            .collect();

        // Build final todos list with lifecycle data
        let mut todos = Todos::from(Vec::new());

        for id in all_ids {
            let lifecycle = self.cache.borrow().get_todo_lifecycle(id, &head_sha)?;

            if let Some(mut todo) = current_todos.remove(&id) {
                // TODO exists in current state - use its content, add lifecycle
                todo.creation_date = lifecycle.creation_date;
                todo.completion_date = lifecycle.completion_date;
                todos.insert(id, todo);
            } else if lifecycle.creation_date.is_some() {
                // TODO was removed - create a minimal record with lifecycle data
                let mut todo = Todo::default();
                todo.id = Some(TodoIdentifier::Primary(id));
                todo.creation_date = lifecycle.creation_date;
                todo.completion_date = lifecycle.completion_date;
                todo.title = format!("[Removed TODO #{}]", id);
                todos.insert(id, todo);
            }
        }

        Ok(todos)
    }

    /// Parse TODOs from file content, returning a map of ID -> Todo for primary TODOs.
    fn parse_blob_from_oid(&self, oid: Oid, file_path: &str) -> Result<HashMap<u32, Todo>> {
        let blob = self.repo.find_blob(oid)?;
        let file_type = match Path::new(file_path).get_filetype_from_name() {
            Some(ft) => ft,
            // TODO #80 (C) 2026-04-08 Error/warning here on missing file type so we can catch more +fix
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

    // TodoCache tests

    #[test]
    fn test_todo_cache_schema_creation() {
        let (dir, repo) = create_test_repo();
        let cache = TodoCache::open(&repo).expect("failed to open cache");

        // Check that cache.db was created in .git/todoozy/
        let db_path = dir.path().join(".git/todoozy/cache.db");
        assert!(db_path.exists());

        // Verify tables exist by querying them
        let parsed = cache.get_parsed_commits().expect("failed to query commits");
        assert!(parsed.is_empty());

        let ids = cache.get_all_todo_ids().expect("failed to query todo ids");
        assert!(ids.is_empty());
    }

    #[test]
    fn test_todo_cache_insert_and_query_commits() {
        let (_dir, repo) = create_test_repo();
        let mut cache = TodoCache::open(&repo).expect("failed to open cache");

        // Create test commit metadata
        let meta = CommitMetadata {
            sha: "abc123".to_string(),
            timestamp: Utc::now(),
            author_name: "Test".to_string(),
            author_email: "test@test.com".to_string(),
        };

        // Insert commit with no todos
        cache.insert_commit(&meta, &[]).expect("failed to insert");

        let parsed = cache.get_parsed_commits().expect("failed to query");
        assert!(parsed.contains("abc123"));
    }

    #[test]
    fn test_todo_cache_insert_commit_with_todos() {
        let (_dir, repo) = create_test_repo();
        let mut cache = TodoCache::open(&repo).expect("failed to open cache");

        let meta = CommitMetadata {
            sha: "def456".to_string(),
            timestamp: Utc::now(),
            author_name: "Test".to_string(),
            author_email: "test@test.com".to_string(),
        };

        let mut todo = Todo::default();
        todo.id = Some(TodoIdentifier::Primary(42));
        todo.location.file_path = Some("test.rs".to_string());
        todo.location.start_line_num = 10;
        todo.location.end_line_num = 12;

        cache
            .insert_commit(&meta, &[todo])
            .expect("failed to insert");

        // Verify todo ID was recorded
        let ids = cache.get_all_todo_ids().expect("failed to query");
        assert!(ids.contains(&42));

        // Verify location was recorded
        let locations = cache
            .get_todos_at_commit("def456")
            .expect("failed to query");
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].todo_id, 42);
        assert_eq!(locations[0].file_path, "test.rs");
        assert_eq!(locations[0].start_line, 10);
        assert_eq!(locations[0].end_line, 12);
    }

    #[test]
    fn test_todo_cache_lifecycle_active_todo() {
        let (_dir, repo) = create_test_repo();
        let mut cache = TodoCache::open(&repo).expect("failed to open cache");

        let meta = CommitMetadata {
            sha: "head123".to_string(),
            timestamp: Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap(),
            author_name: "Test".to_string(),
            author_email: "test@test.com".to_string(),
        };

        let mut todo = Todo::default();
        todo.id = Some(TodoIdentifier::Primary(1));
        todo.location.file_path = Some("test.rs".to_string());

        cache
            .insert_commit(&meta, &[todo])
            .expect("failed to insert");

        // Query lifecycle - todo exists in "HEAD" (head123)
        let lifecycle = cache
            .get_todo_lifecycle(1, "head123")
            .expect("failed to query");

        assert!(lifecycle.creation_date.is_some());
        assert!(lifecycle.completion_date.is_none()); // Still active in HEAD
    }

    #[test]
    fn test_todo_cache_lifecycle_completed_todo() {
        let (_dir, repo) = create_test_repo();
        let mut cache = TodoCache::open(&repo).expect("failed to open cache");

        // First commit: add todo
        let meta1 = CommitMetadata {
            sha: "commit1".to_string(),
            timestamp: Utc.with_ymd_and_hms(2024, 1, 10, 12, 0, 0).unwrap(),
            author_name: "Test".to_string(),
            author_email: "test@test.com".to_string(),
        };

        let mut todo = Todo::default();
        todo.id = Some(TodoIdentifier::Primary(1));
        todo.location.file_path = Some("test.rs".to_string());

        cache
            .insert_commit(&meta1, &[todo])
            .expect("failed to insert");

        // Second commit: todo removed (empty list)
        let meta2 = CommitMetadata {
            sha: "head456".to_string(),
            timestamp: Utc.with_ymd_and_hms(2024, 1, 20, 12, 0, 0).unwrap(),
            author_name: "Test".to_string(),
            author_email: "test@test.com".to_string(),
        };

        cache.insert_commit(&meta2, &[]).expect("failed to insert");

        // Query lifecycle - todo NOT in HEAD (head456)
        let lifecycle = cache
            .get_todo_lifecycle(1, "head456")
            .expect("failed to query");

        assert!(lifecycle.creation_date.is_some());
        assert!(lifecycle.completion_date.is_some()); // Completed since not in HEAD
        assert_eq!(lifecycle.creation_date.unwrap().to_string(), "2024-01-10");
        assert_eq!(lifecycle.completion_date.unwrap().to_string(), "2024-01-10");
    }

    #[test]
    fn test_todo_cache_incremental_updates() {
        let (dir, _repo) = create_test_repo();

        // Create initial commit
        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #100 First task\nfn main() {}",
            "Initial commit",
        );

        // First run - should cache the commit
        let backend1 = GitBackend::new(dir.path(), "TODO").expect("failed to create backend");
        let todos1 = backend1.walk_commits_for_todos().expect("failed to scan");
        assert_eq!(todos1.len(), 1);

        // Get cached commits count
        let parsed1 = backend1
            .cache
            .borrow()
            .get_parsed_commits()
            .expect("failed to query");
        let count1 = parsed1.len();

        // Add another commit
        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #100 First task\n// TODO #200 Second task\nfn main() {}",
            "Add second TODO",
        );

        // Second run - should only cache new commit
        let backend2 = GitBackend::new(dir.path(), "TODO").expect("failed to create backend");
        let todos2 = backend2.walk_commits_for_todos().expect("failed to scan");
        assert_eq!(todos2.len(), 2);

        let parsed2 = backend2
            .cache
            .borrow()
            .get_parsed_commits()
            .expect("failed to query");
        let count2 = parsed2.len();

        // Should have one more cached commit
        assert_eq!(count2, count1 + 1);
    }

    #[test]
    fn test_todo_cache_database_path() {
        let (dir, repo) = create_test_repo();
        let _cache = TodoCache::open(&repo).expect("failed to open cache");

        // Verify .git/todoozy/cache.db exists
        let expected_path = dir.path().join(".git/todoozy/cache.db");
        assert!(
            expected_path.exists(),
            "cache.db should be created at .git/todoozy/cache.db"
        );
    }
}
