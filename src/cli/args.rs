pub struct Args {
    pub exclude: Vec<String>,
}

/* TDZ (D) 2024-08-05 Implement a .tdzignore file +ideas
 *
 * This would allow users to specify a list of directories or files to exclude without having to
 * pass them as arguments in every tdz call.
 * ZDT */
pub fn parse_args() -> Result<Args, lexopt::Error> {
    use lexopt::prelude::*;

    let mut exclude: Vec<String> = Vec::new();

    let mut parser = lexopt::Parser::from_env();

    while let Some(arg) = parser.next()? {
        match arg {
            Short('E') | Long("exclude") => {
                let e: String = parser.value()?.parse()?;
                exclude.append(&mut e.split(',').map(String::from).collect());
            }
            // Long("shout") => {
            //     shout = true;
            // }
            // Value(val) if thing.is_none() => {
            //     thing = Some(val.string()?);
            // }
            Long("help") => {
                println!("Usage: hello [-E|--exclude=PATH[,PATH]]");
                std::process::exit(0);
            }
            _ => return Err(arg.unexpected()),
        }
    }

    Ok(Args { exclude })
}
