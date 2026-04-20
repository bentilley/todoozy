use super::args::Mode;
use super::config;
use super::error;
use super::todo::OutputFormat;
use std::collections::HashMap;
use std::process::ExitCode;
use todoozy::provider::{FileSystemProvider, Provider};

pub const USAGE: &str = r#"Show summary statistics for todos

Usage: tdz summary [OPTIONS]

Options:
    --format <FORMAT>   Output format: raw, json (default: raw)
    --help              Print help
"#;

pub struct SummaryOptions {
    pub format: OutputFormat,
}

impl Default for SummaryOptions {
    fn default() -> Self {
        Self {
            format: OutputFormat::Raw,
        }
    }
}

pub fn parse_opts(mut parser: lexopt::Parser) -> error::Result<Mode> {
    use super::args::Command;
    use lexopt::prelude::*;

    let mut opts = SummaryOptions::default();

    while let Some(arg) = parser.next()? {
        match arg {
            Long("format") => opts.format = parser.value()?.parse()?,
            Long("help") => return Ok(Mode::Help(USAGE)),
            _ => return Err(arg.unexpected().into()),
        }
    }

    Ok(Mode::Cli(Command::Summary(opts)))
}

pub fn summary(conf: &config::Config, opts: &SummaryOptions) -> error::Result<ExitCode> {
    let todos =
        FileSystemProvider::new(&conf.get_todo_token(), conf.exclude.clone()).get_todos()?;

    let stats = SummaryStats::from_todos(todos.iter());

    match opts.format {
        OutputFormat::Raw => write_raw(&mut std::io::stdout(), &stats)?,
        OutputFormat::Json => write_json(&mut std::io::stdout(), &stats)?,
    }

    Ok(ExitCode::SUCCESS)
}

struct SummaryStats {
    total: usize,
    tracked: usize,
    untracked: usize,
    by_priority: Vec<(Option<char>, usize)>,
    by_tag: Vec<(String, usize)>,
}

impl SummaryStats {
    fn from_todos<'a>(todos: impl Iterator<Item = &'a todoozy::todo::Todo>) -> Self {
        let mut total = 0;
        let mut tracked = 0;
        let mut untracked = 0;
        let mut priority_counts: HashMap<Option<char>, usize> = HashMap::new();
        let mut tag_counts: HashMap<String, usize> = HashMap::new();

        for todo in todos {
            total += 1;

            if todo.id.is_some() {
                tracked += 1;
            } else {
                untracked += 1;
            }

            *priority_counts.entry(todo.priority).or_insert(0) += 1;

            for tag in &todo.tags {
                *tag_counts.entry(tag.clone()).or_insert(0) += 1;
            }
        }

        // Sort priorities: A-Z first, then None (no priority)
        let mut by_priority: Vec<_> = priority_counts.into_iter().collect();
        by_priority.sort_by(|a, b| match (&a.0, &b.0) {
            (Some(pa), Some(pb)) => pa.cmp(pb),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });

        // Sort tags by count (descending), then name (ascending)
        let mut by_tag: Vec<_> = tag_counts.into_iter().collect();
        by_tag.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        SummaryStats {
            total,
            tracked,
            untracked,
            by_priority,
            by_tag,
        }
    }
}

fn write_raw(w: &mut impl std::io::Write, stats: &SummaryStats) -> std::io::Result<()> {
    writeln!(w, "Summary")?;
    writeln!(w, "=======")?;
    writeln!(w, "Total todos: {}", stats.total)?;

    if stats.total > 0 {
        let tracked_pct = (stats.tracked as f64 / stats.total as f64 * 100.0).round() as u32;
        let untracked_pct = (stats.untracked as f64 / stats.total as f64 * 100.0).round() as u32;
        writeln!(w, "  Tracked:   {:3} ({}%)", stats.tracked, tracked_pct)?;
        writeln!(w, "  Untracked: {:3} ({}%)", stats.untracked, untracked_pct)?;
    }

    if !stats.by_priority.is_empty() {
        writeln!(w)?;
        writeln!(w, "By Priority")?;
        writeln!(w, "-----------")?;

        // Calculate width for count alignment
        let max_count = stats.by_priority.iter().map(|(_, c)| *c).max().unwrap_or(0);
        let count_width = max_count.to_string().len();

        for (priority, count) in &stats.by_priority {
            let label = match priority {
                Some(p) => format!("({})", p),
                None => " - ".to_string(),
            };
            writeln!(w, "{:>3}  {:>width$}", label, count, width = count_width)?;
        }
    }

    if !stats.by_tag.is_empty() {
        writeln!(w)?;
        writeln!(w, "By Tag")?;
        writeln!(w, "------")?;

        // Calculate widths for alignment
        let tag_width = stats.by_tag.iter().map(|(t, _)| t.len()).max().unwrap_or(0);
        let max_count = stats.by_tag.iter().map(|(_, c)| *c).max().unwrap_or(0);
        let count_width = max_count.to_string().len();

        for (tag, count) in &stats.by_tag {
            writeln!(
                w,
                "{:<tag_width$}  {:>count_width$}",
                tag,
                count,
                tag_width = tag_width,
                count_width = count_width
            )?;
        }
    }

    Ok(())
}

/// JSON output structures
#[derive(serde::Serialize)]
struct SummaryOutput {
    total: usize,
    tracked: usize,
    untracked: usize,
    by_priority: Vec<PriorityCount>,
    by_tag: Vec<TagCount>,
}

#[derive(serde::Serialize)]
struct PriorityCount {
    priority: Option<char>,
    count: usize,
}

#[derive(serde::Serialize)]
struct TagCount {
    tag: String,
    count: usize,
}

impl From<&SummaryStats> for SummaryOutput {
    fn from(stats: &SummaryStats) -> Self {
        SummaryOutput {
            total: stats.total,
            tracked: stats.tracked,
            untracked: stats.untracked,
            by_priority: stats
                .by_priority
                .iter()
                .map(|(p, c)| PriorityCount {
                    priority: *p,
                    count: *c,
                })
                .collect(),
            by_tag: stats
                .by_tag
                .iter()
                .map(|(t, c)| TagCount {
                    tag: t.clone(),
                    count: *c,
                })
                .collect(),
        }
    }
}

fn write_json(w: &mut impl std::io::Write, stats: &SummaryStats) -> std::io::Result<()> {
    let output = SummaryOutput::from(stats);
    serde_json::to_writer_pretty(&mut *w, &output)?;
    writeln!(w)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use todoozy::todo::{Location, Todo, TodoIdentifier, TodoInfoBuilder};

    fn make_todo(id: Option<u32>, priority: Option<char>, tags: Vec<&str>) -> Todo {
        Todo::new(
            TodoInfoBuilder::default()
                .id(id.map(TodoIdentifier::Primary))
                .priority(priority)
                .title("Test todo".to_string())
                .tags(tags.into_iter().map(String::from).collect())
                .build()
                .unwrap(),
            Location::new(Some("test.rs".to_string()), 1, 1),
        )
    }

    #[test]
    fn parse_opts_no_args() {
        let parser = lexopt::Parser::from_iter(["dummy"]);
        let result = parse_opts(parser);
        if let Ok(Mode::Cli(super::super::args::Command::Summary(opts))) = result {
            assert_eq!(opts.format, OutputFormat::Raw);
        } else {
            panic!("expected Ok(Cli(Summary))");
        }
    }

    #[test]
    fn parse_opts_format_json() {
        let parser = lexopt::Parser::from_iter(["dummy", "--format", "json"]);
        let result = parse_opts(parser);
        if let Ok(Mode::Cli(super::super::args::Command::Summary(opts))) = result {
            assert_eq!(opts.format, OutputFormat::Json);
        } else {
            panic!("expected Ok(Cli(Summary))");
        }
    }

    #[test]
    fn parse_opts_help_flag() {
        let parser = lexopt::Parser::from_iter(["dummy", "--help"]);
        let result = parse_opts(parser);
        assert!(matches!(result, Ok(Mode::Help(_))));
    }

    #[test]
    fn summary_stats_empty() {
        let todos: Vec<Todo> = vec![];
        let stats = SummaryStats::from_todos(todos.iter());

        assert_eq!(stats.total, 0);
        assert_eq!(stats.tracked, 0);
        assert_eq!(stats.untracked, 0);
        assert!(stats.by_priority.is_empty());
        assert!(stats.by_tag.is_empty());
    }

    #[test]
    fn summary_stats_tracked_untracked() {
        let todos = vec![
            make_todo(Some(1), Some('A'), vec![]),
            make_todo(Some(2), Some('B'), vec![]),
            make_todo(None, Some('C'), vec![]),
        ];
        let stats = SummaryStats::from_todos(todos.iter());

        assert_eq!(stats.total, 3);
        assert_eq!(stats.tracked, 2);
        assert_eq!(stats.untracked, 1);
    }

    #[test]
    fn summary_stats_priorities_sorted() {
        let todos = vec![
            make_todo(Some(1), Some('C'), vec![]),
            make_todo(Some(2), Some('A'), vec![]),
            make_todo(Some(3), None, vec![]),
            make_todo(Some(4), Some('B'), vec![]),
        ];
        let stats = SummaryStats::from_todos(todos.iter());

        // Should be sorted: A, B, C, None
        assert_eq!(stats.by_priority.len(), 4);
        assert_eq!(stats.by_priority[0], (Some('A'), 1));
        assert_eq!(stats.by_priority[1], (Some('B'), 1));
        assert_eq!(stats.by_priority[2], (Some('C'), 1));
        assert_eq!(stats.by_priority[3], (None, 1));
    }

    #[test]
    fn summary_stats_tags_sorted_by_count() {
        let todos = vec![
            make_todo(Some(1), Some('A'), vec!["cli", "feature"]),
            make_todo(Some(2), Some('B'), vec!["cli", "bug"]),
            make_todo(Some(3), Some('C'), vec!["cli"]),
        ];
        let stats = SummaryStats::from_todos(todos.iter());

        // cli: 3, feature: 1, bug: 1
        // Sorted by count desc, then name asc
        assert_eq!(stats.by_tag.len(), 3);
        assert_eq!(stats.by_tag[0], ("cli".to_string(), 3));
        assert_eq!(stats.by_tag[1], ("bug".to_string(), 1));
        assert_eq!(stats.by_tag[2], ("feature".to_string(), 1));
    }

    #[test]
    fn write_raw_output() {
        let todos = vec![
            make_todo(Some(1), Some('A'), vec!["cli"]),
            make_todo(Some(2), Some('A'), vec!["cli"]),
            make_todo(None, Some('B'), vec!["bug"]),
        ];
        let stats = SummaryStats::from_todos(todos.iter());

        let mut buf = Vec::new();
        write_raw(&mut buf, &stats).unwrap();
        let output = String::from_utf8(buf).unwrap();

        assert!(output.contains("Summary"));
        assert!(output.contains("Total todos: 3"));
        assert!(output.contains("Tracked:"));
        assert!(output.contains("Untracked:"));
        assert!(output.contains("By Priority"));
        assert!(output.contains("(A)"));
        assert!(output.contains("By Tag"));
        assert!(output.contains("cli"));
    }

    #[test]
    fn write_json_output() {
        let todos = vec![
            make_todo(Some(1), Some('A'), vec!["cli"]),
            make_todo(None, Some('B'), vec!["bug"]),
        ];
        let stats = SummaryStats::from_todos(todos.iter());

        let mut buf = Vec::new();
        write_json(&mut buf, &stats).unwrap();
        let output = String::from_utf8(buf).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["total"], 2);
        assert_eq!(parsed["tracked"], 1);
        assert_eq!(parsed["untracked"], 1);
        assert!(parsed["by_priority"].is_array());
        assert!(parsed["by_tag"].is_array());
    }
}
