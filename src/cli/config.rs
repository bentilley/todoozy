use serde::{Deserialize, Serialize};

use todoozy::todo::{filter::Filter, sort::Sorter};

const CONFIG_FILE_NAME: &str = "todoozy.json";

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
