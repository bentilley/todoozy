// Git backend for VCS TODO history extraction

use super::{
    error::{Error, Result},
    VcsBackend,
};
use crate::fs::FileTypeAwarePath;
use crate::todo::{parser::TodoParser, Location, Todo, TodoIdentifier, Todos};
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

type CacheRow = (u32, i64, i64, i32, Option<String>, Option<u32>, Option<u32>);

#[derive(Debug, Clone)]
struct CacheTodo {
    pub id: TodoIdentifier,
    pub completion_date: Option<DateTime<Utc>>,
    pub creation_date: DateTime<Utc>,
    pub location: Location,
}

impl CacheTodo {
    fn new(
        id: TodoIdentifier,
        creation_date: DateTime<Utc>,
        completion_date: Option<DateTime<Utc>>,
        location: Location,
    ) -> Self {
        Self {
            id,
            creation_date,
            completion_date,
            location,
        }
    }

    fn from_cache_row(row: CacheRow, repo_path: &Path) -> Self {
        let (todo_id, creation_ts, last_seen_ts, exists_in_sha, file_path, start_line, end_line) =
            row;

        let creation_date = Utc.timestamp_opt(creation_ts, 0).single().unwrap();

        let completion_date = if exists_in_sha == 0 {
            Utc.timestamp_opt(last_seen_ts, 0).single()
        } else {
            None
        };

        let abs_path = file_path.map(|p| repo_path.join(&p).to_string_lossy().into_owned());

        Self::new(
            TodoIdentifier::Primary(todo_id),
            creation_date,
            completion_date,
            Location::new(
                abs_path,
                start_line.unwrap_or(0) as usize,
                end_line.unwrap_or(0) as usize,
            ),
        )
    }

    /// Load TODO content from its source file location.
    ///
    /// This reads the file at `self.location.file_path`, extracts the lines
    /// from `start_line_num` to `end_line_num`, parses that text to get the
    /// TODO content, and updates this Todo's fields (title, priority, tags, etc.).
    ///
    /// Lifecycle data (creation_date, completion_date) is preserved.
    fn load(&self, parser: &TodoParser) -> Result<Todo> {
        let mut loaded = self.location.load(parser)?;
        loaded.creation_date = Some(self.creation_date.date_naive());
        loaded.completion_date = self.completion_date.map(|dt| dt.date_naive());
        loaded.location = self.location.clone();
        Ok(loaded)
    }
}

impl Into<Todo> for CacheTodo {
    fn into(self) -> Todo {
        let mut todo = Todo::default();
        todo.id = Some(self.id);
        todo.creation_date = Some(self.creation_date.date_naive());
        todo.completion_date = self.completion_date.map(|dt| dt.date_naive());
        todo.location = self.location;
        todo
    }
}

/// SQLite-based persistent cache for TODO history tracking.
struct Cache {
    repo_path: PathBuf,
    conn: Connection,
}

impl Cache {
    /// Open (or create) the cache database for the given repository.
    pub fn open(repo: &Repository) -> Result<Self> {
        let repo_path = repo.workdir().unwrap_or_else(|| repo.path()).to_path_buf();
        let db_path = Self::get_db_path(repo)?;
        let conn = Connection::open(&db_path)?;
        let cache = Cache { repo_path, conn };
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

    /// Get cache data for a specific TODO at a given SHA.
    pub fn get_todo(&self, todo_id: u32, sha: &str) -> Result<CacheTodo> {
        self.get_todos(&[todo_id], sha)?
            .into_iter()
            .next()
            .ok_or_else(|| Error::DataError(format!("TODO with ID {} not found in cache", todo_id)))
    }

    /// Get cache data for multiple TODOs at a given SHA.
    pub fn get_todos(&self, todo_ids: &[u32], sha: &str) -> Result<Vec<CacheTodo>> {
        if todo_ids.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders: Vec<String> =
            (0..todo_ids.len()).map(|i| format!("?{}", i + 1)).collect();
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
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i32>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<u32>>(5)?,
                row.get::<_, Option<u32>>(6)?,
            ))
        })?;

        let mut todos = Vec::new();
        for row in rows {
            todos.push(CacheTodo::from_cache_row(row?, &self.repo_path));
        }

        Ok(todos)
    }

    /// Get cache data for all TODOs that existed at a given SHA.
    pub fn get_all_todos(&self, sha: &str) -> Result<Vec<CacheTodo>> {
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
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i32>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<u32>>(5)?,
                row.get::<_, Option<u32>>(6)?,
            ))
        })?;

        let mut todos = Vec::new();
        for row in rows {
            todos.push(CacheTodo::from_cache_row(row?, &self.repo_path));
        }

        Ok(todos)
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

/// Git-based VCS backend for extracting TODO lifecycle data.
pub struct GitBackend {
    repo: Repository,
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

        let cache = RefCell::new(Cache::open(&repo)?);

        Ok(GitBackend {
            repo,
            history_start,
            parser: TodoParser::new(todo_token),
            cache,
        })
    }

    fn get_repo_path(&self) -> PathBuf {
        self.repo
            .workdir()
            .unwrap_or_else(|| self.repo.path())
            .to_path_buf()
    }

    /// Walk commits incrementally with parallel parsing, skipping already-cached commits.
    fn cache_todo_history(&self) -> Result<()> {
        // Phase 1: Collect unparsed commit OIDs (single-threaded revwalk)
        let unparsed_oids = self.collect_unparsed_oids()?;

        if !unparsed_oids.is_empty() {
            // Phase 2: Parse commits in parallel
            // Each worker opens its own Repository (git2 isn't Send)
            let repo_path = &self.get_repo_path();
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
        };

        Ok(())
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

    fn load_cache_todos(&self, cache_todos: &[CacheTodo]) -> Result<Todos> {
        Ok(cache_todos
            .iter()
            .map(|cache_todo| match cache_todo.load(&self.parser) {
                Ok(todo) => todo,
                Err(_) => {
                    let mut todo: Todo = cache_todo.clone().into();
                    todo.title = format!(
                        "[Failed to load TODO #{}]",
                        match todo.id {
                            Some(TodoIdentifier::Primary(id)) => id.to_string(),
                            _ => "unknown".to_string(),
                        }
                    );
                    todo
                }
            })
            .collect::<Vec<Todo>>()
            .into())
    }

    fn get_todo_for_oid(&self, id: u32, oid: Oid) -> Result<Todo> {
        let sha = oid.to_string();
        let cache_todo = self.cache.borrow().get_todo(id, &sha)?;
        self.load_cache_todos(&[cache_todo])
            .map(|todos| todos.into_iter().next().unwrap_or_default())
    }

    /// Build the final Todos collection using lifecycle data from the cache.
    fn get_todos_for_oid(&self, ids: &[u32], oid: Oid) -> Result<Todos> {
        let sha = oid.to_string();
        let cache_todos = self.cache.borrow().get_todos(ids, &sha)?;
        self.load_cache_todos(&cache_todos)
    }

    /// Build the final Todos collection using lifecycle data from the cache.
    fn get_all_todos_for_oid(&self, oid: Oid) -> Result<Todos> {
        let sha = oid.to_string();
        let lifecycle_todos = self.cache.borrow().get_all_todos(&sha)?;
        self.load_cache_todos(&lifecycle_todos)
    }
}

impl VcsBackend for GitBackend {
    fn get_most_recent_version(&self) -> Result<String> {
        let head = self.repo.head()?;
        let commit = head.peel_to_commit()?;
        Ok(commit.id().to_string())
    }

    fn get_all_todos(&self) -> Result<Todos> {
        self.cache_todo_history()?;
        self.get_all_todos_for_oid(self.repo.head()?.peel_to_commit()?.id())
    }

    fn get_todo_for_version(&self, id: u32, version: String) -> Result<Todo> {
        self.cache_todo_history()?;
        let oid = self.repo.revparse_single(&version)?.id();
        self.get_todo_for_oid(id, oid)
    }

    fn get_todos_for_version(&self, ids: &[u32], version: String) -> Result<Todos> {
        self.cache_todo_history()?;
        let oid = self.repo.revparse_single(&version)?.id();
        self.get_todos_for_oid(ids, oid)
    }

    fn get_all_todos_for_version(&self, version: String) -> Result<Todos> {
        self.cache_todo_history()?;
        let oid = self.repo.revparse_single(&version)?.id();
        self.get_all_todos_for_oid(oid)
    }

    fn get_all_ids(&self) -> Result<HashSet<u32>> {
        self.cache_todo_history()?;
        self.cache.borrow().get_all_todo_ids()
    }

    fn get_ids_for_version(&self, version: String) -> Result<HashSet<u32>> {
        self.cache_todo_history()?;
        let oid = self.repo.revparse_single(&version)?.id();
        let todos = self.get_all_todos_for_oid(oid)?;
        Ok(todos
            .iter()
            .filter_map(|todo| match todo.id {
                Some(TodoIdentifier::Primary(id)) => Some(id),
                _ => None,
            })
            .collect())
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
    fn commit_files(dir: &Path, files: &[(&str, &str)], message: &str) {
        for (filename, content) in files {
            let file_path = dir.join(filename);
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent).expect("failed to create parent directories");
            }
            fs::write(&file_path, content).expect("failed to write file");
        }

        let mut add = Command::new("git");
        add.arg("add");
        for (filename, _) in files {
            add.arg(filename);
        }
        add.current_dir(dir).output().expect("failed to add files");

        Command::new("git")
            .args(["commit", "-m", message])
            .current_dir(dir)
            .output()
            .expect("failed to commit");
    }

    fn commit_file(dir: &Path, filename: &str, content: &str, message: &str) {
        commit_files(dir, &[(filename, content)], message);
    }

    fn tag_head(dir: &Path, name: &str) {
        Command::new("git")
            .args(["update-ref", &format!("refs/tags/{name}"), "HEAD"])
            .current_dir(dir)
            .output()
            .expect("failed to tag HEAD");
    }

    fn assert_path_ends_with(actual: &Option<String>, expected_suffix: &str) {
        let actual = actual.as_deref().expect("expected file path");
        assert!(
            Path::new(actual).ends_with(expected_suffix),
            "expected path `{actual}` to end with `{expected_suffix}`"
        );
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
        let result = backend.cache_todo_history();
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
        let todos = backend.cache_todo_history().expect("failed to scan");

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
        let todos = backend.cache_todo_history().expect("failed to scan");

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
        let todos = backend.cache_todo_history().expect("failed to scan");

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
        let todos = backend.cache_todo_history().expect("failed to scan");

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
        let todos = backend.cache_todo_history().expect("failed to scan");

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
        let todos = backend.cache_todo_history().expect("failed to scan");

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

    #[test]
    fn test_git_backend_missing_history_ref_includes_full_history() {
        let (dir, _repo) = create_test_repo();

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #1 Before missing ref\nfn main() {}",
            "Add first TODO",
        );

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #1 Before missing ref\n// TODO #2 After missing ref\nfn main() {}",
            "Add second TODO",
        );

        let backend = GitBackend::new(dir.path(), "TODO", Some("does-not-exist".to_string()))
            .expect("failed to create backend");
        let todos = backend.cache_todo_history().expect("failed to scan");

        assert_eq!(todos.len(), 2);
        assert!(
            todos.get(&1).is_some(),
            "missing history ref should not hide old TODOs"
        );
        assert!(
            todos.get(&2).is_some(),
            "missing history ref should still include new TODOs"
        );
    }

    #[test]
    fn test_git_backend_rejects_non_commit_history_ref() {
        let (dir, repo) = create_test_repo();

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #1 Invalid history ref target\nfn main() {}",
            "Add TODO",
        );

        let tree_id = repo
            .head()
            .expect("HEAD should exist")
            .peel_to_commit()
            .expect("HEAD should be a commit")
            .tree_id();
        repo.reference("refs/tags/not-a-commit", tree_id, true, "test ref")
            .expect("failed to create tree ref");

        let backend = GitBackend::new(dir.path(), "TODO", Some("not-a-commit".to_string()))
            .expect("failed to create backend");
        match backend.cache_todo_history() {
            Err(Error::GitError(msg)) => {
                assert!(msg.contains("history start `not-a-commit` is not a commit"));
            }
            Err(other) => panic!("expected GitError for non-commit history ref, got {other}"),
            Ok(_) => panic!("tree history ref should fail"),
        }
    }

    #[test]
    fn test_git_backend_tracks_latest_todo_location() {
        let (dir, _repo) = create_test_repo();

        commit_file(
            dir.path(),
            "a.rs",
            "fn old_place() {}\n// TODO #7 Track moved todo\n",
            "Add TODO in original file",
        );

        commit_files(
            dir.path(),
            &[
                ("a.rs", "fn old_place() {}\n"),
                ("b.rs", "fn new_place() {}\n\n// TODO #7 Track moved todo\n"),
            ],
            "Move TODO to new file",
        );

        let backend = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        let todos = backend.cache_todo_history().expect("failed to scan");

        assert_eq!(todos.len(), 1);
        let todo = todos.get(&7).expect("TODO #7 should exist");
        assert!(Path::new(
            todo.location
                .file_path
                .as_deref()
                .expect("TODO should have a file path")
        )
        .ends_with("b.rs"));
        assert_eq!(todo.location.start_line_num, 3);
        assert_eq!(todo.location.end_line_num, 3);
        assert_eq!(todo.title, "Track moved todo");
        assert!(todo.completion_date.is_none());
    }

    #[test]
    fn test_git_backend_falls_back_when_removed_todo_cannot_be_loaded() {
        let (dir, _repo) = create_test_repo();

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #22 Removed task\nfn main() {}",
            "Add TODO",
        );

        commit_file(dir.path(), "main.rs", "fn main() {}", "Remove TODO");

        let backend = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        let todos = backend.cache_todo_history().expect("failed to scan");

        let todo = todos
            .get(&22)
            .expect("removed TODO should still be returned");
        assert_eq!(todo.title, "[Failed to load TODO #22]");
        assert_eq!(todo.id, Some(TodoIdentifier::Primary(22)));
        assert!(todo.creation_date.is_some());
        assert!(todo.completion_date.is_some());
        assert_eq!(todo.location.file_path, None);
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

    #[test]
    fn test_todo_cache_lifecycle_active_todo() {
        let (_dir, repo) = create_test_repo();
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
        todo.location.start_line_num = 10;
        todo.location.end_line_num = 12;

        cache
            .insert_commits(&[(meta, vec![todo])])
            .expect("failed to insert");

        // Query lifecycle - todo exists in "HEAD" (head123)
        let todo = cache.get_todo(1, "head123").expect("failed to query");

        assert_eq!(
            todo.creation_date,
            Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap()
        );
        assert!(todo.completion_date.is_none()); // Still active in HEAD
        assert_path_ends_with(&todo.location.file_path, "test.rs");
        assert_eq!(todo.location.start_line_num, 10);
        assert_eq!(todo.location.end_line_num, 12);
    }

    #[test]
    fn test_todo_cache_lifecycle_completed_todo() {
        let (_dir, repo) = create_test_repo();
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
        todo.location.file_path = Some("before.rs".to_string());
        todo.location.start_line_num = 4;
        todo.location.end_line_num = 6;

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
        let todo = cache.get_todo(1, "head456").expect("failed to query");

        assert_eq!(
            todo.creation_date,
            Utc.with_ymd_and_hms(2024, 1, 10, 12, 0, 0).unwrap()
        );
        assert!(todo.completion_date.is_some()); // Completed since not in HEAD
        assert_eq!(
            todo.completion_date.unwrap(),
            Utc.with_ymd_and_hms(2024, 1, 10, 12, 0, 0).unwrap()
        );
        assert_eq!(todo.location.file_path, None);
        assert_eq!(todo.location.start_line_num, 0);
        assert_eq!(todo.location.end_line_num, 0);
    }

    #[test]
    fn test_todo_cache_lifecycle_not_found() {
        let (_dir, repo) = create_test_repo();
        let cache = Cache::open(&repo).expect("failed to open cache");

        let err = cache
            .get_todo(999, "head123")
            .expect_err("missing todo should return an error");

        assert!(matches!(
            err,
            Error::DataError(ref msg) if msg == "TODO with ID 999 not found in cache"
        ));
    }

    #[test]
    fn test_todo_cache_get_todo_lifecycles_batch() {
        let (_dir, repo) = create_test_repo();
        let mut cache = Cache::open(&repo).expect("failed to open cache");

        let meta = CommitMetadata {
            sha: "head123".to_string(),
            timestamp: Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap(),
            author_name: "Test".to_string(),
            author_email: "test@test.com".to_string(),
        };

        let mut todo1 = Todo::default();
        todo1.id = Some(TodoIdentifier::Primary(1));
        todo1.location.file_path = Some("src/main.rs".to_string());
        todo1.location.start_line_num = 10;
        todo1.location.end_line_num = 12;

        let mut todo2 = Todo::default();
        todo2.id = Some(TodoIdentifier::Primary(2));
        todo2.location.file_path = Some("src/lib.rs".to_string());
        todo2.location.start_line_num = 20;
        todo2.location.end_line_num = 25;

        cache
            .insert_commits(&[(meta, vec![todo1, todo2])])
            .expect("failed to insert");

        // Query batch
        let todos = cache
            .get_todos(&[1, 2], "head123")
            .expect("failed to query");

        assert_eq!(todos.len(), 2);
        let todo1 = todos
            .iter()
            .find(|todo| todo.id == TodoIdentifier::Primary(1))
            .expect("todo 1 should be returned");
        assert_eq!(
            todo1.creation_date,
            Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap()
        );
        assert!(todo1.completion_date.is_none());
        assert_path_ends_with(&todo1.location.file_path, "src/main.rs");
        assert_eq!(todo1.location.start_line_num, 10);
        assert_eq!(todo1.location.end_line_num, 12);

        let todo2 = todos
            .iter()
            .find(|todo| todo.id == TodoIdentifier::Primary(2))
            .expect("todo 2 should be returned");
        assert_eq!(
            todo2.creation_date,
            Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap()
        );
        assert!(todo2.completion_date.is_none());
        assert_path_ends_with(&todo2.location.file_path, "src/lib.rs");
        assert_eq!(todo2.location.start_line_num, 20);
        assert_eq!(todo2.location.end_line_num, 25);

        // Empty batch should return empty vec
        let empty = cache.get_todos(&[], "head123").expect("failed to query");
        assert!(empty.is_empty());
    }

    #[test]
    fn test_todo_cache_get_all_todo_lifecycles() {
        let (_dir, repo) = create_test_repo();
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
        todo1.location.file_path = Some("src/old.rs".to_string());
        todo1.location.start_line_num = 3;
        todo1.location.end_line_num = 5;

        let mut todo2 = Todo::default();
        todo2.id = Some(TodoIdentifier::Primary(2));
        todo2.location.file_path = Some("src/todo.rs".to_string());
        todo2.location.start_line_num = 8;
        todo2.location.end_line_num = 9;

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
        todo2_still.location.file_path = Some("src/todo.rs".to_string());
        todo2_still.location.start_line_num = 30;
        todo2_still.location.end_line_num = 35;

        cache
            .insert_commits(&[(meta2, vec![todo2_still])])
            .expect("failed to insert");

        // Get all lifecycles relative to head456
        let todos = cache.get_all_todos("head456").expect("failed to query");

        assert_eq!(todos.len(), 2);

        let todo1 = todos
            .iter()
            .find(|t| t.id == TodoIdentifier::Primary(1))
            .unwrap();
        let todo2 = todos
            .iter()
            .find(|t| t.id == TodoIdentifier::Primary(2))
            .unwrap();

        // Todo 1 was removed - should have completion_date
        assert_eq!(
            todo1.creation_date,
            Utc.with_ymd_and_hms(2024, 1, 10, 12, 0, 0).unwrap()
        );
        assert!(todo1.completion_date.is_some());
        assert_eq!(todo1.location.file_path, None);
        assert_eq!(todo1.location.start_line_num, 0);
        assert_eq!(todo1.location.end_line_num, 0);

        // Todo 2 still exists - no completion_date
        assert_eq!(
            todo2.creation_date,
            Utc.with_ymd_and_hms(2024, 1, 10, 12, 0, 0).unwrap()
        );
        assert!(todo2.completion_date.is_none());
        assert_path_ends_with(&todo2.location.file_path, "src/todo.rs");
        assert_eq!(todo2.location.start_line_num, 30);
        assert_eq!(todo2.location.end_line_num, 35);
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
        let todos1 = backend1.cache_todo_history().expect("failed to scan");
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
        let todos2 = backend2.cache_todo_history().expect("failed to scan");
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
    fn test_todo_cache_rerun_without_new_commits_reuses_cache() {
        let (dir, _repo) = create_test_repo();

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #100 Cached task\nfn main() {}",
            "Initial commit",
        );

        let backend1 = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        let todos1 = backend1.cache_todo_history().expect("failed to scan");
        let parsed1 = backend1
            .cache
            .borrow()
            .get_parsed_commits()
            .expect("failed to query");

        let backend2 = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        let todos2 = backend2.cache_todo_history().expect("failed to scan");
        let parsed2 = backend2
            .cache
            .borrow()
            .get_parsed_commits()
            .expect("failed to query");

        assert_eq!(todos1.len(), 1);
        assert_eq!(todos2.len(), 1);
        assert_eq!(
            todos2.get(&100).map(|todo| todo.title.as_str()),
            Some("Cached task")
        );
        assert_eq!(parsed2.len(), parsed1.len());
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
