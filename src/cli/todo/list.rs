use super::{OutputFormat, TodoCommand};
use crate::cli::args::{Command, Mode};
use crate::cli::config;
use crate::cli::error;
use std::process::ExitCode;
use todoozy::todo::filter;
use todoozy::todo::sort;

pub const USAGE: &str = r#"List todos in compact table format

Usage: tdz todo list [OPTIONS]

Options:
    -a, --all               Include completed todos from history
    -n, --limit <N>         Limit number of results
    -f, --filter <FILTER>   Filter which todos to display
    -s, --sort <SORT>       How to sort the todos
    --format <FORMAT>       Output format: raw, json (default: raw)
    --help                  Print help
"#;

pub struct TodoListOptions {
    pub include_completed: bool,
    pub limit: Option<usize>,
    pub format: OutputFormat,
    pub filter: Option<Box<dyn filter::Filter>>,
    pub sorter: Option<Box<dyn sort::Sorter>>,
}

impl Default for TodoListOptions {
    fn default() -> Self {
        Self {
            include_completed: false,
            limit: None,
            format: OutputFormat::Raw,
            filter: None,
            sorter: None,
        }
    }
}

/// Serializable representation of a Todo for JSON output.
#[derive(serde::Serialize)]
struct TodoOutput {
    id: Option<u32>,
    priority: Option<char>,
    title: String,
    description: Option<String>,
    file: Option<String>,
    line_number: Option<usize>,
    tags: Vec<String>,
    creation_date: Option<String>,
    completion_date: Option<String>,
}

impl From<&todoozy::todo::Todo> for TodoOutput {
    fn from(todo: &todoozy::todo::Todo) -> Self {
        TodoOutput {
            id: todo.id.as_ref().map(|id| match id {
                todoozy::todo::TodoIdentifier::Primary(n) => *n,
                todoozy::todo::TodoIdentifier::Reference(n) => *n,
            }),
            priority: todo.priority,
            title: todo.title.clone(),
            description: todo.description.clone(),
            file: todo.location.file_path_string(),
            line_number: Some(todo.location.start_line_num),
            tags: todo.tags.clone(),
            creation_date: todo.creation_date.map(|d| d.to_string()),
            completion_date: todo.completion_date.map(|d| d.to_string()),
        }
    }
}

pub fn parse_opts(mut parser: lexopt::Parser) -> error::Result<Mode> {
    use lexopt::prelude::*;

    let mut opts = TodoListOptions::default();

    while let Some(arg) = parser.next()? {
        match arg {
            Short('a') | Long("all") => opts.include_completed = true,
            Short('f') | Long("filter") => opts.filter = Some(parser.value()?.parse()?),
            Long("format") => opts.format = parser.value()?.parse()?,
            Short('n') | Long("limit") => opts.limit = Some(parser.value()?.parse()?),
            Short('s') | Long("sort") => opts.sorter = Some(parser.value()?.parse()?),
            Long("help") => return Ok(Mode::Help(USAGE)),
            _ => return Err(arg.unexpected().into()),
        }
    }

    Ok(Mode::Cli(Command::Todo(TodoCommand::List(opts))))
}

pub fn list(conf: &config::Config, opts: &TodoListOptions) -> error::Result<ExitCode> {
    let tdz = crate::cli::tdz::Tdz::open(conf)?;

    let mut todos = match opts.include_completed {
        true => tdz.get_all_todos()?,
        false => tdz.get_current_todos()?,
    };

    // Use opts.filter if present, otherwise fall back to conf.filter
    if let Some(ref f) = opts.filter {
        todos.apply_filter(|todo| f.filter(todo));
    } else if let Some(ref f) = conf.filter {
        todos.apply_filter(|todo| f.filter(todo));
    }

    // Use opts.sorter if present, otherwise fall back to conf.sorter
    let sorter = opts.sorter.as_ref().or(conf.sorter.as_ref());

    // Convert to Vec, applying sort if specified
    let all_todos: Vec<_> = match sorter {
        Some(s) => todos.into_sorted(|a, b| s.compare(a, b)),
        None => todos.into(),
    };

    // Apply limit if specified
    let all_todos: Vec<_> = match opts.limit {
        Some(n) => all_todos.into_iter().take(n).collect(),
        None => all_todos,
    };

    match opts.format {
        OutputFormat::Raw => write_raw(&mut std::io::stdout(), &all_todos)?,
        OutputFormat::Json => write_json(&mut std::io::stdout(), &all_todos)?,
    }

    Ok(ExitCode::SUCCESS)
}

fn write_raw(w: &mut impl std::io::Write, todos: &[todoozy::todo::Todo]) -> std::io::Result<()> {
    let ids: Vec<String> = todos.iter().map(|t| t.display_id()).collect();
    let id_width = ids.iter().map(|id| id.len()).max().unwrap_or(0);

    let locations: Vec<String> = todos.iter().map(|t| t.location.display_start()).collect();
    let location_width = locations.iter().map(|l| l.len()).max().unwrap_or(0);

    let is_completed: Vec<bool> = todos.iter().map(|t| t.completion_date.is_some()).collect();
    let has_completed = is_completed.iter().any(|s| *s);

    for (i, todo) in todos.iter().enumerate() {
        if has_completed {
            write!(
                w,
                "[{}] ",
                match is_completed[i] {
                    true => "x",
                    false => " ",
                }
            )?;
        }
        writeln!(
            w,
            "{} {:<id_width$} {} {:<location_width$} {} {}",
            todo.display_hash(6),
            todo.display_id(),
            todo.display_priority(),
            todo.location.display_start(),
            todo.title,
            todo.display_tags(),
        )?;
    }

    Ok(())
}

fn write_json(w: &mut impl std::io::Write, todos: &[todoozy::todo::Todo]) -> std::io::Result<()> {
    let output: Vec<TodoOutput> = todos.iter().map(TodoOutput::from).collect();
    serde_json::to_writer_pretty(&mut *w, &output)?;
    writeln!(w)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use todoozy::todo::{Location, Todo, TodoIdentifier, TodoInfoBuilder};

    fn make_todo(id: u32, priority: char, title: &str, file: &str, line: usize) -> Todo {
        Todo::new(
            TodoInfoBuilder::default()
                .id(Some(TodoIdentifier::Primary(id)))
                .priority(Some(priority))
                .title(title.to_string())
                .build()
                .unwrap(),
            Location::new(Some(file.to_string()), line, line),
        )
    }

    fn make_completed_todo(
        id: u32,
        priority: char,
        title: &str,
        file: &str,
        line: usize,
        completion_date: chrono::NaiveDate,
    ) -> Todo {
        let mut todo = Todo::new(
            TodoInfoBuilder::default()
                .id(Some(TodoIdentifier::Primary(id)))
                .priority(Some(priority))
                .title(title.to_string())
                .build()
                .unwrap(),
            Location::new(Some(file.to_string()), line, line),
        );
        todo.completion_date = Some(completion_date.and_hms_opt(0, 0, 0).unwrap().and_utc());
        todo
    }

    #[test]
    fn test_write_raw_active_todo() {
        let todos = vec![make_todo(1, 'A', "Test todo", "src/main.rs", 10)];

        let mut buf = Vec::new();
        write_raw(&mut buf, &todos).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(!output.contains("[ ]"), "active-only list should not show checkbox column");
        assert!(output.contains("#1"));
        assert!(output.contains("(A)"));
        assert!(output.contains("Test todo"));
        assert!(output.contains("src/main.rs:10"));
    }

    #[test]
    fn test_write_raw_completed_todo() {
        let todos = vec![make_completed_todo(
            2,
            'B',
            "Completed task",
            "src/lib.rs",
            20,
            chrono::NaiveDate::from_ymd_opt(2026, 4, 15).unwrap(),
        )];

        let mut buf = Vec::new();
        write_raw(&mut buf, &todos).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(output.contains("[x]"), "completed todo should show [x]");
        assert!(output.contains("#2"));
        assert!(output.contains("(B)"));
        assert!(output.contains("Completed task"));
    }

    #[test]
    fn test_write_raw_mixed_todos() {
        let todos = vec![
            make_todo(1, 'A', "Active task", "src/a.rs", 1),
            make_completed_todo(
                2,
                'B',
                "Done task",
                "src/b.rs",
                2,
                chrono::NaiveDate::from_ymd_opt(2026, 4, 10).unwrap(),
            ),
        ];

        let mut buf = Vec::new();
        write_raw(&mut buf, &todos).unwrap();
        let output = String::from_utf8(buf).unwrap();

        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("[ ]"), "first todo should be active");
        assert!(lines[1].contains("[x]"), "second todo should be completed");
    }

    #[test]
    fn test_write_json_includes_dates() {
        let mut active = Todo::new(
            TodoInfoBuilder::default()
                .id(Some(TodoIdentifier::Primary(1)))
                .priority(Some('A'))
                .title("Active".to_string())
                .build()
                .unwrap(),
            Location::new(Some("src/main.rs".to_string()), 10, 10),
        );
        active.creation_date = Some(
            chrono::NaiveDate::from_ymd_opt(2026, 4, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc(),
        );

        let mut completed = Todo::new(
            TodoInfoBuilder::default()
                .id(Some(TodoIdentifier::Primary(2)))
                .priority(Some('B'))
                .title("Completed".to_string())
                .build()
                .unwrap(),
            Location::new(Some("src/lib.rs".to_string()), 20, 20),
        );
        completed.creation_date = Some(
            chrono::NaiveDate::from_ymd_opt(2026, 3, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc(),
        );
        completed.completion_date = Some(
            chrono::NaiveDate::from_ymd_opt(2026, 4, 15)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc(),
        );

        let todos = vec![active, completed];

        let mut buf = Vec::new();
        write_json(&mut buf, &todos).unwrap();
        let output = String::from_utf8(buf).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

        // First todo: active with creation_date, no completion_date
        assert_eq!(parsed[0]["id"], 1);
        assert_eq!(parsed[0]["creation_date"], "2026-04-01 00:00:00 UTC");
        assert!(parsed[0]["completion_date"].is_null());

        // Second todo: completed with both dates
        assert_eq!(parsed[1]["id"], 2);
        assert_eq!(parsed[1]["creation_date"], "2026-03-01 00:00:00 UTC");
        assert_eq!(parsed[1]["completion_date"], "2026-04-15 00:00:00 UTC");
    }

    #[test]
    fn test_write_raw_empty_list() {
        let todos: Vec<Todo> = vec![];

        let mut buf = Vec::new();
        write_raw(&mut buf, &todos).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(output.is_empty());
    }

    #[test]
    fn test_write_json_empty_list() {
        let todos: Vec<Todo> = vec![];

        let mut buf = Vec::new();
        write_json(&mut buf, &todos).unwrap();
        let output = String::from_utf8(buf).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 0);
    }
}
