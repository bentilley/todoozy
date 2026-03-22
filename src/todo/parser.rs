use crate::todo::{Metadata, Todo, TodoBuilder};
use nom::{
    branch::alt,
    bytes::complete::{is_not, tag, take, take_until},
    character::complete::{
        alphanumeric1, digit1, line_ending, multispace1, one_of, space0, space1,
    },
    combinator::{opt, recognize},
    error::{ErrorKind, ParseError},
    multi::{fold_many1, many0},
    sequence::{delimited, preceded, terminated, tuple},
    IResult, InputTakeAtPosition,
};

#[derive(Debug, PartialEq)]
pub enum Error<I> {
    BadMetadata(String),
    InvalidDate(String),
    Nom(I, ErrorKind),
}

impl<I> ParseError<I> for Error<I> {
    fn from_error_kind(input: I, kind: ErrorKind) -> Self {
        Error::Nom(input, kind)
    }

    fn append(_: I, _: ErrorKind, other: Self) -> Self {
        other
    }
}

// TODO #59 (D) 2026-03-22 Parse reference syntax `TODO &<id>` +parser +refs
//
// Add support for parsing TODO references using `&` prefix instead of `#`:
// - `TODO &43 Optional title` - reference to todo #43
// - `TODO #43 Title` - primary todo (existing behavior)
//
// References have limited fields compared to primaries:
// - Allowed: title, description, projects, contexts, metadata
// - Not allowed: priority, created_date, due_date (these belong to primary only)
//
// Parser should reject priority/dates on references with a clear error message.
// The `&` prefix evokes "address of" - the ID lives in the referenced primary.

fn id(i: &str) -> IResult<&str, u32, Error<&str>> {
    let (i, _) = space0(i)?;
    let (i, p) = delimited(tag("#"), digit1, space1)(i)?;
    Ok((i, p.parse().unwrap()))
}

#[test]
fn test_id() {
    assert_eq!(id("#123 "), Ok(("", 123)));
    assert_eq!(
        id("#123"),
        Err(nom::Err::Error(Error::Nom("", ErrorKind::Space)))
    );
    assert_eq!(
        id("123"),
        Err(nom::Err::Error(Error::Nom("123", ErrorKind::Tag)))
    );
}

fn uppercase(i: &str) -> IResult<&str, char, Error<&str>> {
    let chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let (i, c) = one_of(chars)(i)?;
    Ok((i, c))
}

#[test]
fn test_uppercase() {
    assert_eq!(uppercase("A"), Ok(("", 'A')));
    assert_eq!(
        uppercase("a"),
        Err(nom::Err::Error(Error::Nom("a", ErrorKind::OneOf)))
    );
    let (i, c) = uppercase("ABC").unwrap();
    assert_eq!(i, "BC");
    assert_eq!(c, 'A')
}

fn priority(i: &str) -> IResult<&str, char, Error<&str>> {
    terminated(delimited(tag("("), uppercase, tag(")")), multispace1)(i)
}

#[test]
fn test_priority() {
    assert_eq!(priority("(A) "), Ok(("", 'A')));
    assert_eq!(
        priority("(a)"),
        Err(nom::Err::Error(Error::Nom("a)", ErrorKind::OneOf)))
    );
    assert_eq!(
        priority("(A]"),
        Err(nom::Err::Error(Error::Nom("]", ErrorKind::Tag)))
    );
}

fn date_fmt(i: &str) -> IResult<&str, chrono::NaiveDate, Error<&str>> {
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
        None => Err(nom::Err::Error(Error::InvalidDate(format!(
            "{}-{}-{}",
            year, month, day
        )))),
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
        Err(nom::Err::Error(Error::InvalidDate(
            "2024-08-32".to_string()
        )))
    );
}

fn date(i: &str) -> IResult<&str, chrono::NaiveDate, Error<&str>> {
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
        Err(nom::Err::Error(Error::InvalidDate(
            "2024-08-32".to_string()
        )))
    );
}

#[derive(Debug, PartialEq)]
enum Word<'a> {
    Context(&'a str),
    Project(&'a str),
    Metadata((&'a str, &'a str)),
    Plain(&'a str),
    Raw(&'a str),
}

fn project(i: &str) -> IResult<&str, Word<'_>, Error<&str>> {
    let (i, _) = space0(i)?;
    let (i, p) = preceded(tag("+"), alphanumeric1)(i)?;
    Ok((i, Word::Project(p)))
}

#[test]
fn test_project() {
    assert_eq!(project("+test"), Ok(("", Word::Project("test"))));
    assert_eq!(
        project("+test something"),
        Ok((" something", Word::Project("test")))
    );
    assert_eq!(
        project("test"),
        Err(nom::Err::Error(Error::Nom("test", ErrorKind::Tag)))
    );
}

fn context(i: &str) -> IResult<&str, Word<'_>, Error<&str>> {
    let (i, _) = space0(i)?;
    let (i, c) = preceded(tag("@"), alphanumeric1)(i)?;
    Ok((i, Word::Context(c)))
}

#[test]
fn test_context() {
    assert_eq!(context("@test"), Ok(("", Word::Context("test"))));
    assert_eq!(
        context("@test something"),
        Ok((" something", Word::Context("test")))
    );
    assert_eq!(
        context("test"),
        Err(nom::Err::Error(Error::Nom("test", ErrorKind::Tag)))
    );
    assert_eq!(
        context("hello@example.com"),
        Err(nom::Err::Error(Error::Nom(
            "hello@example.com",
            ErrorKind::Tag
        )))
    );
}

fn non_whitespace(input: &str) -> IResult<&str, &str, Error<&str>> {
    input.split_at_position1_complete(char::is_whitespace, ErrorKind::Alpha)
}

#[test]
fn test_non_whitespace() {
    assert_eq!(non_whitespace("Hello, World!"), Ok((" World!", "Hello,")));
    assert_eq!(
        non_whitespace(" Hello, World!"),
        Err(nom::Err::Error(Error::Nom(
            " Hello, World!",
            ErrorKind::Alpha
        )))
    );
}

fn raw_string(i: &str) -> IResult<&str, Word<'_>, Error<&str>> {
    let (i, r) = recognize(tuple((space0, delimited(tag("`"), is_not("`"), tag("`")))))(i)?;
    Ok((i, Word::Raw(r)))
}

#[test]
fn test_raw_string() {
    assert_eq!(
        raw_string("`Hello, World!`"),
        Ok(("", Word::Raw("`Hello, World!`")))
    );
    assert_eq!(raw_string("`:`"), Ok(("", Word::Raw("`:`"))));
    assert_eq!(
        raw_string("`Hello, World!"),
        Err(nom::Err::Error(Error::Nom("", ErrorKind::Tag)))
    );
    assert_eq!(
        raw_string("Hello, World!`"),
        Err(nom::Err::Error(Error::Nom(
            "Hello, World!`",
            ErrorKind::Tag
        )))
    );
}

fn metadata(i: &str) -> IResult<&str, Word<'_>, Error<&str>> {
    let (i, _) = space0(i)?;
    let (i, key) = is_not(": \t\r\n")(i)?;
    let (i, _) = tag(":")(i)?;
    let (i, value) = non_whitespace(i)?;
    Ok((i, Word::Metadata((key, value))))
}

#[test]
fn test_metadata() {
    assert_eq!(
        metadata("key:value"),
        Ok(("", Word::Metadata(("key", "value"))))
    );
    assert_eq!(
        metadata("9@$,,afs&*:()()23,;."),
        Ok(("", Word::Metadata(("9@$,,afs&*", "()()23,;."))))
    );
    assert_eq!(
        metadata("key: value"),
        Err(nom::Err::Error(Error::Nom(" value", ErrorKind::Alpha)))
    );
    assert_eq!(
        metadata("key value"),
        Err(nom::Err::Error(Error::Nom(" value", ErrorKind::Tag)))
    );
}

fn plain(i: &str) -> IResult<&str, Word<'_>, Error<&str>> {
    let (i, p) = recognize(tuple((space0, is_not(" \t\r\n"))))(i)?;
    Ok((i, Word::Plain(p)))
}

#[test]
fn test_plain() {
    assert_eq!(plain("Hello, World!"), Ok((" World!", Word::Plain("Hello,"))));
    assert_eq!(
        plain(" Hello, World!"),
        Ok((" World!", Word::Plain(" Hello,")))
    );
    assert_eq!(
        plain("Sometimes\tnothing"),
        Ok(("\tnothing", Word::Plain("Sometimes")))
    );
    assert_eq!(plain("Nospace"), Ok(("", Word::Plain("Nospace"))));
    assert_eq!(
        plain(""),
        Err(nom::Err::Error(Error::Nom("", ErrorKind::IsNot)))
    );
}

fn word(i: &str) -> IResult<&str, Word<'_>, Error<&str>> {
    alt((raw_string, context, project, metadata, plain))(i)
}

#[test]
fn test_word() {
    assert_eq!(word("word +project"), Ok((" +project", Word::Plain("word"))));
    assert_eq!(
        word(" word @context"),
        Ok((" @context", Word::Plain(" word")))
    );
    assert_eq!(word("Nospace"), Ok(("", Word::Plain("Nospace"))));
    assert_eq!(word("+project"), Ok(("", Word::Project("project"))));
    assert_eq!(word(" +project"), Ok(("", Word::Project("project"))));
    assert_eq!(word("@context"), Ok(("", Word::Context("context"))));
    assert_eq!(word(" @context"), Ok(("", Word::Context("context"))));
    assert_eq!(
        word("+project word"),
        Ok((" word", Word::Project("project")))
    );
    assert_eq!(
        word("@context word"),
        Ok((" word", Word::Context("context")))
    );
    assert_eq!(
        word("key:value word"),
        Ok((" word", Word::Metadata(("key", "value"))))
    );
}

fn text(i: &str) -> IResult<&str, Vec<Word<'_>>, Error<&str>> {
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
            vec![Word::Plain("Hello,"), Word::Plain(" World!"),]
        ))
    );
    assert_eq!(
        text("Hello, World! +project @context"),
        Ok((
            "",
            vec![
                Word::Plain("Hello,"),
                Word::Plain(" World!"),
                Word::Project("project"),
                Word::Context("context"),
            ]
        ))
    );
    assert_eq!(
        text("Hello, World! test:data"),
        Ok((
            "",
            vec![
                Word::Plain("Hello,"),
                Word::Plain(" World!"),
                Word::Metadata(("test", "data")),
            ]
        ))
    );
    assert_eq!(
        text(""),
        Err(nom::Err::Error(Error::Nom("", ErrorKind::Many1)))
    );
}

fn text_line(i: &str) -> IResult<&str, Vec<Word<'_>>, Error<&str>> {
    let (i, mut words) = text(i)?;
    let (i, ws) = recognize(many0(line_ending))(i)?;
    if !ws.is_empty() {
        words.push(Word::Plain(ws));
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
                Word::Plain("Hello,"),
                Word::Plain(" World!"),
                Word::Project("project"),
                Word::Context("context"),
                Word::Plain("\n\n"),
            ]
        ))
    );
}

fn raw_string_multiline(i: &str) -> IResult<&str, Vec<Word<'_>>, Error<&str>> {
    let (i, _) = tag("```")(i)?;
    let (i, _) = line_ending(i)?;
    let (i, text) = take_until("```")(i)?;
    let (i, _) = tag("```")(i)?;
    let (i, ws) = many0(line_ending)(i)?;
    let mut v = vec![Word::Raw(text)];
    if ws.len() > 1 {
        ws[0..ws.len() - 1].iter().for_each(|l| {
            v.push(Word::Plain(*l));
        });
    }
    Ok((i, v))
}

#[test]
fn test_raw_string_multiline() {
    assert_eq!(
        raw_string_multiline("```\nHello, World!\n```"),
        Ok(("", vec![Word::Raw("Hello, World!\n")]))
    );
    // With trailing newline
    assert_eq!(
        raw_string_multiline("```\nHello, World!\n```\n"),
        Ok(("", vec![Word::Raw("Hello, World!\n")]))
    );
    // With indentation
    assert_eq!(
        raw_string_multiline("```\nList:\n  - item 1\n  - item2\n```"),
        Ok(("", vec![Word::Raw("List:\n  - item 1\n  - item2\n")]))
    );
}

#[test]
fn test_raw_string_multiline_with_backtick_in_content() {
    assert_eq!(
        raw_string_multiline("```\nUse `code` in raw string\n```"),
        Ok(("", vec![Word::Raw("Use `code` in raw string\n")]))
    );
}

fn text_multiline(i: &str) -> IResult<&str, Vec<Vec<Word<'_>>>, Error<&str>> {
    fold_many1(
        alt((raw_string_multiline, text_line)),
        Vec::new,
        |mut acc: Vec<_>, item| {
            acc.push(item);
            acc
        },
    )(i)
}

#[test]
fn test_text_multiline() {
    assert_eq!(
        text_multiline("Hello, World! +project @context\n\nAnother line. meta:data"),
        Ok((
            "",
            vec![
                vec![
                    Word::Plain("Hello,"),
                    Word::Plain(" World!"),
                    Word::Project("project"),
                    Word::Context("context"),
                    Word::Plain("\n\n"),
                ],
                vec![
                    Word::Plain("Another"),
                    Word::Plain(" line."),
                    Word::Metadata(("meta", "data")),
                ],
            ]
        ))
    );
    assert_eq!(
        text_multiline("Test raw colon.\nThis contains raw `:`."),
        Ok((
            "",
            vec![
                vec![
                    Word::Plain("Test"),
                    Word::Plain(" raw"),
                    Word::Plain(" colon."),
                    Word::Plain("\n"),
                ],
                vec![
                    Word::Plain("This"),
                    Word::Plain(" contains"),
                    Word::Plain(" raw"),
                    Word::Raw(" `:`"),
                    Word::Plain("."),
                ],
            ]
        ))
    );
    assert_eq!(
        text_multiline(
            r##"Here is some text with `:` in a multiline raw string.

```
Span::styled(
    format!("#{} ", todo_item.todo.id.unwrap_or(0)),
),
```
"##
        ),
        Ok((
            "",
            vec![
                vec![
                    Word::Plain("Here"),
                    Word::Plain(" is"),
                    Word::Plain(" some"),
                    Word::Plain(" text"),
                    Word::Plain(" with"),
                    Word::Raw(" `:`"),
                    Word::Plain(" in"),
                    Word::Plain(" a"),
                    Word::Plain(" multiline"),
                    Word::Plain(" raw"),
                    Word::Plain(" string."),
                    Word::Plain("\n\n"),
                ],
                vec![Word::Raw(
                    r##"Span::styled(
    format!("#{} ", todo_item.todo.id.unwrap_or(0)),
),
"##
                ),],
            ]
        ))
    );
    assert_eq!(
        text_multiline(
            r##"Here is some text with `:` in a multiline raw string.

```
Span::styled(
    format!("#{} ", todo_item.todo.id.unwrap_or(0)),
),
```

And here is some text that follows.
"##
        ),
        Ok((
            "",
            vec![
                vec![
                    Word::Plain("Here"),
                    Word::Plain(" is"),
                    Word::Plain(" some"),
                    Word::Plain(" text"),
                    Word::Plain(" with"),
                    Word::Raw(" `:`"),
                    Word::Plain(" in"),
                    Word::Plain(" a"),
                    Word::Plain(" multiline"),
                    Word::Plain(" raw"),
                    Word::Plain(" string."),
                    Word::Plain("\n\n"),
                ],
                vec![
                    Word::Raw(
                        r##"Span::styled(
    format!("#{} ", todo_item.todo.id.unwrap_or(0)),
),
"##
                    ),
                    Word::Plain("\n"),
                ],
                vec![
                    Word::Plain("And"),
                    Word::Plain(" here"),
                    Word::Plain(" is"),
                    Word::Plain(" some"),
                    Word::Plain(" text"),
                    Word::Plain(" that"),
                    Word::Plain(" follows."),
                    Word::Plain("\n"),
                ],
            ]
        ))
    );
}

pub fn todo(s: &str) -> IResult<&str, Todo, Error<&str>> {
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
    let mut metadata = Metadata::new();

    for word in text {
        match word {
            Word::Plain(p) => title.push_str(p),
            Word::Raw(r) => title.push_str(r),
            Word::Project(p) => projects.push(p.to_owned()),
            Word::Context(c) => contexts.push(c.to_owned()),
            Word::Metadata((k, v)) => {
                // Metadata keys starting with an underscore are reserved for internal use.
                if k.starts_with("_") {
                    match k {
                        _ => {}
                    }
                } else {
                    match metadata.set(k, v) {
                        Ok(_) => {}
                        Err(e) => return Err(nom::Err::Error(Error::BadMetadata(e))),
                    };
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
                        Word::Plain(p) => description.push_str(p),
                        Word::Raw(r) => description.push_str(r),
                        Word::Project(p) => projects.push(p.to_owned()),
                        Word::Context(c) => contexts.push(c.to_owned()),
                        Word::Metadata((k, v)) => {
                            // Metadata keys starting with an underscore are reserved for internal use.
                            if k.starts_with("_") {
                                match k {
                                    _ => {}
                                }
                            } else {
                                match metadata.set(k, v) {
                                    Ok(_) => {}
                                    Err(e) => return Err(nom::Err::Error(Error::BadMetadata(e))),
                                };
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
    assert_eq!(
        todo(
            r#"#3 (C) 2024-09-06 Meta data parsing interferes with code in todos +bug

This code needs to be in some kind of escaped string so that it can be parsed correctly because
it contains `:` characters which immediately flip the parser into metadata munching."#
        ),
        Ok((
            "",
            TodoBuilder::default()
                .title("Meta data parsing interferes with code in todos".to_string())
                .id(Some(3))
                .priority(Some('C'))
                .projects(vec!["bug".to_string()])
                .creation_date(chrono::NaiveDate::from_ymd_opt(2024, 9, 6))
                .description(Some(
                    "This code needs to be in some kind of escaped string so that it can be parsed correctly because\nit contains `:` characters which immediately flip the parser into metadata munching.".to_string()
                ))
                .build()
                .unwrap()
        ))
    );
    assert_eq!(
        todo(
            r##"#3 (C) 2024-09-06 Meta data parsing interferes with code in todos +bug

```
Span::styled(
    format!("#{} ", todo_item.todo.id.unwrap_or(0)),
    Style::new().fg(Color::Red),
),
```
"##
        ),
        Ok((
            "",
            TodoBuilder::default()
                .title("Meta data parsing interferes with code in todos".to_string())
                .id(Some(3))
                .priority(Some('C'))
                .projects(vec!["bug".to_string()])
                .creation_date(chrono::NaiveDate::from_ymd_opt(2024, 9, 6))
                .description(Some(
                    r##"Span::styled(
    format!("#{} ", todo_item.todo.id.unwrap_or(0)),
    Style::new().fg(Color::Red),
),"##
                        .to_string()
                ))
                .build()
                .unwrap()
        ))
    );
    assert_eq!(
        todo(
            r##"#3 (C) 2024-09-06 Meta data parsing interferes with code in todos +bug

```
Span::styled(
    format!("#{} ", todo_item.todo.id.unwrap_or(0)),
    Style::new().fg(Color::Red),
),
```

Not sure what the solution is yet, as lots of languages use `:` in their syntax so taking it on
a case by case basis feels impossible.
"##
        ),
        Ok((
            "",
            TodoBuilder::default()
                .title("Meta data parsing interferes with code in todos".to_string())
                .id(Some(3))
                .priority(Some('C'))
                .projects(vec!["bug".to_string()])
                .creation_date(chrono::NaiveDate::from_ymd_opt(2024, 9, 6))
                .description(Some(
                    r##"Span::styled(
    format!("#{} ", todo_item.todo.id.unwrap_or(0)),
    Style::new().fg(Color::Red),
),

Not sure what the solution is yet, as lots of languages use `:` in their syntax so taking it on
a case by case basis feels impossible."##
                        .to_string()
                ))
                .build()
                .unwrap()
        ))
    );
}
