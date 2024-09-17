use std::error;

mod cli;

// TODO #11 (Z) 2024-09-04 Sync with external project management tools +feature
//
// Philosophy: Maybe if we manage to sync this data with external project management tools, this is
// somewhere where we would expect them to pick up the slack, rather that making this a full
// project management software stack.
//
// A few useful tools like listing projects and contexts efels fine, but slicing and dicing the
// todo metadata feels like too much.
fn main() -> Result<(), Box<dyn error::Error>> {
    let mut config = cli::config::Config::load_config()?;

    let mut args = cli::args::parse_args().unwrap();
    args.apply(&mut config);

    cli::tui::run(config)
}
