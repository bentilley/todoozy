// Git backend for VCS TODO history extraction

use super::{
    error::{Error, Result},
    VcsBackend,
};
use crate::fs::{FileType, FileTypeAwarePath};
use crate::todo::{parser::TodoParser, Todo, TodoIdentifier, Todos};
use chrono::{DateTime, TimeZone, Utc};
use git2::{Commit, DiffOptions, Oid, Repository};
use itertools::Itertools;
use rayon::prelude::*;
use rusqlite::{params, Connection};
use std::cell::RefCell;
use std::collections::HashMap;
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

#[derive(Debug, Clone)]
enum Event {
    Add(Oid, String),
    Update(Oid, String),
    Remove(Oid, String),
}

struct Cache {
    conn: Connection,
}

impl Cache {
    fn get_cache_path(repo: &Repository) -> PathBuf {
        repo.commondir().join("todoozy").join("cache.db")
    }

    fn open(repo: &Repository) -> Result<Self> {
        let path = Self::get_cache_path(repo);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(&path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA busy_timeout=5000;",
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS git_commit (
                oid TEXT PRIMARY KEY
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS event (
                commit_oid TEXT NOT NULL REFERENCES git_commit(oid),
                todo_id INTEGER NOT NULL,
                event_type TEXT NOT NULL,
                event_oid TEXT NOT NULL,
                file_path TEXT NOT NULL
            )",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_event_commit_oid ON event(commit_oid)",
            [],
        )?;
        Ok(Self { conn })
    }

    /// Returns all cached commit OIDs as strings for fast lookup.
    fn get_cached_commit_oids(&self) -> Result<std::collections::HashSet<String>> {
        let mut stmt = self.conn.prepare("SELECT oid FROM git_commit")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut result = std::collections::HashSet::new();
        for row in rows {
            result.insert(row?);
        }
        Ok(result)
    }

    /// Load events for all cached commits (for merging with newly parsed data).
    fn get_all_cached_events(&self) -> Result<HashMap<u32, Vec<Event>>> {
        let mut stmt = self.conn.prepare(
            "SELECT todo_id, event_type, event_oid, file_path FROM event",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, u32>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;

        let mut result: HashMap<u32, Vec<Event>> = HashMap::new();
        for row in rows {
            let (todo_id, event_type, event_oid, file_path) = row?;
            let oid = Oid::from_str(&event_oid)
                .map_err(|e| Error::CacheError(format!("invalid oid in cache: {}", e)))?;
            let event = match event_type.as_str() {
                "Add" => Event::Add(oid, file_path),
                "Update" => Event::Update(oid, file_path),
                "Remove" => Event::Remove(oid, file_path),
                _ => continue,
            };
            result.entry(todo_id).or_default().push(event);
        }
        Ok(result)
    }

    /// Cache multiple commits and their events in a single transaction.
    fn cache_results(&mut self, results: &[(Oid, HashMap<u32, Vec<Event>>)]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut commit_stmt = tx.prepare("INSERT OR IGNORE INTO git_commit (oid) VALUES (?)")?;
            let mut event_stmt = tx.prepare(
                "INSERT INTO event (commit_oid, todo_id, event_type, event_oid, file_path)
                 VALUES (?, ?, ?, ?, ?)",
            )?;

            for (commit_oid, events) in results {
                let commit_oid_str = commit_oid.to_string();
                commit_stmt.execute([&commit_oid_str])?;

                for (todo_id, event_list) in events {
                    for event in event_list {
                        let (event_type, event_oid, file_path) = match event {
                            Event::Add(oid, path) => ("Add", oid.to_string(), path.clone()),
                            Event::Update(oid, path) => ("Update", oid.to_string(), path.clone()),
                            Event::Remove(oid, path) => ("Remove", oid.to_string(), path.clone()),
                        };
                        event_stmt.execute(params![
                            &commit_oid_str,
                            todo_id,
                            event_type,
                            &event_oid,
                            &file_path
                        ])?;
                    }
                }
            }
        }
        tx.commit()?;
        Ok(())
    }
}

/// Git-based VCS backend for extracting TODO lifecycle data.
pub struct GitBackend {
    repo: Repository,
    /// Optional commit-ish that limits how far back to scan. Commits before this are excluded.
    cutoff: Option<String>,
    parser: TodoParser,
    cache: RefCell<Cache>,
}

impl GitBackend {
    /// Create a new GitBackend for the repository at the given path.
    ///
    /// # Arguments
    /// * `path` - Path within the repository
    /// * `todo_token` - The token used to identify TODOs (e.g., "TODO")
    /// * `cutoff` - Optional commit-ish (tag, branch, SHA) that limits how far back to scan.
    ///   Commits before the cutoff are excluded. The cutoff commit itself is included.
    pub fn new(path: &Path, todo_token: &str, cutoff: Option<String>) -> Result<Self> {
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
            cutoff,
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

    /// Resolve the cutoff commit if one was specified.
    fn get_cutoff_commit(&self) -> Result<Option<Commit<'_>>> {
        if let Some(ref cutoff) = self.cutoff {
            match self.repo.revparse_single(cutoff) {
                Ok(obj) => Ok(Some(obj.peel_to_commit().map_err(|e| {
                    Error::GitError(format!(
                        "cutoff `{}` is not a commit: {}",
                        cutoff,
                        e.message()
                    ))
                })?)),
                Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(None),
                Err(e) => Err(Error::GitError(format!(
                    "failed to resolve cutoff `{}`: {}",
                    cutoff,
                    e.message()
                ))),
            }
        } else {
            Ok(None)
        }
    }

    fn parse_commit(
        repo: &Repository,
        parser: &TodoParser,
        oid: Oid,
        cutoff: Oid,
    ) -> Result<HashMap<u32, Vec<Event>>> {
        let commit = repo.find_commit(oid)?;

        // For root commits (no parents or cutoff commit), diff against empty tree
        let parents = if commit.parent_count() == 0 || oid == cutoff {
            vec![None]
        } else {
            commit.parents().map(Some).collect()
        };

        let mut events: HashMap<u32, Vec<Event>> = HashMap::new();

        for parent in parents {
            let parent_tree = parent.as_ref().map(|p| p.tree()).transpose()?;

            let mut line_changes: HashMap<u32, Event> = HashMap::new();

            let mut diff_opts = DiffOptions::new();
            diff_opts.skip_binary_check(true);
            for pattern in FileType::supported_pathspecs() {
                diff_opts.pathspec(pattern);
            }
            let diff = repo.diff_tree_to_tree(
                parent_tree.as_ref(),
                Some(&commit.tree()?),
                Some(&mut diff_opts),
            )?;

            diff.foreach(
                &mut |_file: git2::DiffDelta<'_>, _| true,
                None,
                None,
                Some(&mut |file: git2::DiffDelta<'_>, _, line: git2::DiffLine| {
                    let file_path = match line.origin() {
                        '+' => file.new_file().path(),
                        '-' => file.old_file().path(),
                        _ => return true,
                    };
                    let file_type = match file_path.and_then(|p| p.get_filetype_from_name()) {
                        Some(ft) => ft,
                        None => return true,
                    };
                    let file_path_name = match file_path.and_then(|p| p.to_str()) {
                        Some(name) => name,
                        None => return true,
                    }
                    .to_string();
                    let status = match line.origin() {
                        '+' => Event::Add(oid, file_path_name.clone()),
                        '-' => {
                            let parent_oid = parent.as_ref().unwrap().id();
                            Event::Remove(parent_oid, file_path_name.clone())
                        }
                        _ => return true,
                    };
                    let todo = match parser
                        .parse_bytes(line.content(), file_type)
                        .into_iter()
                        .exactly_one()
                    {
                        Ok(todo) => todo,
                        Err(_) => return true, // Skip
                    };
                    if let Some(TodoIdentifier::Primary(id)) = todo.id {
                        use Event::*;
                        match line_changes.get(&id) {
                            Some(existing) => match (existing, status) {
                                (Add(_, _), Remove(oid, file_path_name))
                                | (Remove(_, _), Add(oid, file_path_name)) => {
                                    // Add + Remove => Move
                                    line_changes.insert(id, Update(oid, file_path_name));
                                }
                                _ => eprintln!("Multiple Events for commit {:?}", &commit), // Same kind seen twice, keep existing
                            },
                            None => {
                                line_changes.insert(id, status);
                            }
                        }
                    }
                    true
                }),
            )?;

            for (id, event) in line_changes {
                events.entry(id).or_default().push(event);
            }
        }

        Ok(events)
    }

    fn revparse_todos(&self, for_commit: Oid) -> Result<Todos> {
        let mut revwalk = self.repo.revwalk()?;

        revwalk.push(for_commit).map_err(|e| {
            if e.code() == git2::ErrorCode::NotFound {
                return Error::GitError(format!("commit {} not found", for_commit));
            }
            Error::from(e)
        })?;

        let cutoff_oid = if let Some(cutoff_commit) = self.get_cutoff_commit()? {
            for parent in cutoff_commit.parents() {
                revwalk.hide(parent.id())?;
            }
            cutoff_commit.id()
        } else {
            Oid::zero() // Dummy OID that won't match any real commit
        };

        revwalk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME | git2::Sort::REVERSE)?;

        // Get cached commit OIDs
        let cached_commits = self.cache.borrow().get_cached_commit_oids().unwrap_or_default();

        // Filter out already-cached commits
        let oids_to_parse: Vec<Oid> = revwalk
            .into_iter()
            .filter_map(|oid_result| oid_result.ok())
            .filter(|oid| !cached_commits.contains(&oid.to_string()))
            .collect();

        let repo_path = &self.get_repo_path();
        let parser = &self.parser;

        // Parse uncached commits in parallel
        let results: Vec<(Oid, Result<HashMap<u32, Vec<Event>>>)> = oids_to_parse
            .par_iter()
            .map(|oid| {
                let thread_repo = Repository::open(repo_path)?;
                let events = Self::parse_commit(&thread_repo, parser, *oid, cutoff_oid)?;
                Ok((*oid, events))
            })
            .map(|r: Result<(Oid, HashMap<u32, Vec<Event>>)>| match r {
                Ok((oid, events)) => (oid, Ok(events)),
                Err(e) => (Oid::zero(), Err(e)),
            })
            .collect();

        // Aggregate newly parsed events
        let mut events: HashMap<u32, Vec<Event>> = HashMap::new();
        let mut to_cache: Vec<(Oid, HashMap<u32, Vec<Event>>)> = Vec::new();

        for (oid, result) in results {
            match result {
                Ok(commit_events) => {
                    to_cache.push((oid, commit_events.clone()));
                    for (id, evs) in commit_events {
                        events.entry(id).or_default().extend(evs);
                    }
                }
                Err(e) => eprintln!("Error parsing commit: {:?}", e),
            }
        }

        // Cache new results in a single transaction
        let _ = self.cache.borrow_mut().cache_results(&to_cache);

        // Load and merge cached events
        if let Ok(cached_events) = self.cache.borrow().get_all_cached_events() {
            for (id, evs) in cached_events {
                events.entry(id).or_default().extend(evs);
            }
        }

        let mut todos: HashMap<u32, Todo> = HashMap::new();

        for (id, events) in events.iter() {
            use Event::*;
            let created_datetime = match events.first() {
                Some(event) => match event {
                    Add(oid, _) => {
                        let commit = self.repo.find_commit(*oid)?;
                        Utc.timestamp_opt(commit.time().seconds(), 0)
                            .single()
                            .unwrap_or_else(Utc::now)
                    }
                    _ => unreachable!("First event must be an add.."),
                },
                None => continue,
            };
            let todo = match events.last() {
                Some(event) => match event {
                    Remove(oid, path) => {
                        let commit = self.repo.find_commit(*oid)?;
                        let file_blob = commit
                            .tree()?
                            .get_path(Path::new(path))?
                            .to_object(&self.repo)?
                            .peel_to_blob()?;
                        let mut t = match self
                            .parser
                            .parse_bytes(
                                file_blob.content(),
                                Path::new(path).get_filetype_from_name().unwrap(),
                            )
                            .into_iter()
                            .find(|todo| match todo.id {
                                Some(TodoIdentifier::Primary(todo_id)) => todo_id == *id,
                                _ => false,
                            }) {
                            Some(todo) => todo,
                            None => continue,
                        };
                        t.creation_date = Some(created_datetime.date_naive());
                        t.completion_date = Some(
                            Utc.timestamp_opt(commit.time().seconds(), 0)
                                .single()
                                .unwrap_or_else(Utc::now)
                                .date_naive(),
                        );
                        t.location.file_path = Some(path.clone().into());
                        t
                    }
                    Add(oid, path) | Update(oid, path) => {
                        let commit = self.repo.find_commit(*oid)?;
                        let file_blob = commit
                            .tree()?
                            .get_path(Path::new(path))?
                            .to_object(&self.repo)?
                            .peel_to_blob()?;
                        let mut t = match self
                            .parser
                            .parse_bytes(
                                file_blob.content(),
                                Path::new(path).get_filetype_from_name().unwrap(),
                            )
                            .into_iter()
                            .find(|todo| match todo.id {
                                Some(TodoIdentifier::Primary(todo_id)) => todo_id == *id,
                                _ => false,
                            }) {
                            Some(todo) => todo,
                            None => continue,
                        };
                        t.creation_date = Some(created_datetime.date_naive());
                        t.location.file_path = Some(path.clone().into());
                        t
                    }
                },
                None => continue,
            };
            todos.insert(*id, todo);
        }

        Ok(todos.into_values().collect::<Vec<_>>().into())
    }
}

impl VcsBackend for GitBackend {
    fn get_all_todos(&self) -> Result<Todos> {
        let head = self.repo.head()?.peel_to_commit()?.id();
        self.revparse_todos(head)
    }

    fn get_todos_for_version(&self, ids: &[u32], version: &str) -> Result<Todos> {
        let oid = self.repo.revparse_single(&version)?.id();
        let todos = self.revparse_todos(oid)?;
        Ok(todos
            .into_iter()
            .filter(|todo| match todo.id {
                Some(TodoIdentifier::Primary(id)) => ids.contains(&id),
                _ => false,
            })
            .collect::<Vec<_>>()
            .into())
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

    fn assert_path_ends_with(actual: &Option<PathBuf>, expected_suffix: &str) {
        let actual = actual.as_deref().expect("expected file path");
        assert!(
            actual.ends_with(expected_suffix),
            "expected path `{}` to end with `{expected_suffix}`",
            actual.display()
        );
    }

    #[test]
    fn test_git_backend_not_a_repo() {
        let dir = TempDir::new().expect("failed to create temp dir");
        let result = GitBackend::new(dir.path(), "TODO", None);
        assert!(matches!(result, Err(Error::NotARepository)));
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
        let todos = backend.get_all_todos().expect("failed to scan");

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
        let todos = backend.get_all_todos().expect("failed to scan");

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
        let todos = backend.get_all_todos().expect("failed to scan");

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
        let todos = backend.get_all_todos().expect("failed to scan");

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
        let todos = backend.get_all_todos().expect("failed to scan");

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
        tag_head(dir.path(), "tdz_cutoff");

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #2 Start here\n// TODO #3 After adoption\nfn main() {}",
            "Add TODO after adoption",
        );

        let backend = GitBackend::new(dir.path(), "TODO", Some("tdz_cutoff".to_string()))
            .expect("failed to create backend");
        let todos = backend.get_all_todos().expect("failed to scan");

        assert!(
            todos.get(&1).is_none(),
            "TODO before cutoff should be ignored"
        );
        assert!(todos.get(&2).is_some(), "cutoff commit should be included");
        assert!(
            todos.get(&3).is_some(),
            "TODO after cutoff should be included"
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
        let todos = backend.get_all_todos().expect("failed to scan");

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
        let todos = backend.get_all_todos().expect("failed to scan");

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
    fn test_git_backend_loads_removed_todo_from_last_seen_location() {
        let (dir, _repo) = create_test_repo();

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #22 Removed task\nfn main() {}",
            "Add TODO",
        );

        commit_file(dir.path(), "main.rs", "fn main() {}", "Remove TODO");

        let backend = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        let todos = backend.get_all_todos().expect("failed to scan");

        let todo = todos
            .get(&22)
            .expect("removed TODO should still be returned");
        // Removed TODOs now load successfully from their last seen location
        assert_eq!(todo.title, "Removed task");
        assert_eq!(todo.id, Some(TodoIdentifier::Primary(22)));
        assert!(todo.creation_date.is_some());
        assert!(todo.completion_date.is_some());
        assert_path_ends_with(&todo.location.file_path, "main.rs");
    }

    // ====== revparse_todos tests ======

    #[test]
    fn test_revparse_detects_todo_in_initial_commit() {
        let (dir, _repo) = create_test_repo();

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #1 First todo ever\nfn main() {}",
            "Initial commit with TODO",
        );

        let backend = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        let todos = backend.get_all_todos().expect("failed to scan");

        assert_eq!(todos.len(), 1, "should detect TODO in initial commit");
        let todo = todos.get(&1).expect("TODO #1 should exist");
        assert_eq!(todo.title, "First todo ever");
    }

    #[test]
    fn test_revparse_detects_todo_added_in_subsequent_commit() {
        let (dir, _repo) = create_test_repo();

        commit_file(dir.path(), "main.rs", "fn main() {}", "Initial commit");

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #1 Added later\nfn main() {}",
            "Add TODO",
        );

        let backend = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        let todos = backend.get_all_todos().expect("failed to scan");

        assert_eq!(todos.len(), 1);
        let todo = todos.get(&1).expect("TODO #1 should exist");
        assert_eq!(todo.title, "Added later");
        assert!(todo.creation_date.is_some());
    }

    #[test]
    fn test_revparse_detects_todo_removal() {
        let (dir, _repo) = create_test_repo();

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #1 Will be removed\nfn main() {}",
            "Add TODO",
        );

        commit_file(dir.path(), "main.rs", "fn main() {}", "Remove TODO");

        let backend = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        let todos = backend.get_all_todos().expect("failed to scan");

        assert_eq!(todos.len(), 1);
        let todo = todos.get(&1).expect("TODO #1 should exist");
        assert!(
            todo.completion_date.is_some(),
            "removed TODO should have completion_date"
        );
    }

    #[test]
    fn test_revparse_modified_todo_not_duplicated() {
        let (dir, _repo) = create_test_repo();

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #1 Original title\nfn main() {}",
            "Add TODO",
        );

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #1 (A) Modified title +urgent\nfn main() {}",
            "Modify TODO",
        );

        let backend = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        let todos = backend.get_all_todos().expect("failed to scan");

        // Should NOT have duplicate entries
        assert_eq!(todos.len(), 1, "modified TODO should not create duplicates");
        let todo = todos.get(&1).expect("TODO #1 should exist");
        // The latest version should be used
        assert_eq!(todo.title, "Modified title");
        assert!(todo.completion_date.is_none(), "should still be open");
    }

    #[test]
    fn test_revparse_sets_file_path_on_todo() {
        let (dir, _repo) = create_test_repo();

        commit_file(
            dir.path(),
            "src/lib.rs",
            "// TODO #1 Has location\nfn foo() {}",
            "Add TODO",
        );

        let backend = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        let todos = backend.get_all_todos().expect("failed to scan");

        let todo = todos.get(&1).expect("TODO #1 should exist");
        assert!(
            todo.location.file_path.is_some(),
            "TODO should have file_path set"
        );
        assert!(
            todo.location
                .file_path
                .as_deref()
                .unwrap()
                .ends_with("src/lib.rs"),
            "file_path should end with src/lib.rs"
        );
    }

    #[test]
    fn test_revparse_multiple_todos_in_single_commit() {
        let (dir, _repo) = create_test_repo();

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #1 First\n// TODO #2 Second\n// TODO #3 Third\nfn main() {}",
            "Add multiple TODOs",
        );

        let backend = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        let todos = backend.get_all_todos().expect("failed to scan");

        assert_eq!(todos.len(), 3);
        assert!(todos.get(&1).is_some());
        assert!(todos.get(&2).is_some());
        assert!(todos.get(&3).is_some());
    }

    #[test]
    fn test_revparse_todo_in_deleted_file() {
        let (dir, _repo) = create_test_repo();

        commit_file(
            dir.path(),
            "temp.rs",
            "// TODO #1 In temp file\nfn temp() {}",
            "Add temp file with TODO",
        );

        // Delete the file
        std::fs::remove_file(dir.path().join("temp.rs")).expect("failed to remove file");
        Command::new("git")
            .args(["add", "temp.rs"])
            .current_dir(dir.path())
            .output()
            .expect("failed to stage deletion");
        Command::new("git")
            .args(["commit", "-m", "Delete temp file"])
            .current_dir(dir.path())
            .output()
            .expect("failed to commit deletion");

        let backend = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        let todos = backend.get_all_todos().expect("failed to scan");

        assert_eq!(todos.len(), 1);
        let todo = todos.get(&1).expect("TODO #1 should exist");
        assert!(
            todo.completion_date.is_some(),
            "TODO in deleted file should be marked complete"
        );
    }

    #[test]
    fn test_revparse_todo_moved_between_files() {
        let (dir, _repo) = create_test_repo();

        commit_file(
            dir.path(),
            "old.rs",
            "// TODO #1 Moving todo\nfn old() {}",
            "Add TODO in old.rs",
        );

        commit_files(
            dir.path(),
            &[
                ("old.rs", "fn old() {}"),
                ("new.rs", "// TODO #1 Moving todo\nfn new() {}"),
            ],
            "Move TODO to new.rs",
        );

        let backend = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        let todos = backend.get_all_todos().expect("failed to scan");

        // Moving a TODO (same ID) should result in a single TODO, not a completion + new
        assert_eq!(todos.len(), 1, "moved TODO should not be duplicated");
        let todo = todos.get(&1).expect("TODO #1 should exist");
        assert!(
            todo.completion_date.is_none(),
            "moved TODO should not be marked complete"
        );
    }

    #[test]
    fn test_revparse_respects_cutoff() {
        let (dir, _repo) = create_test_repo();

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #1 Before cutoff\nfn main() {}",
            "Pre-cutoff commit",
        );

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #2 At cutoff\nfn main() {}",
            "Cutoff commit",
        );
        tag_head(dir.path(), "cutoff");

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #2 At cutoff\n// TODO #3 After cutoff\nfn main() {}",
            "Post-cutoff commit",
        );

        let backend = GitBackend::new(dir.path(), "TODO", Some("cutoff".to_string()))
            .expect("failed to create backend");
        let todos = backend.get_all_todos().expect("failed to scan");

        assert!(
            todos.get(&1).is_none(),
            "TODO from before cutoff should be excluded"
        );
        assert!(
            todos.get(&2).is_some(),
            "TODO from cutoff commit should be included"
        );
        assert!(
            todos.get(&3).is_some(),
            "TODO after cutoff should be included"
        );
    }
}
