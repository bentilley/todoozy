pub mod args;
pub mod display;
pub mod tui;

pub fn list_projects(exclude: &[String]) {
    let todos = todoozy::get_todos(&exclude).unwrap();
    let mut projects = std::collections::HashMap::new();
    for todo in todos {
        for project in todo.projects {
            let count = projects.entry(project).or_insert(0);
            *count += 1;
        }
    }
    for (project, _) in projects {
        println!("+{}", project);
    }
}

pub fn list_contexts(exclude: &[String]) {
    let todos = todoozy::get_todos(&exclude).unwrap();
    let mut contexts = std::collections::HashMap::new();
    for todo in todos {
        for context in todo.contexts {
            let count = contexts.entry(context).or_insert(0);
            *count += 1;
        }
    }
    for (context, _) in contexts {
        println!("@{}", context);
    }
}
