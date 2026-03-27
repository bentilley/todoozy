use std::error;

mod cli;

// TODO #11 (Z) 2024-09-04 Sync with external project management tools +feature
//
// Philosophy: Maybe if we manage to sync this data with external project management tools, this is
// somewhere where we would expect them to pick up the slack, rather that making this a full
// project management software stack.
//
// A few useful tools like listing tags feels fine, but slicing and dicing the
// todo metadata feels like too much.
fn main() -> Result<(), Box<dyn error::Error>> {
    let mut config = cli::config::Config::load_config()?;

    use cli::args::Mode::*;
    use cli::tag::TagCommand;
    use cli::todo::TodoCommand;
    use cli::Command::*;
    match cli::args::parse_args(lexopt::Parser::from_env()) {
        Ok(mode) => match mode {
            Cli(Todo(TodoCommand::List(ref opts))) => Ok(cli::todo::list(&config, opts)),
            Cli(Todo(TodoCommand::Get(ref opts))) => Ok(cli::todo::get(&config, opts)),
            Cli(Todo(TodoCommand::Import(ref opts))) => {
                Ok(cli::todo::import(&mut config, opts))
            }
            Cli(Todo(TodoCommand::Edit(ref opts))) => Ok(cli::todo::edit(&config, opts)),
            Cli(Todo(TodoCommand::Remove(ref opts))) => Ok(cli::todo::remove(&config, opts)),
            Cli(Tag(TagCommand::List(ref opts))) => Ok(cli::tag::list(&config, opts)),
            TUI(mut args) => {
                args.apply(&mut config);
                cli::tui::run(config)
            }
        },
        Err(e) => Err(e.into()),
    }
}
