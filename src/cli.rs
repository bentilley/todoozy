pub mod args;
pub mod config;
pub mod display;
pub mod tui;

pub enum TodoCommand {
    List,
}

pub enum Command {
    ListProjects,
    ListContexts,
    ImportAll,
    Todo(TodoCommand),
}

pub fn list_projects(exclude: &[String]) {
    let todos = todoozy::get_todos(exclude).unwrap();
    let mut projects = std::collections::HashMap::new();
    for todo in todos {
        for project in todo.projects {
            let count = projects.entry(project).or_insert(0);
            *count += 1;
        }
    }
    for (project, _) in projects {
        println!("{}", project);
    }
}

pub fn list_contexts(exclude: &[String]) {
    let todos = todoozy::get_todos(exclude).unwrap();
    let mut contexts = std::collections::HashMap::new();
    for todo in todos {
        for context in todo.contexts {
            let count = contexts.entry(context).or_insert(0);
            *count += 1;
        }
    }
    for (context, _) in contexts {
        println!("{}", context);
    }
}

pub fn import_all(conf: &mut config::Config) -> Result<(), Box<dyn std::error::Error>> {
    let todos = todoozy::get_todos(&conf.exclude).unwrap();
    for mut todo in todos {
        match todo.id {
            Some(_) => {}
            None => {
                conf.num_todos += 1;
                let id = conf.num_todos;
                todo.id = Some(todoozy::todo::TodoIdentifier::Primary(id));
                todo.write_id()?;
                println!("Imported: #{} {}", id, todo.title);
            }
        };
    }
    conf.save()?;
    Ok(())
}

pub fn todo_list(conf: &config::Config) {
    let mut todos = match todoozy::get_todos(&conf.exclude) {
        Ok(todos) => todos,
        Err(e) => {
            eprintln!("Error loading todos: {}", e);
            return;
        }
    };

    if let Some(ref filter) = conf.filter {
        todos.apply_filter(|todo| filter.filter(todo));
    }

    if let Some(ref sorter) = conf.sorter {
        todos.apply_sort(|a, b| sorter.compare(a, b));
    }

    let all_todos: Vec<_> = todos.into();

    let id_width = all_todos
        .iter()
        .map(|t| t.display_id().len())
        .max()
        .unwrap_or(0);

    let location_width = all_todos
        .iter()
        .map(|t| t.display_location_start().len())
        .max()
        .unwrap_or(0);

    for todo in all_todos {
        println!(
            "{:<id_width$} {} {:<location_width$} {}",
            todo.display_id(),
            todo.display_priority(),
            todo.display_location_start(),
            todo.display_title(),
        )
    }
}
