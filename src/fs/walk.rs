pub struct Walk {
    ignore_walk: ignore::WalkParallel,
}

pub struct WalkConfig {
    root: String,
    exclude: Option<Vec<String>>,
}

impl WalkConfig {
    pub fn new(root: &str, exclude: Option<&[String]>) -> Self {
        Self {
            root: root.to_owned(),
            exclude: exclude.map(|e| e.iter().cloned().collect()),
        }
    }
}

impl Walk {
    pub fn new(config: &WalkConfig) -> Self {
        let mut builder = ignore::WalkBuilder::new(&config.root);
        builder.hidden(false);

        if let Some(to_exclude) = &config.exclude {
            let mut exclude = ignore::overrides::OverrideBuilder::new("./");
            // exclude.add("!.git/").unwrap();

            for e in to_exclude {
                exclude.add(&format!("!{}", e)).unwrap();
            }

            builder.overrides(exclude.build().unwrap());
        }

        Self {
            ignore_walk: builder.build_parallel(),
        }
    }

    pub fn run<F, G>(self, mut factory: F)
    where
        F: FnMut() -> G + Send,
        G: FnMut(&std::path::Path) + Send,
    {
        self.ignore_walk.run(move || {
            let mut handler = factory();
            Box::new(move |result| {
                if let Ok(entry) = result {
                    if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                        handler(entry.path());
                    }
                }
                ignore::WalkState::Continue
            })
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::fs::{self, File};
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_walk_visits_all_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Create a subdirectory
        let subdir = root.join("subdir");
        fs::create_dir(&subdir).unwrap();

        // Create some files
        File::create(root.join("file1.txt")).unwrap();
        File::create(root.join("file2.rs")).unwrap();
        File::create(subdir.join("file3.txt")).unwrap();

        let config = WalkConfig::new(root.to_str().unwrap(), None);
        let walk = Walk::new(&config);

        let visited: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

        walk.run(|| {
            let visited = Arc::clone(&visited);
            move |path: &std::path::Path| {
                let filename = path.file_name().unwrap().to_str().unwrap().to_string();
                visited.lock().unwrap().insert(filename);
            }
        });

        let visited = visited.lock().unwrap();
        assert!(visited.contains("file1.txt"));
        assert!(visited.contains("file2.rs"));
        assert!(visited.contains("file3.txt"));
        assert_eq!(visited.len(), 3);
    }

    #[test]
    fn test_walk_excludes_patterns() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        File::create(root.join("keep.rs")).unwrap();
        File::create(root.join("exclude.txt")).unwrap();

        let config = WalkConfig::new(root.to_str().unwrap(), Some(&vec!["*.txt".to_string()]));
        let walk = Walk::new(&config);

        let visited: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

        walk.run(|| {
            let visited = Arc::clone(&visited);
            move |path: &std::path::Path| {
                let filename = path.file_name().unwrap().to_str().unwrap().to_string();
                visited.lock().unwrap().push(filename);
            }
        });

        let visited = visited.lock().unwrap();
        assert!(visited.contains(&"keep.rs".to_string()));
        assert!(!visited.contains(&"exclude.txt".to_string()));
    }

    #[test]
    fn test_walk_skips_non_regular_files() {
        use std::os::unix::net::UnixListener;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        File::create(root.join("regular.txt")).unwrap();

        // Create a Unix socket (not a regular file, not a directory)
        let socket_path = root.join("test.sock");
        let _socket = UnixListener::bind(&socket_path).unwrap();

        let config = WalkConfig::new(root.to_str().unwrap(), None);
        let walk = Walk::new(&config);

        let visited: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

        walk.run(|| {
            let visited = Arc::clone(&visited);
            move |path: &std::path::Path| {
                let filename = path.file_name().unwrap().to_str().unwrap().to_string();
                visited.lock().unwrap().insert(filename);
            }
        });

        let visited = visited.lock().unwrap();
        assert!(visited.contains("regular.txt"), "should visit regular files");
        assert!(!visited.contains("test.sock"), "should skip socket files");
    }
}
