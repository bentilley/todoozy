use super::Provider;
use super::Result;
use crate::fs::{FileTypeAwarePath, Walk, WalkConfig};
use crate::todo::{parser::TodoParser, Todo, TodoIdentifier, Todos};
use std::sync::{Arc, Mutex};

pub struct FileSystemProvider {
    exclude: Vec<String>,
    todo_parser: TodoParser,
}

impl FileSystemProvider {
    pub fn new(todo_token: &str, exclude: Vec<String>) -> Self {
        Self {
            exclude,
            todo_parser: TodoParser::new(todo_token),
        }
    }

    fn parse_files(&self, files: Walk) -> Result<Todos> {
        let todos: Arc<Mutex<Vec<Todo>>> = Arc::new(Mutex::new(Vec::new()));

        files.run(|| {
            let todos = Arc::clone(&todos);
            move |path: &std::path::Path| {
                if let Ok(ref mut tdz) = self.parse_file(path) {
                    todos.lock().unwrap().append(tdz);
                }
            }
        });

        let todos = Arc::try_unwrap(todos)
            .expect("Walk should have completed")
            .into_inner()
            .unwrap();
        Ok(todos.into())
    }

    fn parse_file(&self, file_path: &std::path::Path) -> Result<Vec<Todo>> {
        let file_type = file_path.get_filetype().ok_or("Invalid file type")?;
        let file_name = file_path.to_str().ok_or("Invalid file path")?;

        let text = match std::fs::read_to_string(file_path) {
            Ok(text) => text,
            Err(_) => return Err(format!("Failed to read file: {}", file_name).into()),
        };

        Ok(self
            .todo_parser
            .parse_text(&text, file_type)
            .into_iter()
            .map(|mut todo| {
                todo.location.file_path = Some(file_name.to_string());
                todo
            })
            .collect())
    }
}

impl Provider for FileSystemProvider {
    fn get_todos(&self) -> Result<Todos> {
        let walk = Walk::new(&WalkConfig::new(".", Some(&self.exclude)));
        Ok(self.parse_files(walk)?)
    }

    fn get_todo(&self, id: u32) -> Result<Option<Todo>> {
        let todos = self.get_todos()?;
        Ok(todos
            .into_iter()
            .find(|t| t.id == Some(TodoIdentifier::Primary(id))))
    }
}
