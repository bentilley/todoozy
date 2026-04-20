use std::error;
use std::process::ExitCode;

mod cli;

// TODO #11 (Z) 2024-09-04 Sync with external project management tools +feature
//
// Philosophy: Maybe if we manage to sync this data with external project management tools, this is
// somewhere where we would expect them to pick up the slack, rather that making this a full
// project management software stack.
//
// A few useful tools like listing tags feels fine, but slicing and dicing the
// todo metadata feels like too much.
fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode, Box<dyn error::Error>> {
    use cli::args::Command::*;
    use cli::args::Mode::*;
    use cli::tag::TagCommand;
    use cli::todo::TodoCommand;

    match cli::args::parse_args(lexopt::Parser::from_env()) {
        Ok(Help(text)) => {
            println!("{}", text);
            Ok(ExitCode::SUCCESS)
        }
        Ok(Cli(cmd)) => {
            let mut config = cli::config::Config::load_config()?;
            match cmd {
                Lint(ref opts) => cli::lint::lint(&config, opts),
                Todo(TodoCommand::List(ref opts)) => cli::todo::list(&config, opts),
                Todo(TodoCommand::Get(ref opts)) => cli::todo::get(&config, opts),
                Todo(TodoCommand::Import(ref opts)) => cli::todo::import(&mut config, opts),
                Todo(TodoCommand::Edit(ref opts)) => cli::todo::edit(&config, opts),
                Todo(TodoCommand::Remove(ref opts)) => cli::todo::remove(&config, opts),
                Tag(TagCommand::List(ref opts)) => cli::tag::list(&config, opts),
            }
        }
        Ok(TUI(mut args)) => {
            let mut config = cli::config::Config::load_config()?;
            args.apply(&mut config);
            cli::tui::run(config)?;
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => Err(e),
    }
}
