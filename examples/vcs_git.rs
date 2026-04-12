// Example: Run git VCS backend on the current repository
//
// Usage: cargo run --example vcs_git

use std::env;
use todoozy::provider::vcs::git::GitBackend;
use todoozy::provider::vcs::VcsBackend;

fn main() {
    let args: Vec<String> = env::args().collect();
    dbg!(&args);

    let path = if args.len() >= 2 {
        std::path::PathBuf::from(args[1].clone())
    } else {
        env::current_dir().expect("failed to get current dir")
    };

    let history_start = if args.len() >= 3 {
        Some(args[2].clone())
    } else {
        None
    };

    println!("Creating Git backend for path: {}", path.display());
    let provider = match GitBackend::new(&path, "TODO", history_start) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    println!("Scanning git history for TODOs");

    let todos = match provider.get_all_todos() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error scanning TODOs: {}", e);
            std::process::exit(1);
        }
    };

    println!("Found {} TODOs:\n", todos.len());

    // Sort by ID
    let todos = todos.into_sorted(|a, b| {
        let id_a = a.id.as_ref().map_or(1000u32, |id| match id {
            todoozy::todo::TodoIdentifier::Primary(n) => *n,
            todoozy::todo::TodoIdentifier::Reference(_) => 100,
        });
        let id_b = b.id.as_ref().map_or(1000u32, |id| match id {
            todoozy::todo::TodoIdentifier::Primary(n) => *n,
            todoozy::todo::TodoIdentifier::Reference(_) => 100,
        });
        id_a.cmp(&id_b)
    });

    for todo in todos {
        println!("{}", todo);
    }
}
