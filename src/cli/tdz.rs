use crate::cli::config::Config;
use crate::cli::error::Result;
use todoozy::provider::{
    vcs::{create_vcs_backend, error::Error as VcsError, CommitMetadata, VcsBackend},
    FileSystemProvider, Provider,
};
use todoozy::todo::{
    id::{IDStrategy, MergeFileIDStrategy},
    store::{SqliteStore, Store},
    Todo, Todos,
};

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
    id_strategy: Option<Box<dyn IDStrategy>>,
}

impl Tdz {
    pub fn open(conf: &Config) -> Result<Self> {
        let fs = FileSystemProvider::new(&conf.get_todo_token(), conf.exclude.clone());
        let cwd = std::env::current_dir()?;
        let vcs = create_vcs_backend(&cwd, &conf.get_todo_token(), None).ok();
        let id_strategy = MergeFileIDStrategy::open(conf.get_id_file_path())
            .ok()
            .map(|s| -> Box<dyn IDStrategy> { Box::new(s) });
        Ok(Tdz { fs, vcs, id_strategy })
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

    pub fn edit_todo(&self, id: &TodoID) -> Result<()> {
        let todo = self
            .get_todo(id)?
            .ok_or_else(|| format!("Todo #{} not found", id))?;

        let editor_cmd = todo.editor_command().map_err(|e| format!("{}", e))?;

        editor_cmd.execute().map_err(|e| format!("{}", e))?;

        Ok(())
    }

    pub fn add_todos(
        &mut self,
        filter: impl Fn(&Todo) -> bool,
    ) -> Result<Vec<(u32, String)>> {
        let todos = self.fs.get_todos()?;
        let vcs = self.vcs.as_mut().ok_or("No VCS backend available")?;
        let id_strategy = self
            .id_strategy
            .as_mut()
            .ok_or("No ID strategy available")?;

        let mut added = Vec::new();

        for mut todo in todos {
            if todo.id.is_some() || !filter(&todo) {
                continue;
            }

            todo.add_id(id_strategy.next(&todo)?)
                .map_err(|e| -> crate::cli::error::Error { e.to_string().into() })?;

            match vcs.stage_todo(&mut todo) {
                Ok(_) => {
                    if let Some(path) = id_strategy.file_path() {
                        if let Err(e) = vcs.stage_file(path) {
                            eprintln!("Warning: could not stage id file: {e}");
                        }
                    }
                    match vcs.commit(&format!("chore: add todo {}", todo.display_id())) {
                        Ok(_) => {
                            let id = match todo.id {
                                Some(todoozy::todo::TodoIdentifier::Primary(id)) => id,
                                _ => unreachable!("add_id assigns a primary ID"),
                            };
                            added.push((id, todo.title.clone()));
                        }
                        Err(e) => eprintln!("Warning: could not commit todo to vcs: {e}"),
                    }
                }
                Err(e) => eprintln!("Warning: could not stage todo: {e}"),
            }
        }

        Ok(added)
    }

    pub fn remove_todo(&self, id: &TodoID) -> Result<Todo> {
        let todo = self
            .fs_lookup(id)?
            .ok_or_else(|| format!("Todo {} not found", id))?;
        todo.remove()
            .map_err(|e| format!("Error removing todo: {}", e))?;
        Ok(todo)
    }

    pub fn trace_todo(&self, id: u32) -> Result<(Todo, Vec<(CommitMetadata, Todo)>)> {
        let vcs = self
            .vcs
            .as_ref()
            .ok_or("todo trace requires a git repository")?;

        let todo = match self.fs.get_todo(id)? {
            Some(todo) => todo,
            None => match vcs.get_todo_for_version(id, "HEAD") {
                Ok(todo) => todo,
                Err(VcsError::Custom(msg)) if msg.contains("not found") => {
                    return Err(format!("Todo #{} not found", id).into());
                }
                Err(e) => return Err(e.into()),
            },
        };

        let history = vcs.trace_todo(&todo)?;
        Ok((todo, history))
    }

    pub fn import_todos(&self) -> Result<Vec<(u32, String)>> {
        let vcs = self.vcs.as_ref().ok_or("No VCS backend available")?;
        let todos = vcs.get_all_todos()?;
        let store = SqliteStore::new()?;

        let mut imported = Vec::new();
        let mut ids: Vec<u32> = todos.ids().collect();
        ids.sort_unstable();

        for id in ids {
            let todo = todos.get(&id).unwrap();
            store
                .import_todo(id, todo)
                .map_err(|e| format!("Error importing #{}: {}", id, e))?;
            imported.push((id, todo.title.clone()));
        }

        Ok(imported)
    }
}
