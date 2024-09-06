use crate::todo::{Todo, TodoBuilder};
use nom::{
    branch::alt,
    bytes::complete::{is_not, tag, take},
    character::complete::{
        alphanumeric1, digit1, line_ending, multispace1, one_of, space0, space1,
    },
    combinator::opt,
    error::{Error, ErrorKind},
    multi::{fold_many1, many0},
    sequence::{delimited, preceded, terminated, tuple},
    IResult, InputTakeAtPosition,
};

fn id(i: &str) -> IResult<&str, u32> {
    let (i, _) = space0(i)?;
    let (i, p) = delimited(tag("#"), digit1, space1)(i)?;
    Ok((i, p.parse().unwrap()))
}

#[test]
fn test_id() {
    assert_eq!(id("#123 "), Ok(("", 123)));
    assert_eq!(
        id("#123"),
        Err(nom::Err::Error(Error::new("", ErrorKind::Space)))
    );
    assert_eq!(
        id("123"),
        Err(nom::Err::Error(Error::new("123", ErrorKind::Tag)))
    );
}

fn uppercase(i: &str) -> IResult<&str, char> {
    let chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let (i, c) = one_of(chars)(i)?;
    Ok((i, c))
}

#[test]
fn test_uppercase() {
    assert_eq!(uppercase("A"), Ok(("", 'A')));
    assert_eq!(
        uppercase("a"),
        Err(nom::Err::Error(Error::new("a", ErrorKind::OneOf)))
    );
    let (i, c) = uppercase("ABC").unwrap();
    assert_eq!(i, "BC");
    assert_eq!(c, 'A')
}

fn priority(i: &str) -> IResult<&str, char> {
    terminated(delimited(tag("("), uppercase, tag(")")), multispace1)(i)
}

#[test]
fn test_priority() {
    assert_eq!(priority("(A) "), Ok(("", 'A')));
    assert_eq!(
        priority("(a)"),
        Err(nom::Err::Error(Error::new("a)", ErrorKind::OneOf)))
    );
    assert_eq!(
        priority("(A]"),
        Err(nom::Err::Error(Error::new("]", ErrorKind::Tag)))
    );
}

fn date_fmt(i: &str) -> IResult<&str, chrono::NaiveDate> {
    let (i, year) = take(4usize)(i)?;
    let (i, _) = tag("-")(i)?;
    let (i, month) = take(2usize)(i)?;
    let (i, _) = tag("-")(i)?;
    let (i, day) = take(2usize)(i)?;

    match chrono::NaiveDate::from_ymd_opt(
        year.parse().unwrap(),
        month.parse().unwrap(),
        day.parse().unwrap(),
    ) {
        Some(date) => Ok((i, date)),
        None => Err(nom::Err::Error(Error::new(
            "invalid date",
            ErrorKind::Satisfy,
        ))),
    }
}

#[test]
fn test_date_fmt() {
    assert_eq!(
        date_fmt("2024-08-05"),
        Ok(("", chrono::NaiveDate::from_ymd_opt(2024, 8, 5).unwrap()))
    );
    assert_eq!(
        date_fmt("2024-08-32"),
        Err(nom::Err::Error(Error::new(
            "invalid date",
            ErrorKind::Satisfy
        )))
    );
}

fn date(i: &str) -> IResult<&str, chrono::NaiveDate> {
    terminated(date_fmt, multispace1)(i)
}

#[test]
fn test_date() {
    assert_eq!(
        date("2024-08-05 "),
        Ok(("", chrono::NaiveDate::from_ymd_opt(2024, 8, 5).unwrap()))
    );
    assert_eq!(
        date("2024-08-32"),
        Err(nom::Err::Error(Error::new(
            "invalid date",
            ErrorKind::Satisfy
        )))
    );
}

#[derive(Debug, PartialEq)]
enum Word {
    Context(String),
    Project(String),
    Metadata((String, String)),
    Plain(String),
}

fn project(i: &str) -> IResult<&str, Word> {
    let (i, _) = space0(i)?;
    let (i, p) = preceded(tag("+"), alphanumeric1)(i)?;
    Ok((i, Word::Project(p.to_string())))
}

#[test]
fn test_project() {
    assert_eq!(
        project("+test"),
        Ok(("", Word::Project("test".to_string())))
    );
    assert_eq!(
        project("+test something"),
        Ok((" something", Word::Project("test".to_string())))
    );
    assert_eq!(
        project("test"),
        Err(nom::Err::Error(Error::new("test", ErrorKind::Tag)))
    );
}

fn context(i: &str) -> IResult<&str, Word> {
    let (i, _) = space0(i)?;
    let (i, c) = preceded(tag("@"), alphanumeric1)(i)?;
    Ok((i, Word::Context(c.to_string())))
}

#[test]
fn test_context() {
    assert_eq!(
        context("@test"),
        Ok(("", Word::Context("test".to_string())))
    );
    assert_eq!(
        context("@test something"),
        Ok((" something", Word::Context("test".to_string())))
    );
    assert_eq!(
        context("test"),
        Err(nom::Err::Error(Error::new("test", ErrorKind::Tag)))
    );
    assert_eq!(
        context("hello@example.com"),
        Err(nom::Err::Error(Error::new(
            "hello@example.com",
            ErrorKind::Tag
        )))
    );
}

fn non_whitespace(input: &str) -> IResult<&str, &str> {
    input.split_at_position1_complete(char::is_whitespace, ErrorKind::Alpha)
}

#[test]
fn test_non_whitespace() {
    assert_eq!(non_whitespace("Hello, World!"), Ok((" World!", "Hello,")));
    assert_eq!(
        non_whitespace(" Hello, World!"),
        Err(nom::Err::Error(Error::new(
            " Hello, World!",
            ErrorKind::Alpha
        )))
    );
}

// TODO #3 (C) 2024-09-06 Meta data parsing interferes with code in todos +bug
//
// This code won't all be parsed correctly because it contains ':' characters which immediately
// flip the parser into metadata munching.
//
// Span::styled(
//     format!("#{} ", todo_item.todo.id.unwrap_or(0)),
//     Style::new().fg(Color::Red),
// ),
//
// Not sure what the solution is yet, as lots of languages use ':' in their syntax so taking it on
// a case by case basis feels impossible. I think we'd need to specify markdown ` or ``` always
// required for code.
//
// This will also apply to + and @ symbols. If those appeared in code in a todo, they would also be
// parsed as projects and contexts. Maybe we need a way to escape their usage to use literal
// characters in the todo without them being parsed specifically.
fn metadata(i: &str) -> IResult<&str, Word> {
    let (i, _) = space0(i)?;
    let (i, key) = is_not(": \t\r\n")(i)?;
    let (i, _) = tag(":")(i)?;
    let (i, value) = non_whitespace(i)?;
    Ok((i, Word::Metadata((key.to_string(), value.to_string()))))
}

#[test]
fn test_metadata() {
    assert_eq!(
        metadata("key:value"),
        Ok(("", Word::Metadata(("key".to_string(), "value".to_string()))))
    );
    assert_eq!(
        metadata("9@$,,afs&*:()()23,;."),
        Ok((
            "",
            Word::Metadata(("9@$,,afs&*".to_string(), "()()23,;.".to_string()))
        ))
    );
    assert_eq!(
        metadata("key: value"),
        Err(nom::Err::Error(Error::new(" value", ErrorKind::Alpha)))
    );
    assert_eq!(
        metadata("key value"),
        Err(nom::Err::Error(Error::new(" value", ErrorKind::Tag)))
    );
}

fn plain(i: &str) -> IResult<&str, Word> {
    let (i, (ws, p)) = tuple((space0, is_not(" \t\r\n")))(i)?;
    Ok((i, Word::Plain(ws.to_string() + p)))
}

#[test]
fn test_plain() {
    assert_eq!(
        plain("Hello, World!"),
        Ok((" World!", Word::Plain("Hello,".to_string())))
    );
    assert_eq!(
        plain(" Hello, World!"),
        Ok((" World!", Word::Plain(" Hello,".to_string())))
    );
    assert_eq!(
        plain("Sometimes\tnothing"),
        Ok(("\tnothing", Word::Plain("Sometimes".to_string())))
    );
    assert_eq!(
        plain("Nospace"),
        Ok(("", Word::Plain("Nospace".to_string())))
    );
    assert_eq!(
        plain(""),
        Err(nom::Err::Error(Error::new("", ErrorKind::IsNot)))
    );
}

fn word(i: &str) -> IResult<&str, Word> {
    alt((context, project, metadata, plain))(i)
}

#[test]
fn test_word() {
    assert_eq!(
        word("word +project"),
        Ok((" +project", Word::Plain("word".to_string())))
    );
    assert_eq!(
        word(" word @context"),
        Ok((" @context", Word::Plain(" word".to_string())))
    );
    assert_eq!(
        word("Nospace"),
        Ok(("", Word::Plain("Nospace".to_string())))
    );
    assert_eq!(
        word("+project"),
        Ok(("", Word::Project("project".to_string())))
    );
    assert_eq!(
        word(" +project"),
        Ok(("", Word::Project("project".to_string())))
    );
    assert_eq!(
        word("@context"),
        Ok(("", Word::Context("context".to_string())))
    );
    assert_eq!(
        word(" @context"),
        Ok(("", Word::Context("context".to_string())))
    );
    assert_eq!(
        word("+project word"),
        Ok((" word", Word::Project("project".to_string())))
    );
    assert_eq!(
        word("@context word"),
        Ok((" word", Word::Context("context".to_string())))
    );
    assert_eq!(
        word("key:value word"),
        Ok((
            " word",
            Word::Metadata(("key".to_string(), "value".to_string()))
        ))
    );
}

fn text(i: &str) -> IResult<&str, Vec<Word>> {
    fold_many1(word, Vec::new, |mut acc: Vec<_>, item| {
        acc.push(item);
        acc
    })(i)
}

#[test]
fn test_text() {
    assert_eq!(
        text("Hello, World!"),
        Ok((
            "",
            vec![
                Word::Plain("Hello,".to_string()),
                Word::Plain(" World!".to_string()),
            ]
        ))
    );
    assert_eq!(
        text("Hello, World! +project @context"),
        Ok((
            "",
            vec![
                Word::Plain("Hello,".to_string()),
                Word::Plain(" World!".to_string()),
                Word::Project("project".to_string()),
                Word::Context("context".to_string()),
            ]
        ))
    );
    assert_eq!(
        text("Hello, World! test:data"),
        Ok((
            "",
            vec![
                Word::Plain("Hello,".to_string()),
                Word::Plain(" World!".to_string()),
                Word::Metadata(("test".to_string(), "data".to_string())),
            ]
        ))
    );
    assert_eq!(
        text(""),
        Err(nom::Err::Error(Error::new("", ErrorKind::Many1)))
    );
}

fn text_line(i: &str) -> IResult<&str, Vec<Word>> {
    let (i, mut words) = text(i)?;
    let (i, ws) = many0(line_ending)(i)?;
    if ws.len() > 0 {
        words.push(Word::Plain(ws.join("").to_string()));
    }
    Ok((i, words))
}

#[test]
fn test_text_line() {
    assert_eq!(
        text_line("Hello, World! +project @context\n\n"),
        Ok((
            "",
            vec![
                Word::Plain("Hello,".to_string()),
                Word::Plain(" World!".to_string()),
                Word::Project("project".to_string()),
                Word::Context("context".to_string()),
                Word::Plain("\n\n".to_string()),
            ]
        ))
    );
}

fn text_multiline(i: &str) -> IResult<&str, Vec<Vec<Word>>> {
    fold_many1(text_line, Vec::new, |mut acc: Vec<_>, item| {
        acc.push(item);
        acc
    })(i)
}

#[test]
fn test_text_multiline() {
    assert_eq!(
        text_multiline("Hello, World! +project @context\n\nAnother line. meta:data"),
        Ok((
            "",
            vec![
                vec![
                    Word::Plain("Hello,".to_string()),
                    Word::Plain(" World!".to_string()),
                    Word::Project("project".to_string()),
                    Word::Context("context".to_string()),
                    Word::Plain("\n\n".to_string()),
                ],
                vec![
                    Word::Plain("Another".to_string()),
                    Word::Plain(" line.".to_string()),
                    Word::Metadata(("meta".to_string(), "data".to_string())),
                ],
            ]
        ))
    );
}

pub fn todo(s: &str) -> IResult<&str, Todo> {
    let (i, id) = opt(id)(s)?;
    let (i, priority) = opt(priority)(i)?;
    let (i, date1) = opt(date)(i)?;
    let (i, date2) = opt(date)(i)?;

    let (completion_date, creation_date) = if date2.is_none() {
        (None, date1)
    } else {
        (date1, date2)
    };

    let (i, text) = text(i)?;

    let mut title = String::new();
    let mut projects: Vec<String> = Vec::new();
    let mut contexts: Vec<String> = Vec::new();
    let mut metadata = std::collections::HashMap::new();

    for word in text {
        match word {
            Word::Plain(p) => title.push_str(&p),
            Word::Project(p) => projects.push(p),
            Word::Context(c) => contexts.push(c),
            Word::Metadata((k, v)) => {
                // Metadata keys starting with an underscore are reserved for internal use.
                if k.starts_with("_") {
                    match k.as_str() {
                        _ => {}
                    }
                } else {
                    metadata.insert(k, v);
                }
            }
        }
    }

    let (i, _) = many0(line_ending)(i)?;

    let (i, body) = opt(text_multiline)(i)?;
    let description = match body {
        Some(lines) => {
            let mut description = String::new();
            for line in lines {
                for word in line {
                    match word {
                        Word::Plain(p) => description.push_str(&p),
                        Word::Project(p) => projects.push(p.to_owned()),
                        Word::Context(c) => contexts.push(c.to_owned()),
                        Word::Metadata((k, v)) => {
                            // Metadata keys starting with an underscore are reserved for internal use.
                            if k.starts_with("_") {
                                match k.as_str() {
                                    _ => {}
                                }
                            } else {
                                metadata.insert(k, v);
                            }
                        }
                    }
                }
            }
            Some(description.trim().to_string())
        }
        None => None,
    };

    Ok((
        i,
        TodoBuilder::default()
            .id(id)
            .priority(priority)
            .completion_date(completion_date)
            .creation_date(creation_date)
            .title(title.trim().to_string())
            .description(description)
            .projects(projects)
            .contexts(contexts)
            .metadata(metadata)
            .build()
            .unwrap(),
    ))
}

#[test]
fn test_todo() {
    assert_eq!(
        todo("This is a test todo"),
        Ok((
            "",
            TodoBuilder::default()
                .title("This is a test todo".to_string())
                .build()
                .unwrap()
        ))
    );
    assert_eq!(
        todo("(A) This is a test todo"),
        Ok((
            "",
            TodoBuilder::default()
                .title("This is a test todo".to_string())
                .priority(Some('A'))
                .build()
                .unwrap()
        ))
    );
    assert_eq!(
        todo("#123 (A) This is a test todo"),
        Ok((
            "",
            TodoBuilder::default()
                .title("This is a test todo".to_string())
                .id(Some(123))
                .priority(Some('A'))
                .build()
                .unwrap()
        ))
    );
    assert_eq!(
        todo("(A) Test todo +project @context key:value"),
        Ok((
            "",
            TodoBuilder::default()
                .title("Test todo".to_string())
                .priority(Some('A'))
                .projects(vec!["project".to_string()])
                .contexts(vec!["context".to_string()])
                .metadata(
                    vec![("key".to_string(), "value".to_string())]
                        .into_iter()
                        .collect()
                )
                .build()
                .unwrap()
        ))
    );
    assert_eq!(
        todo(
            r#"(A) 2024-08-11 Test todo +project @context

This is a test todo with a description. more:data

With multiple paragraphs, and some paragraphs that contain projects. +extra"#
        ),
        Ok((
            "",
            TodoBuilder::default()
                .title("Test todo".to_string())
                .priority(Some('A'))
                .projects(vec!["project".to_string(), "extra".to_string()])
                .contexts(vec!["context".to_string()])
                .creation_date(chrono::NaiveDate::from_ymd_opt(2024, 8, 11))
                .metadata(
                    vec![("more".to_string(), "data".to_string())]
                        .into_iter()
                        .collect()
                )
                .description(Some(
                    "This is a test todo with a description.\n\nWith multiple paragraphs, and some paragraphs that contain projects."
                        .to_string()
                ))
                .build()
                .unwrap()
        ))
    );
    assert_eq!(
        todo(
            r#"(A) 2024-08-14 2024-08-11 Test todo +project @context

- Can it handle indented lines?
  - Yes, it can. +extra"#
        ),
        Ok((
            "",
            TodoBuilder::default()
                .title("Test todo".to_string())
                .priority(Some('A'))
                .projects(vec!["project".to_string(), "extra".to_string()])
                .contexts(vec!["context".to_string()])
                .creation_date(chrono::NaiveDate::from_ymd_opt(2024, 8, 11))
                .completion_date(chrono::NaiveDate::from_ymd_opt(2024, 8, 14))
                .description(Some(
                    "- Can it handle indented lines?\n  - Yes, it can.".to_string()
                ))
                .build()
                .unwrap()
        ))
    );
}
