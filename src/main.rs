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
    // TODO (A) 2024-09-05 Add a todoozy.yaml file for project config. +feature
    //
    // This will allow the user to set the filter and sort to be used on start up as well as keep
    // track of the todo ID number.
    let args = cli::args::parse_args().unwrap();

    // TODO (A) 2024-09-05 Give IDs to any todos without and write back to file +feature
    //
    // Maybe this should happen in the TUI code as we'll want to display a popup informing the user
    // that we're about to write some data back to their files.

    cli::tui::run(cli::tui::app::AppConfig {
        exclude: args.exclude,
        filter: args.filter,
        sorter: args.sorter,
    })
}
