use todoozy::todo::filter;
use todoozy::todo::sort;

pub struct Args {
    pub exclude: Vec<String>,
    pub filter: Option<Box<dyn filter::Filter>>,
    pub sorter: Option<Box<dyn sort::Sorter>>,
}

impl Args {
    pub fn new() -> Args {
        Args {
            exclude: Vec::new(),
            filter: None,
            sorter: None,
        }
    }
}

pub fn parse_args() -> Result<Args, lexopt::Error> {
    use lexopt::prelude::*;

    let mut args = Args::new();
    let mut list_projects = false;
    let mut list_contexts = false;

    let mut parser = lexopt::Parser::from_env();

    while let Some(arg) = parser.next()? {
        match arg {
            // TODO (Z) 2024-08-05 Implement a .tdzignore file +idea
            //
            // This would allow users to specify a list of directories or files to exclude without
            // having to pass them as arguments in every tdz call.
            //
            // Unsure if we need exclude atm, now that the todo comment parsing logic is tighter.
            // Needs more data from use in the field!
            Short('E') | Long("exclude") => {
                let e: String = parser.value()?.parse()?;
                args.exclude
                    .append(&mut e.split(',').map(String::from).collect());
            }

            Short('f') | Long("filter") => {
                args.filter = match filter::parse_str(parser.value()?.parse()?) {
                    Ok(f) => Some(f),
                    Err(e) => panic!("{}", e),
                };
            }

            Short('s') | Long("sort") => {
                args.sorter = match sort::parse_str(parser.value()?.parse()?) {
                    Ok(s) => Some(s),
                    Err(e) => panic!("{}", e),
                };
            }

            Long("list-projects") => {
                list_projects = true;
            }

            Long("list-contexts") => {
                list_contexts = true;
            }

            Long("help") => {
                println!("Usage: hello [-E|--exclude=PATH[,PATH]]");
                std::process::exit(0);
            }
            _ => return Err(arg.unexpected()),
        }
    }

    // TODO (Z) 2024-09-04 These probably shouldn't live here. +refactor
    //
    // My take is that this function should only be parsing args. Deciding what to do with them is
    // downstream's problem.
    if list_projects {
        super::list_projects(&args.exclude);
        std::process::exit(0);
    }

    if list_contexts {
        super::list_contexts(&args.exclude);
        std::process::exit(0);
    }

    Ok(args)
}
