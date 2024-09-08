use std::error;

mod cli;

// TODO (Z) 2024-09-04 Sync with external project management tools +feature
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

    // TODO (A) 2024-09-05 Give IDs to any todos without and write back to file +feature
    //
    // Maybe this should happen in the TUI code as we'll want to display a popup informing the user
    // that we're about to write some data back to their files.
    //
    // This also requires storing some state about how many todos we've seen. This needs to be done
    // in a file that is also kept under version control. I think a todoozy.yaml file that can also
    // have project config in would be the place for this. When you add new todos, todoozy will
    // number them for you and then increment the number in the file. The user then has to remember
    // to commit the todoozy.yaml file with the new todos.

    cli::tui::run(config)
}
