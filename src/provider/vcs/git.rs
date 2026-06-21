// Git backend for VCS TODO history extraction

use super::{
    error::{Error, Result},
    VcsBackend,
};
use crate::fs::{FileType, FileTypeAwarePath};
use crate::todo::{parser::TodoParser, Todo, TodoIdentifier, Todos};
use chrono::{DateTime, TimeZone, Utc};
use git2::{ApplyLocation, BlameOptions, Commit, Diff, DiffOptions, Oid, Repository};
use itertools::Itertools;
use rayon::prelude::*;
use std::collections::HashMap;
use std::io::Write;
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

/// Git-based VCS backend for extracting TODO lifecycle data.
pub struct GitBackend {
    repo: Repository,
    /// Optional commit-ish that limits how far back to scan. Commits before this are excluded.
    cutoff: Option<String>,
    parser: TodoParser,
    // cache: RefCell<Cache>,
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

        // let cache = RefCell::new(Cache::open(&repo)?);

        Ok(GitBackend {
            repo,
            cutoff,
            parser: TodoParser::new(todo_token),
            // cache,
        })
    }

    fn get_repo_path(&self) -> PathBuf {
        self.repo
            .workdir()
            .unwrap_or_else(|| self.repo.path())
            .to_path_buf()
    }

    /// Resolve `path` to a path relative to the repository root, suitable for
    /// use as a git2 pathspec or `Index::add_path` argument.
    ///
    /// Canonicalizes `path` first so that relative paths (e.g. `./src/lib.rs`
    /// from `Walk`) are resolved to absolute before stripping the repo-root
    /// prefix, otherwise git2 pathspec matching fails. If the canonicalized
    /// path does not live under the repo root, it is returned unchanged.
    fn to_repo_relative_path(&self, path: &Path) -> Result<PathBuf> {
        let repo_root = self.get_repo_path();
        let abs_path = path
            .canonicalize()
            .map_err(|e| Error::Custom(e.to_string()))?;
        Ok(match abs_path.strip_prefix(&repo_root) {
            Ok(rel) => rel.to_path_buf(),
            Err(_) => abs_path,
        })
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
        // revwalk.push_head().map_err(|e| {
        //     if e.code() == git2::ErrorCode::UnbornBranch {
        //         return Error::GitError("repository has no commits".to_string());
        //     }
        //     Error::from(e)
        // })?;

        if let Some(cutoff_commit) = self.get_cutoff_commit()? {
            for parent in cutoff_commit.parents() {
                revwalk.hide(parent.id())?;
            }
        }
        let cutoff_oid = if let Some(cutoff_commit) = self.get_cutoff_commit()? {
            cutoff_commit.id()
        } else {
            Oid::zero() // Dummy OID that won't match any real commit
        };

        revwalk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME | git2::Sort::REVERSE)?;

        let mut todos: HashMap<u32, Todo> = HashMap::new();

        let repo_path = &self.get_repo_path();
        let parser = &self.parser;

        let oids: Vec<Oid> = revwalk
            .into_iter()
            .filter_map(|oid_result| oid_result.ok())
            .collect();

        let results: Vec<Result<HashMap<u32, Vec<Event>>>> = oids
            .par_iter()
            .map(|oid| {
                let thread_repo = Repository::open(repo_path)?;
                Self::parse_commit(&thread_repo, parser, *oid, cutoff_oid)
            })
            .collect();

        let mut events: HashMap<u32, Vec<Event>> = HashMap::new();

        for result in results {
            match result {
                Ok(commit_events) => {
                    for (id, evs) in commit_events {
                        events.entry(id).or_default().extend(evs);
                    }
                }
                Err(e) => eprintln!("Error parsing commit: {:?}", e),
            }
        }

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

    /// Build a synthetic unified-diff patch that stages exactly the TODO comment.
    ///
    /// Diff spans `[start_line, end_line]`. All other changed lines are ignored.
    fn build_single_todo_patch(
        &self,
        diff: &Diff<'_>,
        start_line: u32,
        end_line: u32,
    ) -> Result<Vec<u8>> {
        let mut patch: Vec<u8> = Vec::new();
        let mut lines: Vec<u8> = Vec::new();

        let mut num_additions: u32 = 0;
        let mut num_deletions: u32 = 0;
        let mut old_insertion_pos: u32 = end_line;

        diff.foreach(
            &mut |delta, _| {
                let index_file = delta.old_file();
                let workdir_file = delta.new_file();

                let index_path = match index_file.path() {
                    Some(p) => p.display().to_string(),
                    None => return false,
                };
                let workdir_path = match workdir_file.path() {
                    Some(p) => p.display().to_string(),
                    None => return false,
                };

                let index_oid = index_file.id().to_string();
                let workdir_oid = workdir_file.id().to_string();
                let mode = u32::from(workdir_file.mode());

                write!(
                    &mut patch,
                    "diff --git i/{} w/{}\nindex {}..{} {:o}\n--- i/{}\n+++ w/{}\n",
                    index_path,
                    workdir_path,
                    &index_oid[..7],
                    &workdir_oid[..7],
                    mode,
                    index_path,
                    workdir_path,
                )
                .unwrap();

                true
            },
            None,
            None,
            Some(&mut |_delta, _hunk, line| {
                let in_todo = |n| n >= start_line && n <= end_line;
                let in_todo_context = |n| n >= start_line - 1 && n <= end_line + 1;

                use git2::DiffLineType::*;
                match line.origin_value() {
                    Addition if line.new_lineno().map_or(false, in_todo) => {
                        num_additions += 1;
                        lines.push(b'+');
                        lines.extend_from_slice(line.content());
                    }
                    Deletion if line.old_lineno().map_or(false, in_todo) => {
                        if line.old_lineno().unwrap() < old_insertion_pos {
                            old_insertion_pos = line.old_lineno().unwrap();
                        }
                        num_deletions += 1;
                        lines.push(b'-');
                        lines.extend_from_slice(line.content());
                    }
                    Context => {
                        if !line.new_lineno().map_or(false, in_todo_context)
                            && !line.old_lineno().map_or(false, in_todo_context)
                        {
                            return true;
                        }

                        if line.old_lineno().unwrap() < old_insertion_pos {
                            old_insertion_pos = line.old_lineno().unwrap();
                        }
                        num_additions += 1;
                        num_deletions += 1;
                        lines.push(b' ');
                        lines.extend_from_slice(line.content());
                    }
                    _ => return true,
                }

                true
            }),
        )?;

        if num_additions == 0 {
            return Err(Error::Custom(format!(
                "TODO at lines {start_line}\u{2013}{end_line} not found in unstaged diff"
            )));
        }

        // This is always a single-hunk patch applied to the index, so the
        // new-file start matches the old-file start: nothing before this
        // hunk shifts line numbers.
        write!(
            &mut patch,
            "@@ -{0},{1} +{0},{2} @@\n",
            old_insertion_pos, num_deletions, num_additions,
        )
        .unwrap();
        patch.extend(lines);

        Ok(patch)
    }

    /// Parse all TODOs from `rel_path` as it exists in `commit`.
    ///
    /// Returns an empty `Vec` if the file does not exist in `commit` (e.g. it
    /// was added later or already deleted at this point in history).
    fn todos_in_commit_file(&self, commit: &Commit<'_>, rel_path: &Path) -> Result<Vec<Todo>> {
        let entry = match commit.tree()?.get_path(rel_path) {
            Ok(entry) => entry,
            Err(_) => return Ok(Vec::new()),
        };
        let blob = entry.to_object(&self.repo)?.peel_to_blob()?;
        let file_type = rel_path.get_filetype_from_name().ok_or_else(|| {
            Error::Custom(format!("unsupported file type: {}", rel_path.display()))
        })?;
        Ok(self.parser.parse_bytes(blob.content(), file_type))
    }

    fn get_history_for_todo(&self, todo: &Todo) -> Result<Vec<(CommitMetadata, Todo)>> {
        // 1. get the file path for the todo
        let file_path = todo
            .location
            .file_path
            .as_ref()
            .ok_or_else(|| Error::Custom("todo has no file path".to_string()))?;
        // `file_path` on todos from `get_all_todos`/`revparse_todos` is already
        // repo-relative (and may no longer exist on disk for removed todos), so
        // only resolve it when it can be canonicalized; otherwise use it as-is.
        let rel_path = self
            .to_repo_relative_path(file_path)
            .unwrap_or_else(|_| file_path.clone());

        let todo_id = todo.id.clone();

        // 2. get the git blame for that file path
        let blame = self.repo.blame_file(&rel_path, None)?;
        // 3. get the blame hunk for the start line of the todo
        let hunk = blame
            .get_line(todo.location.start_line_num)
            .ok_or_else(|| {
                Error::Custom(format!(
                    "no blame info for {}:{}",
                    rel_path.display(),
                    todo.location.start_line_num
                ))
            })?;
        // 4. get the commit for that hunk (target commit)
        let mut commit = self.repo.find_commit(hunk.final_commit_id())?;

        // The hunk's path is the path of the file as it existed in `commit`,
        // which may differ from `rel_path` (HEAD's path) if the file was
        // renamed at some point between `commit` and HEAD.
        let mut tracking_path = hunk.path().map(Path::to_path_buf).unwrap_or(rel_path);

        // 5. get the blob for the target commit and file path and parse all todos from that blob
        //
        // The hunk covers a contiguous run of lines starting at
        // `hunk.final_start_line()` (HEAD's numbering) / `hunk.orig_start_line()`
        // (the target commit's own numbering). Translate the todo's HEAD line
        // number to the target commit's numbering by applying the same offset
        // from the start of the hunk.
        let target_line =
            hunk.orig_start_line() + (todo.location.start_line_num - hunk.final_start_line());
        let todos = self.todos_in_commit_file(&commit, &tracking_path)?;
        let mut current = todos
            .into_iter()
            .find(|t| t.location.start_line_num == target_line)
            .ok_or_else(|| Error::Custom("todo not found in blamed commit".to_string()))?;
        current.location.file_path = Some(tracking_path.clone());

        let mut history = Vec::new();

        loop {
            history.push((CommitMetadata::from(&commit), current.clone()));

            // 6. For each parent, locate this todo (by its stable ID) in the
            //    parent's version of the file, then re-blame the file with
            //    that parent set as the *newest* commit to consider. The
            //    resulting hunk for the todo's line tells us the commit that
            //    actually last touched it before `commit` - which may be the
            //    parent itself, or an earlier ancestor if the parent simply
            //    inherited the line unchanged (e.g. unrelated edits elsewhere
            //    in the file, or merge commits).
            let mut next: Option<(Commit<'_>, PathBuf, usize)> = None;

            for parent in commit.parents() {
                let parent_todos = self.todos_in_commit_file(&parent, &tracking_path)?;
                let parent_todo = match parent_todos.iter().find(|t| {
                    t.id == todo_id
                        || t.title == current.title
                        || t.location.start_line_num == current.location.start_line_num
                }) {
                    Some(t) => t,
                    None => continue, // todo doesn't exist on this parent's side
                };

                let mut opts = BlameOptions::new();
                opts.newest_commit(parent.id());
                let parent_blame = self.repo.blame_file(&tracking_path, Some(&mut opts))?;
                let parent_hunk = parent_blame
                    .get_line(parent_todo.location.start_line_num)
                    .ok_or_else(|| {
                        Error::Custom(format!(
                            "no blame info for {}:{}",
                            tracking_path.display(),
                            parent_todo.location.start_line_num
                        ))
                    })?;

                let next_commit = self.repo.find_commit(parent_hunk.final_commit_id())?;
                let next_path = parent_hunk
                    .path()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| tracking_path.clone());
                let next_line = parent_hunk.orig_start_line();

                next = Some((next_commit, next_path, next_line));
                break;
            }

            // 7. If no parent has this todo, `commit` is the one that created
            //    it and we're done. Otherwise go back to 5. with the commit
            //    and line found above as the new target.
            match next {
                Some((next_commit, next_path, next_line)) => {
                    let next_todos = self.todos_in_commit_file(&next_commit, &next_path)?;
                    let mut next_todo = next_todos
                        .into_iter()
                        .find(|t| t.location.start_line_num == next_line)
                        .ok_or_else(|| {
                            Error::Custom("todo not found in blamed commit".to_string())
                        })?;
                    next_todo.location.file_path = Some(next_path.clone());

                    commit = next_commit;
                    tracking_path = next_path;
                    current = next_todo;
                }
                None => break,
            }
        }

        // `history` was built newest-first (target commit -> creation commit);
        // reverse so callers see chronological order (creation first).
        history.reverse();
        Ok(history)
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

    fn trace_todo(&self, todo: &Todo) -> Result<Vec<(CommitMetadata, Todo)>> {
        self.get_history_for_todo(todo)
    }

    fn stage_todo(&mut self, todo: &mut Todo) -> Result<()> {
        let file_path = todo
            .location
            .file_path
            .as_ref()
            .ok_or("no file path on todo")?;

        let diff_path = self.to_repo_relative_path(file_path)?;

        let mut diff_opts = DiffOptions::new();
        diff_opts.pathspec(diff_path);
        let workdir_diff = self
            .repo
            .diff_index_to_workdir(None, Some(&mut diff_opts))?;

        let patch = self.build_single_todo_patch(
            &workdir_diff,
            todo.location.start_line_num as u32,
            todo.location.end_line_num as u32,
        )?;
        let synthetic_diff = Diff::from_buffer(&patch)?;
        self.repo
            .apply(&synthetic_diff, ApplyLocation::Index, None)?;

        Ok(())
    }

    fn stage_file(&mut self, path: &Path) -> Result<()> {
        let rel_path = self.to_repo_relative_path(path)?;

        let mut index = self.repo.index()?;
        index.add_path(&rel_path)?;
        index.write()?;

        Ok(())
    }

    fn commit(&mut self, message: &str) -> Result<()> {
        let mut index = self.repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = self.repo.find_tree(tree_id)?;
        let sig = self.repo.signature()?;
        let parent = self.repo.head()?.peel_to_commit()?;
        self.repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    use crate::provider::FileSystemProvider;

    /// Helper to create a test git repository.
    fn create_test_repo() -> TempDir {
        let dir = TempDir::new().expect("failed to create temp dir");

        let repo = Repository::init(dir.path()).expect("failed to init repo");
        let mut config = repo.config().expect("failed to get config");
        config
            .set_str("user.email", "test@example.com")
            .expect("failed email");
        config
            .set_str("user.name", "Test User")
            .expect("failed name");
        config
            .set_bool("commit.gpgsign", false)
            .expect("failed gpgsign");

        dir
    }

    fn create_local_remote() -> TempDir {
        let dir = TempDir::new().expect("failed to create remote dir");
        Repository::init_bare(&dir).expect("failed to init remote");
        dir
    }

    fn create_test_repo_with_remote() -> (TempDir, TempDir) {
        let dir = create_test_repo();
        let repo = Repository::open(&dir).expect("fail to open repo");
        let remote_dir = create_local_remote();

        let remote_url = format!("file://{}", remote_dir.path().display());
        repo.remote("origin", &remote_url)
            .expect("failed to add remote");

        (dir, remote_dir)
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

        let repo = Repository::open(dir).expect("failed to open repo");
        let mut index = repo.index().expect("failed to get index");
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .expect("failed to add files to index");
        index.write().expect("failed to write index");

        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = repo.signature().unwrap();
        let parents: Vec<git2::Commit> = match repo.head() {
            Ok(head) => vec![head.peel_to_commit().unwrap()],
            Err(_) => vec![],
        };
        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, &message, &tree, &parent_refs)
            .unwrap();
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

    /// Returns the SHA of the current HEAD commit.
    fn head_sha(dir: &Path) -> String {
        let repo = Repository::open(dir).expect("failed to open repo");
        let sha = repo
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string();
        sha
    }

    /// Returns the content of a file as it exists in the HEAD commit tree.
    fn head_file_content(dir: &Path, filename: &str) -> String {
        let repo = Repository::open(dir).expect("failed to open repo");
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let entry = head.tree().unwrap().get_path(Path::new(filename)).unwrap();
        let blob = entry.to_object(&repo).unwrap().peel_to_blob().unwrap();
        String::from_utf8_lossy(blob.content()).to_string()
    }

    /// Returns the lines added (+) and removed (-) by the HEAD commit vs its parent.
    fn head_commit_diff(dir: &Path) -> (Vec<String>, Vec<String>) {
        let repo = Repository::open(dir).expect("failed to open repo");
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let parent = head.parent(0).unwrap();
        let diff = repo
            .diff_tree_to_tree(
                Some(&parent.tree().unwrap()),
                Some(&head.tree().unwrap()),
                None,
            )
            .unwrap();
        let mut added = Vec::new();
        let mut removed = Vec::new();
        diff.foreach(
            &mut |_, _| true,
            None,
            None,
            Some(&mut |_, _, line| {
                let s = String::from_utf8_lossy(line.content())
                    .trim_end()
                    .to_string();
                match line.origin() {
                    '+' => added.push(s),
                    '-' => removed.push(s),
                    _ => {}
                }
                true
            }),
        )
        .unwrap();
        (added, removed)
    }

    // ====== stage_todo / stage_file / commit tests ======

    #[test]
    fn test_add_todo_single_line_stages_only_the_todo() {
        let (dir, _remote_dir) = create_test_repo_with_remote();
        commit_file(dir.path(), "main.rs", "fn main() {}\n", "Initial");

        // Working tree: TODO (no ID yet) added at line 1, plus an unrelated extra line.
        fs::write(
            dir.path().join("main.rs"),
            "// TODO #1 Fix bug\nfn main() {}\nfn extra() {}\n",
        )
        .unwrap();

        let fsp = FileSystemProvider::new("TODO", Vec::new());
        let mut todos = fsp
            .parse_file(&dir.path().join("main.rs"))
            .expect("failed to parse file");
        let mut todo = &mut todos[0];
        let mut backend =
            GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        backend.stage_todo(&mut todo).expect("stage_todo failed");
        backend
            .commit(&format!("chore: add todo {}", todo.display_id()))
            .expect("commit failed");

        let (added, removed) = head_commit_diff(dir.path());
        assert_eq!(
            added,
            vec!["// TODO #1 Fix bug"],
            "only the TODO line should be added"
        );
        assert!(removed.is_empty(), "nothing should be removed");
    }

    #[test]
    fn test_add_todo_multiline_stages_entire_comment_block() {
        let (dir, _remote_dir) = create_test_repo_with_remote();
        commit_file(dir.path(), "main.rs", "fn main() {}\n", "Initial");

        // Working tree: a three-line TODO comment (no ID) at lines 1–3.
        fs::write(
            dir.path().join("main.rs"),
            "// TODO #1 Fix bug\n// Detail line 1\n// Detail line 2\nfn main() {}\n",
        )
        .unwrap();

        let fsp = FileSystemProvider::new("TODO", Vec::new());
        let todos = fsp
            .parse_file(&dir.path().join("main.rs"))
            .expect("failed to parse file");
        let mut todo = todos.into_iter().next().expect("should have a todo");
        let mut backend =
            GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        backend.stage_todo(&mut todo).expect("stage_todo failed");
        backend
            .commit(&format!("chore: add todo {}", todo.display_id()))
            .expect("commit failed");

        let (added, removed) = head_commit_diff(dir.path());
        assert_eq!(
            added,
            vec!["// TODO #1 Fix bug", "// Detail line 1", "// Detail line 2"],
            "all three comment lines should be staged"
        );
        assert!(removed.is_empty());
    }

    #[test]
    fn test_add_todo_modification_stages_only_the_id_change() {
        let (dir, _remote_dir) = create_test_repo_with_remote();
        commit_file(
            dir.path(),
            "main.rs",
            "// TODO Fix bug\nfn main() {}\n",
            "Add TODO without ID",
        );

        // Working tree: unrelated extra line appended; ID will be inserted by stage_todo.
        fs::write(
            dir.path().join("main.rs"),
            "// TODO #1 Fix bug\nfn main() {}\nfn extra() {}\n",
        )
        .unwrap();

        let fsp = FileSystemProvider::new("TODO", Vec::new());
        let todos = fsp
            .parse_file(&dir.path().join("main.rs"))
            .expect("failed to parse file");
        let mut todo = todos.into_iter().next().expect("should have a todo");
        let mut backend =
            GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        backend.stage_todo(&mut todo).expect("stage_todo failed");
        backend
            .commit(&format!("chore: add todo {}", todo.display_id()))
            .expect("commit failed");

        let (added, removed) = head_commit_diff(dir.path());
        assert_eq!(
            added,
            vec!["// TODO #1 Fix bug"],
            "only the new first line should be added"
        );
        assert_eq!(
            removed,
            vec!["// TODO Fix bug"],
            "only the old first line should be removed"
        );
    }

    #[test]
    fn test_add_todo_with_preceding_unstaged_additions() {
        let (dir, _remote_dir) = create_test_repo_with_remote();
        commit_file(
            dir.path(),
            "main.rs",
            "fn a() {}\nfn b() {}\nfn c() {}\n",
            "Initial",
        );

        // Unrelated fn new() inserted at line 2; TODO (no ID) at line 4.
        fs::write(
            dir.path().join("main.rs"),
            "fn a() {}\nfn new() {}\nfn b() {}\n// TODO #1 Fix bug\nfn c() {}\n",
        )
        .unwrap();

        let fsp = FileSystemProvider::new("TODO", Vec::new());
        let todos = fsp
            .parse_file(&dir.path().join("main.rs"))
            .expect("failed to parse file");
        let mut todo = todos.into_iter().next().expect("should have a todo");
        let mut backend =
            GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        backend.stage_todo(&mut todo).expect("stage_todo failed");
        backend
            .commit(&format!("chore: add todo {}", todo.display_id()))
            .expect("commit failed");

        let content = head_file_content(dir.path(), "main.rs");
        let todo_pos = content
            .find("// TODO #1 Fix bug")
            .expect("TODO should be present in the committed file");
        let c_pos = content
            .find("fn c() {}")
            .expect("fn c should be present in the committed file");
        assert!(
            todo_pos < c_pos,
            "TODO should be committed before fn c(), got:\n{content}"
        );
    }

    #[test]
    fn test_add_todo_errors_when_source_file_deleted() {
        let (dir, _remote_dir) = create_test_repo_with_remote();
        commit_file(dir.path(), "main.rs", "fn main() {}\n", "Initial");

        fs::write(
            dir.path().join("main.rs"),
            "// TODO Fix bug\nfn main() {}\n",
        )
        .unwrap();

        let fsp = FileSystemProvider::new("TODO", Vec::new());
        let todos = fsp
            .parse_file(&dir.path().join("main.rs"))
            .expect("failed to parse file");
        let mut todo = todos.into_iter().next().expect("should have a todo");

        fs::remove_file(dir.path().join("main.rs")).expect("failed to delete file");

        let mut backend =
            GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        assert!(
            backend.stage_todo(&mut todo).is_err(),
            "should return Err when the source file no longer exists"
        );
    }

    #[test]
    fn test_add_todo_errors_when_todo_removed_from_file() {
        let (dir, _remote_dir) = create_test_repo_with_remote();
        commit_file(dir.path(), "main.rs", "fn main() {}\n", "Initial");

        fs::write(
            dir.path().join("main.rs"),
            "// TODO Fix bug\nfn main() {}\n",
        )
        .unwrap();

        let fsp = FileSystemProvider::new("TODO", Vec::new());
        let todos = fsp
            .parse_file(&dir.path().join("main.rs"))
            .expect("failed to parse file");
        let mut todo = todos.into_iter().next().expect("should have a todo");

        // Overwrite the file — the TODO comment at line 1 is gone.
        fs::write(dir.path().join("main.rs"), "fn main() {}\n").unwrap();

        let mut backend =
            GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        assert!(
            backend.stage_todo(&mut todo).is_err(),
            "should return Err when the TODO comment has been removed from the file"
        );
    }

    #[test]
    fn test_add_todo_stage_preceding_unrelated_deletion() {
        let (dir, _remote_dir) = create_test_repo_with_remote();
        commit_file(
            dir.path(),
            "main.rs",
            "unrelated_code\ndeleted_when_todo_staged\nfn main() {}\n",
            "Initial",
        );

        // Working tree: two old lines removed, TODO (no ID) at line 1.
        fs::write(
            dir.path().join("main.rs"),
            "// TODO #1 Fix bug\n//\n// more info\nfn main() {}\n",
        )
        .unwrap();

        let fsp = FileSystemProvider::new("TODO", Vec::new());
        let todos = fsp
            .parse_file(&dir.path().join("main.rs"))
            .expect("failed to parse file");
        let mut todo = todos.into_iter().next().expect("should have a todo");
        let mut backend =
            GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        backend.stage_todo(&mut todo).expect("stage_todo failed");
        backend
            .commit(&format!("chore: add todo {}", todo.display_id()))
            .expect("commit failed");

        let (added, removed) = head_commit_diff(dir.path());
        assert_eq!(
            added,
            vec!["// TODO #1 Fix bug", "//", "// more info"],
            "only the TODO should be staged"
        );
        assert_eq!(
            removed,
            vec!["unrelated_code", "deleted_when_todo_staged"],
            "only the TODO should be staged"
        );
    }

    #[test]
    fn test_add_todo_does_stage_preceding_todo_deletion() {
        let (dir, _remote_dir) = create_test_repo_with_remote();
        commit_file(
            dir.path(),
            "main.rs",
            "// TODO old todo\n//\n// stale info\nfn main() {}\n",
            "Initial",
        );

        // Working tree: old TODO replaced with new TODO (no ID yet).
        fs::write(
            dir.path().join("main.rs"),
            "// TODO #1 Fix bug\n//\n// more info\nfn main() {}\n",
        )
        .unwrap();

        let fsp = FileSystemProvider::new("TODO", Vec::new());
        let todos = fsp
            .parse_file(&dir.path().join("main.rs"))
            .expect("failed to parse file");
        let mut todo = todos.into_iter().next().expect("should have a todo");
        let mut backend =
            GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        backend.stage_todo(&mut todo).expect("stage_todo failed");
        backend
            .commit(&format!("chore: add todo {}", todo.display_id()))
            .expect("commit failed");

        let (added, removed) = head_commit_diff(dir.path());
        assert_eq!(
            added,
            vec!["// TODO #1 Fix bug", "// more info"],
            "only the TODO should be staged"
        );
        assert_eq!(
            removed,
            vec!["// TODO old todo", "// stale info"],
            "old TODO deletion should be staged, got: {removed:?}"
        );
    }

    #[test]
    fn test_stage_file_and_stage_todo_commit_together() {
        let (dir, _remote_dir) = create_test_repo_with_remote();
        commit_file(dir.path(), "main.rs", "fn main() {}\n", "Initial");

        // Working tree: TODO (with ID already assigned) added at line 1.
        fs::write(
            dir.path().join("main.rs"),
            "// TODO #1 Fix bug\nfn main() {}\n",
        )
        .unwrap();

        // Simulate `.tdz/ids`: a brand-new, untracked file written by
        // MergeFileIDStrategy::write_id.
        let ids_dir = dir.path().join(".tdz");
        fs::create_dir_all(&ids_dir).unwrap();
        fs::write(ids_dir.join("ids"), "#1 Fix bug\n").unwrap();

        let fsp = FileSystemProvider::new("TODO", Vec::new());
        let todos = fsp
            .parse_file(&dir.path().join("main.rs"))
            .expect("failed to parse file");
        let mut todo = todos.into_iter().next().expect("should have a todo");

        let mut backend =
            GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");

        backend.stage_todo(&mut todo).expect("stage_todo failed");
        backend
            .stage_file(&dir.path().join(".tdz/ids"))
            .expect("stage_file failed");
        backend
            .commit(&format!("chore: add todo {}", todo.display_id()))
            .expect("commit failed");

        // The TODO line should be part of the commit's diff against main.rs.
        let (added, removed) = head_commit_diff(dir.path());
        assert!(
            added.contains(&"// TODO #1 Fix bug".to_string()),
            "TODO line should be staged and committed, got added: {added:?}"
        );
        assert!(removed.is_empty(), "nothing should be removed from main.rs");

        // The .tdz/ids file should also be present in HEAD with its full content.
        let ids_content = head_file_content(dir.path(), ".tdz/ids");
        assert_eq!(
            ids_content, "#1 Fix bug\n",
            ".tdz/ids should be committed alongside the TODO"
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
        let dir = create_test_repo();

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
        let dir = create_test_repo();

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
        let dir = create_test_repo();

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
        let dir = create_test_repo();

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
        let dir = create_test_repo();

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
        let dir = create_test_repo();

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
        let dir = create_test_repo();

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
        let dir = create_test_repo();

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
        let dir = create_test_repo();

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
        let dir = create_test_repo();

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
        let dir = create_test_repo();

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
        let dir = create_test_repo();

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
        let dir = create_test_repo();

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
        let dir = create_test_repo();

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
        let dir = create_test_repo();

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
        let dir = create_test_repo();

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
        let dir = create_test_repo();

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
        let dir = create_test_repo();

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

    // ====== get_history_for_todo / trace_todo tests ======

    #[test]
    fn test_trace_todo_single_commit() {
        let dir = create_test_repo();

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #1 Fix bug\nfn main() {}",
            "Add TODO",
        );
        let sha = head_sha(dir.path());

        let backend = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        let todos = backend.get_all_todos().expect("failed to scan");
        let todo = todos.get(&1).expect("TODO #1 should exist").clone();

        let history = backend.trace_todo(&todo).expect("trace_todo failed");

        assert_eq!(history.len(), 1, "should have exactly one history entry");
        assert_eq!(history[0].0.sha, sha);
        assert_eq!(history[0].1.title, "Fix bug");
    }

    #[test]
    fn test_trace_todo_tracks_edits() {
        let dir = create_test_repo();

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #1 Original title\nfn main() {}",
            "Add TODO",
        );
        let sha_a = head_sha(dir.path());

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #1 (A) Modified title +urgent\nfn main() {}",
            "Modify TODO",
        );
        let sha_b = head_sha(dir.path());

        let backend = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        let todos = backend.get_all_todos().expect("failed to scan");
        let todo = todos.get(&1).expect("TODO #1 should exist").clone();

        let history = backend.trace_todo(&todo).expect("trace_todo failed");

        assert_eq!(
            history.len(),
            2,
            "should track both the creation and the edit"
        );
        assert_eq!(
            history[0].0.sha, sha_a,
            "oldest entry should be the creation commit"
        );
        assert_eq!(history[0].1.title, "Original title");
        assert_eq!(
            history[1].0.sha, sha_b,
            "newest entry should be the edit commit"
        );
        assert_eq!(history[1].1.title, "Modified title");
    }

    #[test]
    fn test_trace_todo_tracks_moves_via_title() {
        let dir = create_test_repo();

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #1 Stable title\nfn main() {}",
            "Add TODO",
        );
        let sha_a = head_sha(dir.path());

        // Insert a line above the TODO and add a priority/tag to it. The
        // edited TODO line keeps its title but moves to line 2, so blame
        // attributes line 2 to this commit while the parent's matching
        // todo is found by title rather than start line.
        commit_file(
            dir.path(),
            "main.rs",
            "fn helper() {}\n// TODO #1 (A) Stable title +urgent\nfn main() {}",
            "Annotate and shift TODO",
        );
        let sha_b = head_sha(dir.path());

        let backend = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        let todos = backend.get_all_todos().expect("failed to scan");
        let todo = todos.get(&1).expect("TODO #1 should exist").clone();
        assert_eq!(todo.location.start_line_num, 2);

        let history = backend.trace_todo(&todo).expect("trace_todo failed");

        assert_eq!(
            history.len(),
            2,
            "should track creation and the title-matched move"
        );
        assert_eq!(history[0].0.sha, sha_a);
        assert_eq!(history[0].1.location.start_line_num, 1);
        assert_eq!(history[0].1.title, "Stable title");
        assert_eq!(history[1].0.sha, sha_b);
        assert_eq!(history[1].1.location.start_line_num, 2);
        assert_eq!(history[1].1.title, "Stable title");
    }

    #[test]
    fn test_trace_todo_skips_unrelated_intermediate_commits() {
        let dir = create_test_repo();

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #1 Fix bug\nfn main() {}\n",
            "Add TODO",
        );
        let sha_a = head_sha(dir.path());

        // Unrelated change: insert a line above the TODO, shifting it from
        // line 1 to line 2 without altering its content. This commit must
        // not appear in the trace.
        commit_file(
            dir.path(),
            "main.rs",
            "fn helper() {}\n// TODO #1 Fix bug\nfn main() {}\n",
            "Add helper function",
        );

        // Edit the TODO itself.
        commit_file(
            dir.path(),
            "main.rs",
            "fn helper() {}\n// TODO #1 (A) Fixed bug +urgent\nfn main() {}\n",
            "Modify TODO",
        );
        let sha_c = head_sha(dir.path());

        let backend = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        let todos = backend.get_all_todos().expect("failed to scan");
        let todo = todos.get(&1).expect("TODO #1 should exist").clone();
        assert_eq!(todo.location.start_line_num, 2);

        let history = backend.trace_todo(&todo).expect("trace_todo failed");

        assert_eq!(
            history.len(),
            2,
            "the unrelated intermediate commit should not appear in the trace, got: {history:#?}"
        );
        assert_eq!(history[0].0.sha, sha_a);
        assert_eq!(history[0].1.location.start_line_num, 1);
        assert_eq!(history[0].1.title, "Fix bug");
        assert_eq!(history[1].0.sha, sha_c);
        assert_eq!(history[1].1.location.start_line_num, 2);
        assert_eq!(history[1].1.title, "Fixed bug");
    }

    #[test]
    fn test_trace_todo_stops_at_root_commit() {
        let dir = create_test_repo();

        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #1 Fix bug\nfn main() {}",
            "Add TODO",
        );
        let sha_a = head_sha(dir.path());

        // A later, unrelated commit that doesn't touch main.rs.
        commit_file(dir.path(), "other.rs", "fn other() {}", "Add other file");

        let backend = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        let todos = backend.get_all_todos().expect("failed to scan");
        let todo = todos.get(&1).expect("TODO #1 should exist").clone();

        let history = backend.trace_todo(&todo).expect("trace_todo failed");

        assert_eq!(
            history.len(),
            1,
            "history should stop at the root commit that created the todo"
        );
        assert_eq!(history[0].0.sha, sha_a);
    }

    #[test]
    fn test_trace_todo_requires_file_path() {
        let dir = create_test_repo();
        commit_file(
            dir.path(),
            "main.rs",
            "// TODO #1 Fix bug\nfn main() {}",
            "Add TODO",
        );

        let backend = GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");

        let todo = Todo {
            id: Some(TodoIdentifier::Primary(1)),
            title: "Fix bug".to_string(),
            location: crate::todo::Location {
                file_path: None,
                start_line_num: 1,
                end_line_num: 1,
            },
            ..Default::default()
        };

        let result = backend.trace_todo(&todo);
        assert!(
            matches!(result, Err(Error::Custom(_))),
            "expected Error::Custom when todo has no file path, got {:?}",
            result
        );
    }
}
