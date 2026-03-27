pub mod args;
pub mod config;
pub mod todo;
pub mod tui;

use self::todo::TodoCommand;

pub enum Command {
    ImportAll,
    Todo(TodoCommand),
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
