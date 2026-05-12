use super::TodoCommand;
use crate::cli::args::{Command, Mode};
use crate::cli::config;
use crate::cli::error;
use git2::{ApplyLocation, ApplyOptions, DiffOptions, Repository};
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;
use todoozy::provider::{FileSystemProvider, Provider};
use todoozy::todo::{
    store::{SqliteStore, Store},
    Todo,
};

pub const USAGE: &str = r#"Add untracked todos (assign IDs)

Usage: tdz todo add [OPTIONS]

Options:
    --all                          Add all untracked todos
    --location <DIR|FILE[:LINE]>   Add todo at specific location
    --help                         Print help

Examples:
    tdz todo add --all
    tdz todo add --location src/main.rs
    tdz todo add --location src/main.rs:42
    tdz todo add --location src/         Add all todos in src/ directory
"#;

pub struct TodoAddOptions {
    pub all: bool,
    pub location: Option<LocationSpec>,
}

pub enum LocationSpec {
    File(PathBuf),
    FileLine(PathBuf, usize),
    Dir(PathBuf),
}

impl TodoAddOptions {
    pub fn new() -> Self {
        Self {
            all: false,
            location: None,
        }
    }
}

pub fn parse_opts(mut parser: lexopt::Parser) -> error::Result<Mode> {
    use lexopt::prelude::*;

    let mut opts = TodoAddOptions::new();

    while let Some(arg) = parser.next()? {
        match arg {
            Long("all") => opts.all = true,
            Long("location") => {
                let value: String = parser.value()?.parse()?;
                opts.location = Some(value.try_into()?);
            }
            Long("help") => return Ok(Mode::Help(USAGE)),
            _ => return Err(arg.unexpected().into()),
        }
    }

    if !opts.all && opts.location.is_none() {
        return Err("must specify --all or --location <file[:line]>".into());
    }

    if opts.all && opts.location.is_some() {
        return Err("cannot specify both --all and --location".into());
    }

    Ok(Mode::Cli(Command::Todo(TodoCommand::Add(opts))))
}

impl TryFrom<String> for LocationSpec {
    type Error = error::Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let path = PathBuf::from(&value);

        if path.is_dir() {
            return Ok(LocationSpec::Dir(path));
        }

        if let Some((file, line_str)) = value.rsplit_once(':') {
            if let Ok(line) = line_str.parse::<usize>() {
                return Ok(LocationSpec::FileLine(file.into(), line));
            }
        }

        Ok(LocationSpec::File(value.into()))
    }
}

fn todo_matches_location(todo: &Todo, location: &LocationSpec) -> bool {
    let Some(todo_path) = todo.location.file_path.as_deref() else {
        return false;
    };

    match location {
        LocationSpec::File(file) => {
            normalize_location_path(todo_path) == normalize_location_path(file)
        }
        LocationSpec::FileLine(file, line) => {
            normalize_location_path(todo_path) == normalize_location_path(file)
                && todo.location.start_line_num == *line
        }
        LocationSpec::Dir(dir) => {
            let normalized_dir = normalize_location_path(dir);
            let normalized_todo = normalize_location_path(todo_path);
            normalized_todo.starts_with(&normalized_dir)
        }
    }
}

fn normalize_location_path(path: impl AsRef<Path>) -> PathBuf {
    path.as_ref()
        .components()
        .filter(|component| !matches!(component, Component::CurDir))
        .fold(PathBuf::new(), |mut normalized, component| {
            normalized.push(component.as_os_str());
            normalized
        })
}

fn commit_todo_id(todo: &Todo, id: u32) -> Result<(), Box<dyn std::error::Error>> {
    let repo = Repository::open_from_env()?;

    let file_path = todo
        .location
        .file_path
        .as_ref()
        .ok_or("no file path on todo")?;
    let line_num = todo.location.start_line_num as u32;

    let mut diff_opts = DiffOptions::new();
    diff_opts.pathspec(file_path);
    let diff = repo.diff_index_to_workdir(None, Some(&mut diff_opts))?;

    let mut apply_opts = ApplyOptions::new();
    apply_opts.hunk_callback(move |hunk| {
        hunk.map_or(false, |h| {
            let start = h.new_start();
            line_num >= start && line_num < start + h.new_lines()
        })
    });

    repo.apply(&diff, ApplyLocation::Index, Some(&mut apply_opts))?;

    let mut index = repo.index()?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let sig = repo.signature()?;
    let parent = repo.head()?.peel_to_commit()?;
    repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        &format!("chore: add todo #{id}"),
        &tree,
        &[&parent],
    )?;

    Ok(())
}

pub fn add(conf: &mut config::Config, opts: &TodoAddOptions) -> error::Result<ExitCode> {
    let todos =
        FileSystemProvider::new(&conf.get_todo_token(), conf.exclude.clone()).get_todos()?;

    let store = SqliteStore::new()?;

    let mut added_count = 0;

    for mut todo in todos {
        if todo.id.is_some() {
            continue;
        }

        // Apply location filter if specified
        if let Some(ref location) = opts.location {
            if !todo_matches_location(&todo, location) {
                continue;
            }
        }

        let id = store.set_todo(&todo)?;

        // conf.num_todos += 1;
        // let id = conf.num_todos;

        match todo.add(id) {
            Ok(_) => {
                println!("Added: #{} {}", id, todo.title);
                added_count += 1;
                if let Err(e) = commit_todo_id(&todo, id) {
                    eprintln!("Warning: could not commit todo #{id} to vcs: {e}");
                }
            }
            Err(e) => {
                eprintln!("Error adding '{}': {}", todo.title, e);
                conf.num_todos -= 1; // Roll back
            }
        }
    }

    if added_count > 0 {
        if let Err(e) = conf.save() {
            eprintln!("Error saving config: {}", e);
        }
    }

    if added_count == 0 {
        println!("No untracked todos found matching the criteria.");
    }

    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use todoozy::todo::{Location, Todo, TodoInfoBuilder};

    fn make_todo(file_path: &str, line: usize) -> Todo {
        Todo::new(
            TodoInfoBuilder::default()
                .title("Test todo".to_string())
                .build()
                .unwrap(),
            Location::from_file_line(Some(file_path.to_string()), line),
        )
    }

    #[test]
    fn test_location_spec_try_from_parses_directory() {
        let dir = std::env::current_dir().unwrap();

        let location = LocationSpec::try_from(dir.to_string_lossy().into_owned()).unwrap();

        assert!(matches!(location, LocationSpec::Dir(path) if path == dir));
    }

    #[test]
    fn test_location_spec_try_from_parses_file_and_line() {
        let location = LocationSpec::try_from("src/main.rs:42".to_string()).unwrap();

        assert!(matches!(
            location,
            LocationSpec::FileLine(path, 42) if path == PathBuf::from("src/main.rs")
        ));
    }

    #[test]
    fn test_location_spec_try_from_parses_file_without_line() {
        let location = LocationSpec::try_from("src/main.rs".to_string()).unwrap();

        assert!(matches!(
            location,
            LocationSpec::File(path) if path == PathBuf::from("src/main.rs")
        ));
    }

    #[test]
    fn test_location_spec_try_from_treats_invalid_line_suffix_as_file() {
        let location = LocationSpec::try_from("src/main.rs:not-a-line".to_string()).unwrap();

        assert!(matches!(
            location,
            LocationSpec::File(path) if path == PathBuf::from("src/main.rs:not-a-line")
        ));
    }

    #[test]
    fn test_todo_matches_location_normalizes_curdir_prefix_for_file() {
        let todo = make_todo("./src/cli/todo/get.rs", 8);

        assert!(todo_matches_location(
            &todo,
            &LocationSpec::File("src/cli/todo/get.rs".into())
        ));
    }

    #[test]
    fn test_todo_matches_location_normalizes_curdir_prefix_for_file_and_line() {
        let todo = make_todo("./src/cli/todo/get.rs", 8);

        assert!(todo_matches_location(
            &todo,
            &LocationSpec::FileLine("src/cli/todo/get.rs".into(), 8)
        ));
    }

    #[test]
    fn test_todo_matches_location_for_dir() {
        let todo = make_todo("./src/cli/todo/get.rs", 8);

        // Todo in src/cli/todo/ should match src/cli/ directory
        assert!(todo_matches_location(
            &todo,
            &LocationSpec::Dir("src/cli".into())
        ));

        // Todo in src/cli/todo/ should NOT match src/provider/ directory
        assert!(!todo_matches_location(
            &todo,
            &LocationSpec::Dir("src/provider".into())
        ));
    }
}
