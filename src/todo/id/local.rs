use super::error::Result;
use super::IDStrategy;
use crate::todo::Todo;

pub struct MergeFileIDStrategy {
    file_path: std::path::PathBuf,
}

impl MergeFileIDStrategy {
    pub fn open(file_path: std::path::PathBuf) -> Result<Self> {
        match file_path.parent() {
            Some(parent) => {
                if !parent.exists() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            None => (),
        }

        Ok(Self { file_path })
    }

    fn read_max_id(&self) -> Result<u32> {
        if !self.file_path.exists() {
            return Ok(0);
        }

        let file = std::fs::OpenOptions::new()
            .read(true)
            .open(&self.file_path)
            .map_err(|e| format!("failed to open file {}: {}", self.file_path.display(), e))?;

        use std::io::BufRead;
        let lines = std::io::BufReader::new(file).lines();
        let last_line = lines.last().ok_or_else(|| "file is empty")?.map_err(|e| {
            format!(
                "failed to read line from file {}: {}",
                self.file_path.display(),
                e
            )
        })?;
        let max_id_str = last_line
            .trim_start_matches('#')
            .split_whitespace()
            .next()
            .ok_or_else(|| "failed to parse max ID from file")?;

        let max_id = max_id_str.parse::<u32>().map_err(|e| {
            format!(
                "failed to parse max ID from file {}: {}",
                self.file_path.display(),
                e
            )
        })?;

        Ok(max_id)
    }

    fn write_id(&self, max_id: u32, todo: &Todo) -> Result<()> {
        let f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)
            .map_err(|e| format!("failed to open file {}: {}", self.file_path.display(), e))?;

        use std::io::Write;
        if let Err(e) = writeln!(&f, "#{} {}", max_id, todo.title) {
            return Err(format!(
                "failed to write max ID to file {}: {}",
                self.file_path.display(),
                e
            )
            .into());
        }

        Ok(())
    }
}

impl IDStrategy for MergeFileIDStrategy {
    fn next(&mut self, todo: &Todo) -> Result<u32> {
        let mut max_id = self.read_max_id()?;
        max_id += 1;
        self.write_id(max_id, todo)?;
        Ok(max_id)
    }

    fn file_path(&self) -> Option<&std::path::Path> {
        Some(&self.file_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn todo(title: &str) -> Todo {
        Todo {
            title: title.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_next_creates_file_and_returns_one_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let id_file = dir.path().join(".tdzids");
        let mut strategy = MergeFileIDStrategy::open(id_file.clone()).unwrap();

        let id = strategy.next(&todo("First todo")).unwrap();

        assert_eq!(id, 1);
        assert_eq!(
            std::fs::read_to_string(&id_file).unwrap(),
            "#1 First todo\n"
        );
    }

    #[test]
    fn test_next_increments_sequentially_and_appends_lines() {
        let dir = tempfile::tempdir().unwrap();
        let id_file = dir.path().join(".tdzids");
        let mut strategy = MergeFileIDStrategy::open(id_file.clone()).unwrap();

        let id1 = strategy.next(&todo("First")).unwrap();
        let id2 = strategy.next(&todo("Second")).unwrap();
        let id3 = strategy.next(&todo("Third")).unwrap();

        assert_eq!((id1, id2, id3), (1, 2, 3));
        assert_eq!(
            std::fs::read_to_string(&id_file).unwrap(),
            "#1 First\n#2 Second\n#3 Third\n"
        );
    }

    #[test]
    fn test_next_continues_from_existing_max_id() {
        let dir = tempfile::tempdir().unwrap();
        let id_file = dir.path().join(".tdzids");
        std::fs::write(&id_file, "#41 Existing todo\n").unwrap();
        let mut strategy = MergeFileIDStrategy::open(id_file.clone()).unwrap();

        let id = strategy.next(&todo("Next todo")).unwrap();

        assert_eq!(id, 42);
        assert_eq!(
            std::fs::read_to_string(&id_file).unwrap(),
            "#41 Existing todo\n#42 Next todo\n"
        );
    }

    #[test]
    fn test_next_uses_only_last_line_of_existing_multiline_file() {
        let dir = tempfile::tempdir().unwrap();
        let id_file = dir.path().join(".tdzids");
        std::fs::write(&id_file, "#5 Old\n#10 Newer\n").unwrap();
        let mut strategy = MergeFileIDStrategy::open(id_file.clone()).unwrap();

        let id = strategy.next(&todo("Latest")).unwrap();

        assert_eq!(id, 11);
        assert_eq!(
            std::fs::read_to_string(&id_file).unwrap(),
            "#5 Old\n#10 Newer\n#11 Latest\n"
        );
    }

    #[test]
    fn test_next_errors_when_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let id_file = dir.path().join(".tdzids");
        std::fs::write(&id_file, "").unwrap();
        let mut strategy = MergeFileIDStrategy::open(id_file.clone()).unwrap();

        assert!(strategy.next(&todo("Anything")).is_err());
    }

    #[test]
    fn test_next_errors_on_non_numeric_id() {
        let dir = tempfile::tempdir().unwrap();
        let id_file = dir.path().join(".tdzids");
        std::fs::write(&id_file, "#abc Bad line\n").unwrap();
        let mut strategy = MergeFileIDStrategy::open(id_file.clone()).unwrap();

        assert!(strategy.next(&todo("Anything")).is_err());
    }

    #[test]
    fn test_open_creates_missing_parent_directory() {
        let dir = tempfile::tempdir().unwrap();
        let id_file = dir.path().join("nested/dir/.tdzids");

        let mut strategy = MergeFileIDStrategy::open(id_file.clone()).unwrap();

        assert!(id_file.parent().unwrap().is_dir());

        let id = strategy.next(&todo("First")).unwrap();
        assert_eq!(id, 1);
        assert_eq!(std::fs::read_to_string(&id_file).unwrap(), "#1 First\n");
    }
}
