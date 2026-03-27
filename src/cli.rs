pub mod args;
pub mod config;
pub mod todo;
pub mod tui;

use self::todo::TodoCommand;

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
