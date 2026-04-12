// Git backend for VCS TODO history extraction

use super::{
    error::{Error, Result},
    VcsBackend,
};
use crate::todo::{parser::TodoParser, Todo, TodoIdentifier, Todos};
use crate::fs::FileTypeAwarePath;
use chrono::{DateTime, TimeZone, Utc};
use git2::{Commit, Oid, Repository};
use rayon::prelude::*;
use rusqlite::Connection;
use std::cell::RefCell;
use std::collections::HashSet;
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
/// SQLite-based persistent cache for TODO history tracking.
struct Cache {
    conn: Connection,
}

impl Cache {
    /// Open (or create) the cache database for the given repository.
    pub fn open(repo: &Repository) -> Result<Self> {
        let db_path = Self::get_db_path(repo)?;
        let conn = Connection::open(&db_path)?;
        let cache = Cache { conn };
        cache.init_schema()?;
        Ok(cache)
    }

    /// Get the path to the cache database.
    fn get_db_path(repo: &Repository) -> Result<PathBuf> {
        let git_dir = repo.commondir(); // shared .git directory across all worktrees
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

    /// Insert multiple commits and their TODO locations in a single transaction.
    pub fn insert_commits(&mut self, commits: &[(CommitMetadata, Vec<Todo>)]) -> Result<()> {
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

    /// Get lifecycle data for a specific TODO, including location at the given SHA.
    pub fn get_todo_lifecycle(&self, todo_id: u32, sha: &str, repo_path: &Path) -> Result<Todo> {
        let row: (Option<i64>, Option<i64>, i32, Option<String>, Option<u32>, Option<u32>) = self
            .conn
            .query_row(
                "SELECT
                    MIN(c.timestamp) as creation_ts,
                    MAX(c.timestamp) as last_seen_ts,
                    MAX(CASE WHEN l.commit_sha = ?2 THEN 1 ELSE 0 END) as exists_in_sha,
                    MAX(CASE WHEN l.commit_sha = ?2 THEN l.file_path END) as file_path,
                    MAX(CASE WHEN l.commit_sha = ?2 THEN l.start_line END) as start_line,
                    MAX(CASE WHEN l.commit_sha = ?2 THEN l.end_line END) as end_line
                FROM todo_locations l
                JOIN commits c ON l.commit_sha = c.sha
                WHERE l.todo_id = ?1",
                rusqlite::params![todo_id, sha],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
            )?;

        let (creation_ts, last_seen_ts, exists_in_sha, file_path, start_line, end_line) = row;

        let creation_date = creation_ts.and_then(|ts| Utc.timestamp_opt(ts, 0).single());
        let completion_date = if exists_in_sha == 0 {
            last_seen_ts.and_then(|ts| Utc.timestamp_opt(ts, 0).single())
        } else {
            None
        };

        // Make path absolute by joining with repo_path
        let abs_path = file_path.map(|p| repo_path.join(&p).to_string_lossy().into_owned());

        let mut todo = Todo::default();
        todo.id = Some(TodoIdentifier::Primary(todo_id));
        todo.creation_date = creation_date.map(|d| d.date_naive());
        todo.completion_date = completion_date.map(|d| d.date_naive());
        todo.location.file_path = abs_path;
        todo.location.start_line_num = start_line.unwrap_or(0) as usize;
        todo.location.end_line_num = end_line.unwrap_or(0) as usize;
        Ok(todo)
    }

    /// Get lifecycle data for multiple TODOs, including location at the given SHA.
    pub fn get_todo_lifecycles(&self, todo_ids: &[u32], sha: &str, repo_path: &Path) -> Result<Vec<Todo>> {
        if todo_ids.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders: Vec<String> = (0..todo_ids.len()).map(|i| format!("?{}", i + 1)).collect();
        let sha_param = todo_ids.len() + 1;
        let query = format!(
            "SELECT
                l.todo_id,
                MIN(c.timestamp) as creation_ts,
                MAX(c.timestamp) as last_seen_ts,
                MAX(CASE WHEN l.commit_sha = ?{sha_param} THEN 1 ELSE 0 END) as exists_in_sha,
                MAX(CASE WHEN l.commit_sha = ?{sha_param} THEN l.file_path END) as file_path,
                MAX(CASE WHEN l.commit_sha = ?{sha_param} THEN l.start_line END) as start_line,
                MAX(CASE WHEN l.commit_sha = ?{sha_param} THEN l.end_line END) as end_line
            FROM todo_locations l
            JOIN commits c ON l.commit_sha = c.sha
            WHERE l.todo_id IN ({})
            GROUP BY l.todo_id",
            placeholders.join(", ")
        );

        let mut params: Vec<Box<dyn rusqlite::ToSql>> = todo_ids
            .iter()
            .map(|id| Box::new(*id) as Box<dyn rusqlite::ToSql>)
            .collect();
        params.push(Box::new(sha.to_string()));

        let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let mut stmt = self.conn.prepare(&query)?;
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            Ok((
                row.get::<_, u32>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, i32>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<u32>>(5)?,
                row.get::<_, Option<u32>>(6)?,
            ))
        })?;

        let mut todos = Vec::new();
        for row in rows {
            let (todo_id, creation_ts, last_seen_ts, exists_in_sha, file_path, start_line, end_line) = row?;
            let creation_date = creation_ts.and_then(|ts| Utc.timestamp_opt(ts, 0).single());
            let completion_date = if exists_in_sha == 0 {
                last_seen_ts.and_then(|ts| Utc.timestamp_opt(ts, 0).single())
            } else {
                None
            };
            // Make path absolute by joining with repo_path
            let abs_path = file_path.map(|p| repo_path.join(&p).to_string_lossy().into_owned());

            let mut todo = Todo::default();
            todo.id = Some(TodoIdentifier::Primary(todo_id));
            todo.creation_date = creation_date.map(|d| d.date_naive());
            todo.completion_date = completion_date.map(|d| d.date_naive());
            todo.location.file_path = abs_path;
            todo.location.start_line_num = start_line.unwrap_or(0) as usize;
            todo.location.end_line_num = end_line.unwrap_or(0) as usize;
            todos.push(todo);
        }

        Ok(todos)
    }

    /// Get lifecycle data for all TODOs that have ever existed, including location at the given SHA.
    pub fn get_all_todo_lifecycles(&self, sha: &str, repo_path: &Path) -> Result<Vec<Todo>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                l.todo_id,
                MIN(c.timestamp) as creation_ts,
                MAX(c.timestamp) as last_seen_ts,
                MAX(CASE WHEN l.commit_sha = ?1 THEN 1 ELSE 0 END) as exists_in_sha,
                MAX(CASE WHEN l.commit_sha = ?1 THEN l.file_path END) as file_path,
                MAX(CASE WHEN l.commit_sha = ?1 THEN l.start_line END) as start_line,
                MAX(CASE WHEN l.commit_sha = ?1 THEN l.end_line END) as end_line
            FROM todo_locations l
            JOIN commits c ON l.commit_sha = c.sha
            GROUP BY l.todo_id",
        )?;

        let rows = stmt.query_map([sha], |row| {
            Ok((
                row.get::<_, u32>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, i32>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<u32>>(5)?,
                row.get::<_, Option<u32>>(6)?,
            ))
        })?;

        let mut todos = Vec::new();
        for row in rows {
            let (todo_id, creation_ts, last_seen_ts, exists_in_sha, file_path, start_line, end_line) = row?;
            let creation_date = creation_ts.and_then(|ts| Utc.timestamp_opt(ts, 0).single());
            let completion_date = if exists_in_sha == 0 {
                last_seen_ts.and_then(|ts| Utc.timestamp_opt(ts, 0).single())
            } else {
                None
            };
            // Make path absolute by joining with repo_path
            let abs_path = file_path.map(|p| repo_path.join(&p).to_string_lossy().into_owned());

            let mut todo = Todo::default();
            todo.id = Some(TodoIdentifier::Primary(todo_id));
            todo.creation_date = creation_date.map(|d| d.date_naive());
            todo.completion_date = completion_date.map(|d| d.date_naive());
            todo.location.file_path = abs_path;
            todo.location.start_line_num = start_line.unwrap_or(0) as usize;
            todo.location.end_line_num = end_line.unwrap_or(0) as usize;
            todos.push(todo);
        }

        Ok(todos)
    }

    // /// Get all TODO IDs and locations for a specific commit.
    // pub fn get_todos_at_commit(&self, commit_sha: &str) -> Result<Vec<TodoLocation>> {
    //     let mut stmt = self.conn.prepare(
    //         "SELECT todo_id, file_path, start_line, end_line FROM todo_locations WHERE commit_sha = ?1",
    //     )?;
    //     let rows = stmt.query_map([commit_sha], |row| {
    //         Ok(TodoLocation {
    //             todo_id: row.get(0)?,
    //             file_path: row.get(1)?,
    //             start_line: row.get(2)?,
    //             end_line: row.get(3)?,
    //         })
    //     })?;
    //     let mut locations = Vec::new();
    //     for loc in rows {
    //         locations.push(loc?);
    //     }
    //     Ok(locations)
    // }

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

/// Git-based VCS backend for extracting TODO lifecycle data.
pub struct GitBackend {
    repo: Repository,
    repo_path: PathBuf,
    history_start: Option<String>,
    parser: TodoParser,
    cache: RefCell<Cache>,
}

impl GitBackend {
    pub fn new(path: &Path, todo_token: &str, history_start: Option<String>) -> Result<Self> {
        let repo = Repository::discover(path).map_err(|e| {
            if e.code() == git2::ErrorCode::NotFound {
                Error::NotARepository
            } else {
                Error::from(e)
            }
        })?;

        // Store the repo path for parallel workers to open their own connections
        let repo_path = repo.workdir().unwrap_or_else(|| repo.path()).to_path_buf();

        let cache = RefCell::new(Cache::open(&repo)?);

        Ok(GitBackend {
            repo,
            repo_path,
            history_start,
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

            let results: Vec<Result<(CommitMetadata, Vec<Todo>)>> = unparsed_oids
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
                let (meta, todos) = result?;
                parsed_commits.push((meta, todos));
            }

            // Phase 4: Batch insert to cache
            eprintln!("Caching {} new commits", parsed_commits.len());
            self.cache.borrow_mut().insert_commits(&parsed_commits)?;
        }

        // Build todos with lifecycle data from cache
        self.build_todos_with_lifecycle()
    }

    fn get_history_start_commit(&self) -> Result<Option<Commit<'_>>> {
        if let Some(ref history_start) = self.history_start {
            match self.repo.revparse_single(history_start) {
                Ok(obj) => Ok(Some(obj.peel_to_commit().map_err(|e| {
                    Error::GitError(format!(
                        "history start `{}` is not a commit: {}",
                        history_start,
                        e.message()
                    ))
                })?)),
                Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(None),
                Err(e) => Err(Error::GitError(format!(
                    "failed to resolve history start `{}`: {}",
                    history_start,
                    e.message()
                ))),
            }
        } else {
            Ok(None)
        }
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

        if let Some(history_start_commit) = self.get_history_start_commit()? {
            println!(
                "Using history start commit `{}` ({}), timestamp {:?}",
                self.history_start.as_ref().unwrap(),
                history_start_commit.id(),
                history_start_commit.time()
            );
            for parent in history_start_commit.parents() {
                revwalk.hide(parent.id())?;
            }
        }

        revwalk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME | git2::Sort::REVERSE)?;

        let mut unparsed = Vec::new();
        for oid_result in revwalk {
            let oid = oid_result?;
            if !parsed.contains(&oid.to_string()) {
                unparsed.push(oid);
            }
        }

        println!("Found {} unparsed commits", unparsed.len());

        Ok(unparsed)
    }

    /// Parse a single commit - called in parallel in walk_commits_for_todos.
    fn parse_commit_standalone(
        repo: &Repository,
        parser: &TodoParser,
        oid: Oid,
    ) -> Result<(CommitMetadata, Vec<Todo>)> {
        let commit = repo.find_commit(oid)?;
        let meta = CommitMetadata::from(&commit);

        let commit_tree = commit.tree()?;
        let mut todos = Vec::new();

        commit_tree.walk(git2::TreeWalkMode::PreOrder, |parent_path, entry| {
            let file_name = entry.name().unwrap_or("");
            let file_path = format!("{}{}", parent_path, file_name);
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

        println!("Parsed commit {} with {} TODOs", meta.sha, todos.len());

        Ok((meta, todos))
    }

    /// Build the final Todos collection using lifecycle data from the cache.
    fn build_todos_with_lifecycle(&self) -> Result<Todos> {
        println!("Building final TODOs with lifecycle data from cache...");
        let head = self.repo.head()?;
        let head_commit = head.peel_to_commit()?;
        let head_sha = head_commit.id().to_string();

        let lifecycle_todos = self.cache.borrow().get_all_todo_lifecycles(&head_sha, &self.repo_path)?;
        let mut todos = Todos::from(Vec::new());

        for mut todo in lifecycle_todos {
            if let Some(TodoIdentifier::Primary(id)) = todo.id {
                // If the todo has a location (exists in HEAD), load its content
                if todo.location.file_path.is_some() {
                    if let Err(e) = todo.load(self.parser.todo_token()) {
                        eprintln!("Warning: Failed to load TODO #{}: {}", id, e);
                        todo.title = format!("[Failed to load TODO #{}]", id);
                    }
                } else {
                    // Todo was removed - use placeholder title
                    todo.title = format!("[Removed TODO #{}]", id);
                }
                todos.insert(id, todo);
            }
        }

        println!("Built {} TODOs with lifecycle data", todos.len());
        Ok(todos)
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

    fn tag_head(dir: &Path, name: &str) {
        Command::new("git")
            .args(["update-ref", &format!("refs/tags/{name}"), "HEAD"])
            .current_dir(dir)
            .output()
            .expect("failed to tag HEAD");
    }

    #[test]
    fn test_git_backend_not_a_repo() {
        let dir = TempDir::new().expect("failed to create temp dir");
        let result = GitBackend::new(dir.path(), "TODO", None);
        assert!(matches!(result, Err(Error::NotARepository)));
    }

    #[test]
    fn test_git_backend_empty_repo() {
        let (dir, _repo) = create_test_repo();
        let backend = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
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

        let backend = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
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

        let backend = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
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

        let backend = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
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

        let backend = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
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

        let backend = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        let todos = backend.walk_commits_for_todos().expect("failed to scan");

        assert_eq!(todos.len(), 1);
        let todo = todos.get(&42).expect("TODO #42 should exist");
        assert_eq!(todo.title, "Fix bug");
        assert_eq!(todo.priority, Some('A'));
        assert!(todo.tags.contains(&"urgent".to_string()));
    }

    #[test]
    fn test_git_backend_starts_from_history_ref() {
        let (dir, _repo) = create_test_repo();

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #1 Before adoption\nfn main() {}",
            "Add TODO before adoption",
        );

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #2 Start here\nfn main() {}",
            "Start using todoozy",
        );
        tag_head(dir.path(), "tdz_history_start");

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #2 Start here\n// TODO #3 After adoption\nfn main() {}",
            "Add TODO after adoption",
        );

        let backend = GitBackend::new(dir.path(), "TODO", Some("tdz_history_start".to_string()))
            .expect("failed to create backend");
        let todos = backend.walk_commits_for_todos().expect("failed to scan");

        assert!(
            todos.get(&1).is_none(),
            "pre-adoption TODO should be ignored"
        );
        assert!(
            todos.get(&2).is_some(),
            "history start commit should be included"
        );
        assert!(
            todos.get(&3).is_some(),
            "post-adoption TODO should be included"
        );
    }

    // TodoCache tests

    #[test]
    fn test_todo_cache_schema_creation() {
        let (dir, repo) = create_test_repo();
        let cache = Cache::open(&repo).expect("failed to open cache");

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
        let mut cache = Cache::open(&repo).expect("failed to open cache");

        // Create test commit metadata
        let meta = CommitMetadata {
            sha: "abc123".to_string(),
            timestamp: Utc::now(),
            author_name: "Test".to_string(),
            author_email: "test@test.com".to_string(),
        };

        // Insert commit with no todos
        cache
            .insert_commits(&[(meta, vec![])])
            .expect("failed to insert");

        let parsed = cache.get_parsed_commits().expect("failed to query");
        assert!(parsed.contains("abc123"));
    }

    // #[test]
    // fn test_todo_cache_insert_commit_with_todos() {
    //     let (_dir, repo) = create_test_repo();
    //     let mut cache = Cache::open(&repo).expect("failed to open cache");
    //
    //     let meta = CommitMetadata {
    //         sha: "def456".to_string(),
    //         timestamp: Utc::now(),
    //         author_name: "Test".to_string(),
    //         author_email: "test@test.com".to_string(),
    //     };
    //
    //     let mut todo = Todo::default();
    //     todo.id = Some(TodoIdentifier::Primary(42));
    //     todo.location.file_path = Some("test.rs".to_string());
    //     todo.location.start_line_num = 10;
    //     todo.location.end_line_num = 12;
    //
    //     cache
    //         .insert_commits(&[(meta, vec![todo])])
    //         .expect("failed to insert");
    //
    //     // Verify todo ID was recorded
    //     let ids = cache.get_all_todo_ids().expect("failed to query");
    //     assert!(ids.contains(&42));
    //
    //     // Verify location was recorded
    //     let locations = cache
    //         .get_todos_at_commit("def456")
    //         .expect("failed to query");
    //     assert_eq!(locations.len(), 1);
    //     assert_eq!(locations[0].todo_id, 42);
    //     assert_eq!(locations[0].file_path, "test.rs");
    //     assert_eq!(locations[0].start_line, 10);
    //     assert_eq!(locations[0].end_line, 12);
    // }

    #[test]
    fn test_todo_cache_lifecycle_active_todo() {
        let (dir, repo) = create_test_repo();
        let mut cache = Cache::open(&repo).expect("failed to open cache");

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
            .insert_commits(&[(meta, vec![todo])])
            .expect("failed to insert");

        // Query lifecycle - todo exists in "HEAD" (head123)
        let todo = cache
            .get_todo_lifecycle(1, "head123", dir.path())
            .expect("failed to query");

        assert!(todo.creation_date.is_some());
        assert!(todo.completion_date.is_none()); // Still active in HEAD
    }

    #[test]
    fn test_todo_cache_lifecycle_completed_todo() {
        let (dir, repo) = create_test_repo();
        let mut cache = Cache::open(&repo).expect("failed to open cache");

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
            .insert_commits(&[(meta1, vec![todo])])
            .expect("failed to insert");

        // Second commit: todo removed (empty list)
        let meta2 = CommitMetadata {
            sha: "head456".to_string(),
            timestamp: Utc.with_ymd_and_hms(2024, 1, 20, 12, 0, 0).unwrap(),
            author_name: "Test".to_string(),
            author_email: "test@test.com".to_string(),
        };

        cache
            .insert_commits(&[(meta2, vec![])])
            .expect("failed to insert");

        // Query lifecycle - todo NOT in HEAD (head456)
        let todo = cache
            .get_todo_lifecycle(1, "head456", dir.path())
            .expect("failed to query");

        assert!(todo.creation_date.is_some());
        assert!(todo.completion_date.is_some()); // Completed since not in HEAD
        assert_eq!(todo.creation_date.unwrap().to_string(), "2024-01-10");
        assert_eq!(todo.completion_date.unwrap().to_string(), "2024-01-10");
    }

    #[test]
    fn test_todo_cache_get_todo_lifecycles_batch() {
        let (dir, repo) = create_test_repo();
        let mut cache = Cache::open(&repo).expect("failed to open cache");

        let meta = CommitMetadata {
            sha: "head123".to_string(),
            timestamp: Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap(),
            author_name: "Test".to_string(),
            author_email: "test@test.com".to_string(),
        };

        let mut todo1 = Todo::default();
        todo1.id = Some(TodoIdentifier::Primary(1));
        todo1.location.file_path = Some("test.rs".to_string());

        let mut todo2 = Todo::default();
        todo2.id = Some(TodoIdentifier::Primary(2));
        todo2.location.file_path = Some("test.rs".to_string());

        cache
            .insert_commits(&[(meta, vec![todo1, todo2])])
            .expect("failed to insert");

        // Query batch
        let todos = cache
            .get_todo_lifecycles(&[1, 2], "head123", dir.path())
            .expect("failed to query");

        assert_eq!(todos.len(), 2);
        for todo in &todos {
            assert!(todo.creation_date.is_some());
            assert!(todo.completion_date.is_none());
        }

        // Empty batch should return empty vec
        let empty = cache
            .get_todo_lifecycles(&[], "head123", dir.path())
            .expect("failed to query");
        assert!(empty.is_empty());
    }

    #[test]
    fn test_todo_cache_get_all_todo_lifecycles() {
        let (dir, repo) = create_test_repo();
        let mut cache = Cache::open(&repo).expect("failed to open cache");

        // First commit: add two todos
        let meta1 = CommitMetadata {
            sha: "commit1".to_string(),
            timestamp: Utc.with_ymd_and_hms(2024, 1, 10, 12, 0, 0).unwrap(),
            author_name: "Test".to_string(),
            author_email: "test@test.com".to_string(),
        };

        let mut todo1 = Todo::default();
        todo1.id = Some(TodoIdentifier::Primary(1));
        todo1.location.file_path = Some("test.rs".to_string());

        let mut todo2 = Todo::default();
        todo2.id = Some(TodoIdentifier::Primary(2));
        todo2.location.file_path = Some("test.rs".to_string());

        cache
            .insert_commits(&[(meta1, vec![todo1, todo2])])
            .expect("failed to insert");

        // Second commit: remove todo 1, keep todo 2
        let meta2 = CommitMetadata {
            sha: "head456".to_string(),
            timestamp: Utc.with_ymd_and_hms(2024, 1, 20, 12, 0, 0).unwrap(),
            author_name: "Test".to_string(),
            author_email: "test@test.com".to_string(),
        };

        let mut todo2_still = Todo::default();
        todo2_still.id = Some(TodoIdentifier::Primary(2));
        todo2_still.location.file_path = Some("test.rs".to_string());

        cache
            .insert_commits(&[(meta2, vec![todo2_still])])
            .expect("failed to insert");

        // Get all lifecycles relative to head456
        let todos = cache
            .get_all_todo_lifecycles("head456", dir.path())
            .expect("failed to query");

        assert_eq!(todos.len(), 2);

        let todo1 = todos.iter().find(|t| t.id == Some(TodoIdentifier::Primary(1))).unwrap();
        let todo2 = todos.iter().find(|t| t.id == Some(TodoIdentifier::Primary(2))).unwrap();

        // Todo 1 was removed - should have completion_date
        assert!(todo1.creation_date.is_some());
        assert!(todo1.completion_date.is_some());

        // Todo 2 still exists - no completion_date
        assert!(todo2.creation_date.is_some());
        assert!(todo2.completion_date.is_none());
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
        let backend1 = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
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
        let backend2 = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
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
        let _cache = Cache::open(&repo).expect("failed to open cache");

        // Verify .git/todoozy/cache.db exists
        let expected_path = dir.path().join(".git/todoozy/cache.db");
        assert!(
            expected_path.exists(),
            "cache.db should be created at .git/todoozy/cache.db"
        );
    }
}
