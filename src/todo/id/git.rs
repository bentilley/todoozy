use super::error::{Error, Result};
use super::IDStrategy;
use crate::todo::Todo;
use git2::Repository;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct GitIDStrategy {
    repo: git2::Repository,
    remote_name: String,
}

impl GitIDStrategy {
    pub fn new(path: &Path, remote_name: String) -> Result<Self> {
        let repo = Repository::discover(path)?;

        Ok(Self { repo, remote_name })
    }

    fn fetch_id_tags(&self) -> Result<()> {
        let mut remote = self
            .repo
            .find_remote(&self.remote_name)
            .map_err(|_| -> Error {
                format!("no remote '{}' configured", &self.remote_name).into()
            })?;
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

    fn try_push_tag(&self, id: u32) -> Result<bool> {
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
        let mut remote = self.repo.find_remote(&self.remote_name)?;
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

    /// Move the claim tag for `id` to the current HEAD and force-push it.
    ///
    /// This is best-effort: the ID reservation was already established by
    /// `try_push_tag`, so a failure here leaves the tag at the wrong commit
    /// but does not affect correctness.
    #[allow(dead_code)]
    fn move_tag_to_head(&self, remote_name: &str, id: u32) -> Result<()> {
        let tag_name = format!("tdz/{id}");
        let head = self.repo.head()?.peel_to_commit()?;
        self.repo
            .tag_lightweight(&tag_name, head.as_object(), true)?;

        let callbacks = Self::make_credentials_callback();
        let mut opts = git2::PushOptions::new();
        opts.remote_callbacks(callbacks);

        let refspec = format!("+refs/tags/{tag_name}:refs/tags/{tag_name}");
        let mut remote = self.repo.find_remote(remote_name)?;
        // Ignore push errors: reservation is already established.
        let _ = remote.push(&[refspec.as_str()], Some(&mut opts));
        Ok(())
    }
}

impl IDStrategy for GitIDStrategy {
    fn next(&mut self, _todo: &Todo) -> super::error::Result<u32> {
        self.fetch_id_tags()?;
        let start = self.max_local_tag_id() + 1;
        for id in start.. {
            if self.try_push_tag(id)? {
                return Ok(id);
            }
        }
        unreachable!()
    }
}

// #[cfg(test)]
// mod tests {
//     #[test]
//     fn test_add_todo_pushes_claim_tag_to_remote() {
//         let (dir, remote_dir) = create_test_repo_with_remote();
//         commit_file(dir.path(), "main.rs", "fn main() {}\n", "Initial");
//
//         fs::write(
//             dir.path().join("main.rs"),
//             "// TODO Fix bug\nfn main() {}\n",
//         )
//         .unwrap();
//
//         let fsp = FileSystemProvider::new("TODO", Vec::new());
//         let todos = fsp
//             .parse_file(&dir.path().join("main.rs"))
//             .expect("failed to parse file");
//         let mut todo = todos.into_iter().next().expect("should have a todo");
//         let mut backend =
//             GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
//         backend.add_todo(&mut todo).expect("add_todo failed");
//
//         let remote_repo = Repository::open(remote_dir.path()).expect("failed to open remote repo");
//         assert!(
//             remote_repo.find_reference("refs/tags/tdz/1").is_ok(),
//             "refs/tags/tdz/1 should exist on the remote after add_todo"
//         );
//     }
//
//     #[test]
//     fn test_add_todo_skips_already_claimed_id() {
//         let (dir, remote_dir) = create_test_repo_with_remote();
//
//         // Claim id=1 via the backend itself so libgit2 is used end-to-end.
//         commit_file(
//             dir.path(),
//             "main.rs",
//             "// TODO First todo\nfn main() {}\n",
//             "Initial",
//         );
//         let fsp = FileSystemProvider::new("TODO", Vec::new());
//         let mut backend =
//             GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
//         let todos1 = fsp
//             .parse_file(&dir.path().join("main.rs"))
//             .expect("failed to parse file");
//         let mut todo1 = todos1.into_iter().next().expect("should have a todo");
//         backend.add_todo(&mut todo1).expect("first add_todo failed");
//
//         // Now add a second todo — must claim tdz/2 since tdz/1 already exists on the remote.
//         fs::write(
//             dir.path().join("main.rs"),
//             "// TODO Second todo\nfn main() {}\n",
//         )
//         .unwrap();
//         let todos2 = fsp
//             .parse_file(&dir.path().join("main.rs"))
//             .expect("failed to parse file");
//         let mut todo2 = todos2.into_iter().next().expect("should have a todo");
//         backend
//             .add_todo(&mut todo2)
//             .expect("second add_todo failed");
//
//         let remote_repo = Repository::open(remote_dir.path()).expect("failed to open remote repo");
//         assert!(
//             remote_repo.find_reference("refs/tags/tdz/2").is_ok(),
//             "refs/tags/tdz/2 should be claimed when tdz/1 was already taken"
//         );
//
//         let content = head_file_content(dir.path(), "main.rs");
//         assert!(
//             content.contains("// TODO #2 Second todo"),
//             "committed file should contain #2, got:\n{content}"
//         );
//     }
//
//     #[test]
//     fn test_add_todo_errors_without_remote() {
//         let dir = create_test_repo();
//         commit_file(dir.path(), "main.rs", "fn main() {}\n", "Initial");
//
//         fs::write(
//             dir.path().join("main.rs"),
//             "// TODO Fix bug\nfn main() {}\n",
//         )
//         .unwrap();
//
//         let fsp = FileSystemProvider::new("TODO", Vec::new());
//         let todos = fsp
//             .parse_file(&dir.path().join("main.rs"))
//             .expect("failed to parse file");
//         let mut todo = todos.into_iter().next().expect("should have a todo");
//         let mut backend =
//             GitBackend::new(dir.path(), "TODO", None).expect("failed to create backend");
//         match backend.add_todo(&mut todo) {
//             Err(Error::Custom(msg)) => assert!(
//                 msg.contains("no remote"),
//                 "error should mention 'no remote', got: {msg}"
//             ),
//             Err(e) => panic!("expected Error::Custom mentioning 'no remote', got: {e:?}"),
//             Ok(()) => panic!("should return Err when no remote is configured"),
//         }
//     }
// }
