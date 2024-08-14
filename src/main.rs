mod args;

fn main() {
    let args = args::parse_args().unwrap();

    let mut exclude = ignore::overrides::OverrideBuilder::new("./");
    exclude.add("!.git/").unwrap();

    for e in args.exclude {
        exclude.add(&format!("!{}", e)).unwrap();
    }

    let walk = ignore::WalkBuilder::new("./")
        .hidden(false)
        .overrides(exclude.build().unwrap())
        .build();

    let mut todos = Vec::<todoozy::Todo>::new();

    for results in walk {
        match results {
            Ok(entry) => {
                /* TDZ (C) 2024-08-02 Handle this unwrap error +ErrHandling
                ZDT */
                if entry.file_type().unwrap().is_dir() {
                    continue;
                }

                let file_path = entry.path().to_str().unwrap();
                todos.append(&mut todoozy::parse_file(file_path));
            }
            Err(err) => eprintln!("Error: {}", err),
        }
    }

    for t in todos {
        println!("{:?}\n", t);
        // match t.file {
        //     Some(ref file) => match t.line_number {
        //         Some(line_number) => println!("{}:{}: {}", file, line_number, t.title),
        //         None => println!("{}: {}", file, t.title),
        //     },
        //     None => println!("{}", t.title),
        // }
        // match t.description {
        //     Some(ref description) => println!("{}", description),
        //     None => (),
        // }
    }
}
