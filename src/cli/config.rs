use serde::{Deserialize, Serialize};

use todoozy::todo::{
    filter::{self, Filter},
    sort::{self, Sorter},
};

const CONFIG_FILE_NAME: &str = "todoozy.json";

#[derive(Debug, Serialize, Deserialize)]
struct RawConfig {
    pub _num_todos: u64,
    pub filter: Option<String>,
    pub sort: Option<String>,
}

// TODO (C) 2024-09-05 Add exclude to the config file.
pub struct Config {
    pub _num_todos: u64,
    pub filter: Option<Box<dyn Filter>>,
    pub sorter: Option<Box<dyn Sorter>>,
}

impl From<RawConfig> for Config {
    fn from(raw: RawConfig) -> Self {
        Config {
            _num_todos: raw._num_todos,
            filter: match raw.filter {
                Some(f) => Some(filter::parse_str(f).expect("Invalid filter in config file")),
                None => None,
            },
            sorter: match raw.sort {
                Some(f) => Some(sort::parse_str(f).expect("Invalid sort in config file")),
                None => None,
            },
        }
    }
}

pub fn load_config() -> Result<Config, Box<dyn std::error::Error>> {
    let repo = git2::Repository::open_from_env()?;
    let root = repo.workdir().ok_or("Could not find workdir")?;

    match std::fs::read_to_string(root.join(CONFIG_FILE_NAME)) {
        Ok(data) => {
            let c: RawConfig = serde_json::from_str(&data)?;
            Ok(c.into())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            write_initial_config()?;
            Ok(Config {
                _num_todos: 0,
                filter: None,
                sorter: None,
            })
        }
        Err(e) => Err(e.into()),
    }
}

fn write_initial_config() -> Result<(), Box<dyn std::error::Error>> {
    let repo = git2::Repository::open_from_env()?;
    let root = repo.workdir().ok_or("Could not find workdir")?;
    let data = r#"{
  "_num_todos": 0
}"#;
    std::fs::write(root.join(CONFIG_FILE_NAME), data)?;
    Ok(())
}
