use std::error;

mod cli;

fn main() -> Result<(), Box<dyn error::Error>> {
    let args = cli::args::parse_args().unwrap();

    cli::tui::run(cli::tui::app::AppConfig {
        exclude: args.exclude,
        filter: args.filter,
        sorter: args.sorter,
    })
}
