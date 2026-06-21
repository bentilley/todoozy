use crate::cli::config::Config;
use crate::cli::error::Result;
use todoozy::provider::{
    vcs::{create_vcs_backend, error::Error as VcsError, VcsBackend},
    FileSystemProvider, Provider,
};
use todoozy::todo::{Todo, Todos};

#[derive(Debug, PartialEq)]
pub enum TodoID {
    Legacy(u32),
    Hash(String),
}

impl std::str::FromStr for TodoID {
    type Err = String;
    fn from_str(s: &str) -> core::result::Result<Self, Self::Err> {
        if let Ok(id) = s.parse::<u32>() {
            Ok(TodoID::Legacy(id))
        } else if u64::from_str_radix(s, 16).is_ok() {
            Ok(TodoID::Hash(s.to_string()))
        } else {
            Err(format!(
                "invalid ID '{}', expected a number or hexadecimal hash",
                s
            ))
        }
    }
}

impl std::fmt::Display for TodoID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TodoID::Legacy(id) => write!(f, "#{}", id),
            TodoID::Hash(hash) => write!(f, "{}", hash),
        }
    }
}

pub struct Tdz {
    fs: FileSystemProvider,
    vcs: Option<Box<dyn VcsBackend>>,
}

impl Tdz {
    pub fn open(conf: &Config) -> Result<Self> {
        let fs = FileSystemProvider::new(&conf.get_todo_token(), conf.exclude.clone());
        let cwd = std::env::current_dir()?;
        let vcs = create_vcs_backend(&cwd, &conf.get_todo_token(), None).ok();
        Ok(Tdz { fs, vcs })
    }

    pub fn get_current_todos(&self) -> Result<Todos> {
        let todos = self.fs.get_todos()?;
        Ok(todos.into())
    }

    pub fn get_all_todos(&self) -> Result<Todos> {
        match &self.vcs {
            Some(vcs_backend) => {
                let mut vcs_todos = vcs_backend.get_all_todos()?;
                let fs_todos = self.fs.get_todos()?;

                vcs_todos.merge(fs_todos);
                Ok(vcs_todos.into())
            }
            None => Err("No VCS backend available".into()),
        }
    }

    pub fn get_todo(&self, id: &TodoID) -> Result<Option<Todo>> {
        let fs_todo = self.fs_lookup(&id)?;
        match fs_todo {
            Some(todo) => Ok(Some(todo)),
            None => self.vcs_lookup(&id, "HEAD"),
        }
    }

    pub fn get_current_todo(&self, id: &TodoID) -> Result<Option<Todo>> {
        self.fs_lookup(&id)
    }

    pub fn get_todo_for_version(&self, id: &TodoID, version: &str) -> Result<Option<Todo>> {
        self.vcs_lookup(&id, version)
    }

    fn fs_lookup(&self, id: &TodoID) -> Result<Option<Todo>> {
        match id {
            TodoID::Hash(hash_id) => {
                let todo = self.fs.get_todo_from_hash(hash_id)?;
                Ok(todo)
            }
            TodoID::Legacy(legacy_id) => {
                let todo = self.fs.get_todo(legacy_id.clone())?;
                Ok(todo)
            }
        }
    }

    fn vcs_lookup(&self, id: &TodoID, version: &str) -> Result<Option<Todo>> {
        match id {
            TodoID::Hash(_) => {
                return Err("--version only supports legacy numeric IDs".into());
            }
            TodoID::Legacy(legacy_id) => match &self.vcs {
                Some(vcs_backend) => {
                    match vcs_backend.get_todo_for_version(legacy_id.clone(), version) {
                        Ok(todo) => Ok(Some(todo)),
                        Err(VcsError::Custom(msg)) if msg.contains("not found") => Ok(None),
                        Err(e) => Err(e.into()),
                    }
                }
                None => Err("No VCS backend available".into()),
            },
        }
    }
}
