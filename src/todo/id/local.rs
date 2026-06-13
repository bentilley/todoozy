use super::error::Result;
use super::IDStrategy;
use crate::todo::Todo;

pub struct MergeFileIDStrategy {
    file_path: std::path::PathBuf,
}

impl MergeFileIDStrategy {
    pub fn new(file_path: std::path::PathBuf) -> Self {
        Self { file_path }
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
            .write(true)
            .truncate(true)
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
}

// TODO #104 (C) Add tests for MergeFileIDStrategy +test
#[cfg(test)]
mod tests {}
