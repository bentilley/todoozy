pub struct Args {
    pub exclude: Vec<String>,
    pub filter: Box<dyn todoozy::filter::Filter>,
    pub sorter: Box<dyn todoozy::sort::Sorter>,
}

impl Args {
    pub fn new() -> Args {
        Args {
            exclude: Vec::new(),
            filter: Box::new(todoozy::filter::All {}),
            sorter: Box::new(todoozy::sort::Priority {}),
        }
    }
}

// TODO (E) Add args to list available projects / contexts instead of dropping into the TUI.
//
// Essentially, some other options / tools to explore the todo data in different ways. The only way
// to view the data is via each todo atm, it would also be useful to query the project, context,
// priority data in different ways.
//
// Philosophy: Maybe if we manage to sync this data with external project management tools, this is
// somewhere where we would expect them to pick up the slack, rather that making this a full
// project management software stack.
// ODOT
pub fn parse_args() -> Result<Args, lexopt::Error> {
    use lexopt::prelude::*;

    let mut args = Args::new();

    let mut parser = lexopt::Parser::from_env();

    while let Some(arg) = parser.next()? {
        match arg {
            // TODO (Z) 2024-08-05 Implement a .tdzignore file +ideas
            //
            // This would allow users to specify a list of directories or files to exclude without
            // having to pass them as arguments in every tdz call.
            //
            // Unsure if we need exclude atm, now that the todo comment parsing logic is tighter.
            // Needs more data from use in the field!
            // ODOT
            Short('E') | Long("exclude") => {
                let e: String = parser.value()?.parse()?;
                args.exclude
                    .append(&mut e.split(',').map(String::from).collect());
            }

            // TODO (C) 2024-08-20 Refine sorting options +feature
            //
            // Add a --sort option, so you can mix and match more easily and write more complex
            // expressions. I imagine the syntax being something like 'priority > date > user',
            // meaning that the tasks should be sorted by priority first, then by date, and finally
            // by user. Maybe there is precendent for this type of syntax though in one of those
            // databases that has it's own piping query language...
            // ODOT
            Short('f') | Long("filter") => {
                args.filter = match todoozy::filter::parse_str(parser.value()?.parse()?) {
                    Ok(f) => f,
                    Err(e) => panic!("{}", e),
                };
            }

            Short('s') | Long("sort") => {
                args.sorter = todoozy::sort::parse_str(parser.value()?.parse()?);
            }

            Long("help") => {
                println!("Usage: hello [-E|--exclude=PATH[,PATH]]");
                std::process::exit(0);
            }
            _ => return Err(arg.unexpected()),
        }
    }

    Ok(args)
}
