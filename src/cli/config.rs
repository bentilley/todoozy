use serde::{Deserialize, Serialize};

use todoozy::todo::{filter::Filter, sort::Sorter};

const CONFIG_FILE_NAME: &str = "todoozy.json";

// TODO #65 (D) 2026-03-22 Move _num_todos to local state +ids +config
//
// The _num_todos counter shouldn't be in version control because:
// - It causes merge conflicts when multiple branches import TODOs
// - It doesn't actually help coordination (branches diverge anyway)
//
// Move to local state, e.g., `.tdz/state.json` (gitignored) or `~/.tdz/<repo-hash>/`:
// - File locking prevents conflicts between local worktrees/agents
// - Value derived from `tdz cache build` (max ID from git history + 1)
// - No more merge conflicts on the counter
//
// The todoozy.json config file remains in git for: exclude, filter, sorter.
// Only the counter moves to local state.
//
// See also: `tdz cache build` command in args.rs

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(skip_serializing, default)]
    file_name: std::path::PathBuf,

    #[serde(rename = "_num_todos")]
    pub num_todos: u32,

    pub exclude: Vec<String>,

    pub filter: Option<Box<dyn Filter>>,
    pub sorter: Option<Box<dyn Sorter>>,
}

impl Config {
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let data = serde_json::to_string_pretty(self)?;
        std::fs::write(&self.file_name, data)?;
        Ok(())
    }

    pub fn load_config() -> Result<Config, Box<dyn std::error::Error>> {
        let repo = git2::Repository::open_from_env()?;
        let root = repo.workdir().ok_or("Could not find workdir")?;
        let config_file = root.join(CONFIG_FILE_NAME);

        match std::fs::read_to_string(&config_file) {
            Ok(data) => {
                let mut c: Config = serde_json::from_str(&data)?;
                c.file_name = config_file;
                Ok(c)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let config = Config {
                    file_name: config_file,
                    num_todos: 0,
                    exclude: Vec::new(),
                    filter: None,
                    sorter: Some(Box::new(todoozy::todo::sort::SortPipeline::app_default())),
                };
                config.save()?;
                Ok(config)
            }
            Err(e) => Err(e.into()),
        }
    }
}
