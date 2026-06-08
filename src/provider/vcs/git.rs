// Git backend for VCS TODO history extraction

use super::{
    error::{Error, Result},
    VcsBackend,
};
use crate::fs::{FileType, FileTypeAwarePath};
use crate::todo::{parser::TodoParser, Todo, TodoIdentifier, Todos};
use chrono::{DateTime, TimeZone, Utc};
use git2::{ApplyLocation, Commit, Diff, DiffOptions, Oid, Repository};
use itertools::Itertools;
use rayon::prelude::*;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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

    fn make_credentials_callback() -> git2::RemoteCallbacks<'static> {
        let mut callbacks = git2::RemoteCallbacks::new();
        let mut tried = false;

        callbacks.credentials(move |url, username, allowed| {
            if tried {
                return Err(git2::Error::from_str("authentication failed"));
            }
            tried = true;
            if allowed.contains(git2::CredentialType::SSH_KEY) {
                if let Ok(cred) = git2::Cred::ssh_key_from_agent(username.unwrap_or("git")) {
                    return Ok(cred);
                }
            }
            if allowed.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
                if let Ok(cfg) = git2::Config::open_default() {
                    if let Ok(cred) = git2::Cred::credential_helper(&cfg, url, username) {
                        return Ok(cred);
                    }
                }
            }
            if allowed.contains(git2::CredentialType::DEFAULT) {
                return git2::Cred::default();
            }
            Err(git2::Error::from_str("no suitable credentials available"))
        });

        callbacks
    }

    fn fetch_id_tags(&self, remote_name: &str) -> Result<()> {
        let mut remote = self
            .repo
            .find_remote(remote_name)
            .map_err(|_| Error::Custom(format!("no remote '{remote_name}' configured")))?;
        let mut opts = git2::FetchOptions::new();
        opts.remote_callbacks(Self::make_credentials_callback());
        opts.download_tags(git2::AutotagOption::None);
        // Ignore errors: remote may have no tdz/* tags yet
        let _ = remote.fetch(&["refs/tags/tdz/*:refs/tags/tdz/*"], Some(&mut opts), None);
        Ok(())
    }

    fn max_local_tag_id(&self) -> u32 {
        let Ok(refs) = self.repo.references_glob("refs/tags/tdz/*") else {
            return 0;
        };
        refs.flatten()
            .filter_map(|r| {
                let name = r.name()?;
                let suffix = name.strip_prefix("refs/tags/tdz/")?;
                suffix.parse::<u32>().ok()
            })
            .max()
            .unwrap_or(0)
    }

    fn try_push_tag(&self, remote_name: &str, id: u32) -> Result<bool> {
        let tag_name = format!("tdz/{id}");
        let tag_ref = format!("refs/tags/{tag_name}");

        // If the tag was fetched from the remote it already exists locally — skip it.
        if self.repo.find_reference(&tag_ref).is_ok() {
            return Ok(false);
        }

        let head = self.repo.head()?.peel_to_commit()?;
        self.repo
            .tag_lightweight(&tag_name, head.as_object(), false)?;

        let rejected = Arc::new(AtomicBool::new(false));
        let rejected_clone = Arc::clone(&rejected);
        let mut callbacks = Self::make_credentials_callback();
        callbacks.push_update_reference(move |_, status| {
            if status.is_some() {
                rejected_clone.store(true, Ordering::Relaxed);
            }
            Ok(())
        });

        let mut opts = git2::PushOptions::new();
        opts.remote_callbacks(callbacks);

        let refspec = format!("refs/tags/{tag_name}:refs/tags/{tag_name}");
        let mut remote = self.repo.find_remote(remote_name)?;
        match remote.push(&[refspec.as_str()], Some(&mut opts)) {
            Ok(()) => {}
            Err(e) => {
                let _ = self.repo.tag_delete(&tag_name);
                return Err(Error::from(e));
            }
        }

        if rejected.load(Ordering::Relaxed) {
            let _ = self.repo.tag_delete(&tag_name);
            Ok(false)
        } else {
            Ok(true)
        }
    }

    fn claim_next_id(&self, remote_name: &str) -> Result<u32> {
        self.fetch_id_tags(remote_name)?;
        let start = self.max_local_tag_id() + 1;
        for id in start.. {
            if self.try_push_tag(remote_name, id)? {
                return Ok(id);
            }
        }
        unreachable!()
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
        let mut new_insertion_pos: u32 = end_line;

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

                use git2::DiffLineType::*;
                match line.origin_value() {
                    Addition if line.new_lineno().map_or(false, in_todo) => {
                        if line.new_lineno().unwrap() < new_insertion_pos {
                            new_insertion_pos = line.new_lineno().unwrap();
                        }
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
                        if line.new_lineno().unwrap() < new_insertion_pos {
                            new_insertion_pos = line.new_lineno().unwrap();
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

        write!(
            &mut patch,
            "@@ -{},{} +{},{} @@\n",
            old_insertion_pos, num_deletions, new_insertion_pos, num_additions,
        )
        .unwrap();
        patch.extend(lines);

        Ok(patch)
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

    fn add_todo(&mut self, todo: &mut Todo) -> Result<()> {
        let id = self.claim_next_id("origin")?;

        todo.add_id(id).map_err(|e| Error::Custom(e.to_string()))?;

        let file_path = todo
            .location
            .file_path
            .as_ref()
            .ok_or("no file path on todo")?;

        // pathspec must be relative to the repo root; strip prefix if absolute
        let repo_root = self.get_repo_path();
        let diff_path = file_path.strip_prefix(&repo_root).unwrap_or(file_path);

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

        let mut index = self.repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = self.repo.find_tree(tree_id)?;
        let sig = self.repo.signature()?;
        let parent = self.repo.head()?.peel_to_commit()?;
        self.repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            &format!("chore: add todo #{id}"),
            &tree,
            &[&parent],
        )?;

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

    // ====== add_todo tests ======

    #[test]
    fn test_add_todo_single_line_stages_only_the_todo() {
        let (dir, _remote_dir) = create_test_repo_with_remote();
        commit_file(dir.path(), "main.rs", "fn main() {}\n", "Initial");

        // Working tree: TODO (no ID yet) added at line 1, plus an unrelated extra line.
        fs::write(
            dir.path().join("main.rs"),
            "// TODO Fix bug\nfn main() {}\nfn extra() {}\n",
        )
        .unwrap();

        let fsp = FileSystemProvider::new("TODO", Vec::new());
        let mut todos = fsp
            .parse_file(&dir.path().join("main.rs"))
            .expect("failed to parse file");
        let mut todo = &mut todos[0];
        let mut backend =
            GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        backend.add_todo(&mut todo).expect("add_todo failed");

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
            "// TODO Fix bug\n// Detail line 1\n// Detail line 2\nfn main() {}\n",
        )
        .unwrap();

        let fsp = FileSystemProvider::new("TODO", Vec::new());
        let todos = fsp
            .parse_file(&dir.path().join("main.rs"))
            .expect("failed to parse file");
        let mut todo = todos.into_iter().next().expect("should have a todo");
        let mut backend =
            GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        backend.add_todo(&mut todo).expect("add_todo failed");

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

        // Working tree: unrelated extra line appended; ID will be inserted by add_todo.
        fs::write(
            dir.path().join("main.rs"),
            "// TODO Fix bug\nfn main() {}\nfn extra() {}\n",
        )
        .unwrap();

        let fsp = FileSystemProvider::new("TODO", Vec::new());
        let todos = fsp
            .parse_file(&dir.path().join("main.rs"))
            .expect("failed to parse file");
        let mut todo = todos.into_iter().next().expect("should have a todo");
        let mut backend =
            GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        backend.add_todo(&mut todo).expect("add_todo failed");

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
            "fn a() {}\nfn new() {}\nfn b() {}\n// TODO Fix bug\nfn c() {}\n",
        )
        .unwrap();

        let fsp = FileSystemProvider::new("TODO", Vec::new());
        let todos = fsp
            .parse_file(&dir.path().join("main.rs"))
            .expect("failed to parse file");
        let mut todo = todos.into_iter().next().expect("should have a todo");
        let mut backend =
            GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        backend.add_todo(&mut todo).expect("add_todo failed");

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
            backend.add_todo(&mut todo).is_err(),
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
            backend.add_todo(&mut todo).is_err(),
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
            "// TODO Fix bug\n//\n// more info\nfn main() {}\n",
        )
        .unwrap();

        let fsp = FileSystemProvider::new("TODO", Vec::new());
        let todos = fsp
            .parse_file(&dir.path().join("main.rs"))
            .expect("failed to parse file");
        let mut todo = todos.into_iter().next().expect("should have a todo");
        let mut backend =
            GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        backend.add_todo(&mut todo).expect("add_todo failed");

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
            "// TODO Fix bug\n//\n// more info\nfn main() {}\n",
        )
        .unwrap();

        let fsp = FileSystemProvider::new("TODO", Vec::new());
        let todos = fsp
            .parse_file(&dir.path().join("main.rs"))
            .expect("failed to parse file");
        let mut todo = todos.into_iter().next().expect("should have a todo");
        let mut backend =
            GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        backend.add_todo(&mut todo).expect("add_todo failed");

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

    // ====== tag-push ID claiming tests ======

    #[test]
    fn test_add_todo_pushes_claim_tag_to_remote() {
        let (dir, remote_dir) = create_test_repo_with_remote();
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
        let mut backend =
            GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        backend.add_todo(&mut todo).expect("add_todo failed");

        let remote_repo = Repository::open(remote_dir.path()).expect("failed to open remote repo");
        assert!(
            remote_repo.find_reference("refs/tags/tdz/1").is_ok(),
            "refs/tags/tdz/1 should exist on the remote after add_todo"
        );
    }

    #[test]
    fn test_add_todo_skips_already_claimed_id() {
        let (dir, remote_dir) = create_test_repo_with_remote();

        // Claim id=1 via the backend itself so libgit2 is used end-to-end.
        commit_file(
            dir.path(),
            "main.rs",
            "// TODO First todo\nfn main() {}\n",
            "Initial",
        );
        let fsp = FileSystemProvider::new("TODO", Vec::new());
        let mut backend =
            GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        let todos1 = fsp
            .parse_file(&dir.path().join("main.rs"))
            .expect("failed to parse file");
        let mut todo1 = todos1.into_iter().next().expect("should have a todo");
        backend.add_todo(&mut todo1).expect("first add_todo failed");

        // Now add a second todo — must claim tdz/2 since tdz/1 already exists on the remote.
        fs::write(
            dir.path().join("main.rs"),
            "// TODO Second todo\nfn main() {}\n",
        )
        .unwrap();
        let todos2 = fsp
            .parse_file(&dir.path().join("main.rs"))
            .expect("failed to parse file");
        let mut todo2 = todos2.into_iter().next().expect("should have a todo");
        backend
            .add_todo(&mut todo2)
            .expect("second add_todo failed");

        let remote_repo = Repository::open(remote_dir.path()).expect("failed to open remote repo");
        assert!(
            remote_repo.find_reference("refs/tags/tdz/2").is_ok(),
            "refs/tags/tdz/2 should be claimed when tdz/1 was already taken"
        );

        let content = head_file_content(dir.path(), "main.rs");
        assert!(
            content.contains("// TODO #2 Second todo"),
            "committed file should contain #2, got:\n{content}"
        );
    }

    #[test]
    fn test_add_todo_errors_without_remote() {
        let dir = create_test_repo();
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
        let mut backend =
            GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
        match backend.add_todo(&mut todo) {
            Err(Error::Custom(msg)) => assert!(
                msg.contains("no remote"),
                "error should mention 'no remote', got: {msg}"
            ),
            Err(e) => panic!("expected Error::Custom mentioning 'no remote', got: {e:?}"),
            Ok(()) => panic!("should return Err when no remote is configured"),
        }
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
}
