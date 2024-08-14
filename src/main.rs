use ratatui::{
    backend::CrosstermBackend,
    crossterm::{
        // event::{self, Event, KeyCode},
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
        ExecutableCommand,
    },
    // widgets::{Block, List, Paragraph},
    // Frame, Terminal,
    Terminal,
};
use std::io::{self, stdout};

mod cli;

fn main() {
    let args = cli::args::parse_args().unwrap();

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

    // for t in todos {
    //     println!("{:?}\n", t);
    //     // match t.file {
    //     //     Some(ref file) => match t.line_number {
    //     //         Some(line_number) => println!("{}:{}: {}", file, line_number, t.title),
    //     //         None => println!("{}: {}", file, t.title),
    //     //     },
    //     //     None => println!("{}", t.title),
    //     // }
    //     // match t.description {
    //     //     Some(ref description) => println!("{}", description),
    //     //     None => (),
    //     // }
    // }

    let _ = draw(todos);
}

fn draw(todos: Vec<todoozy::Todo>) -> io::Result<()> {
    // tui::init_error_hooks()?;
    // let terminal = tui::init_terminal()?;

    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let mut app = cli::app::App::new(&todos);
    app.run(terminal)?;

    // let mut should_quit = false;
    // while !should_quit {
    //     terminal.draw(ui)?;
    //     should_quit = handle_events()?;
    // }

    // tui::restore_terminal()?;
    // Ok(())

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

// fn handle_events() -> io::Result<bool> {
//     if event::poll(std::time::Duration::from_millis(50))? {
//         if let Event::Key(key) = event::read()? {
//             if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Char('q') {
//                 return Ok(true);
//             }
//         }
//     }
//     Ok(false)
// }

// fn ui(frame: &mut Frame) {
//     let list = List::new(todos)
//         .block(Block::bordered().title("List"))
//         .style(Style::default().fg(Color::White))
//         .highlight_style(Style::default().add_modifier(Modifier::ITALIC))
//         .highlight_symbol(">>")
//         .repeat_highlight_symbol(true)
//         .direction(ListDirection::BottomToTop);

//     frame.render_widget(
//         Paragraph::new("Hello World!").block(Block::bordered().title("Greeting")),
//         frame.area(),
//     );
// }
