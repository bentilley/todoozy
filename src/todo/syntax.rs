use crate::todo::{Metadata, TodoIdentifier};
use derive_builder::Builder;
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
    InvalidDate(String),
    Nom(I, ErrorKind),
    Parser(String),
}

impl<I> ParseError<I> for Error<I> {
    fn from_error_kind(input: I, kind: ErrorKind) -> Self {
        Error::Nom(input, kind)
    }

    fn append(_: I, _: ErrorKind, other: Self) -> Self {
        other
    }
}

impl<I: std::fmt::Debug> std::fmt::Display for Error<I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::InvalidDate(msg) => write!(f, "invalid date: {}", msg),
            Error::Nom(input, kind) => write!(f, "parse error ({:?}): {:?}", kind, input),
            Error::Parser(msg) => write!(f, "parse error: {}", msg),
        }
    }
}

fn identifier(i: &str) -> IResult<&str, TodoIdentifier, Error<&str>> {
    let (i, _) = space0(i)?;
    let (i, prefix) = alt((tag("#"), tag("&")))(i)?;
    let (i, digits) = digit1(i)?;
    let (i, _) = space1(i)?;

    let id: u32 = digits.parse().unwrap();
    let identifier = match prefix {
        "#" => TodoIdentifier::Primary(id),
        "&" => TodoIdentifier::Reference(id),
        _ => unreachable!(),
    };
    Ok((i, identifier))
}

fn uppercase(i: &str) -> IResult<&str, char, Error<&str>> {
    let chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let (i, c) = one_of(chars)(i)?;
    Ok((i, c))
}

fn priority(i: &str) -> IResult<&str, char, Error<&str>> {
    terminated(delimited(tag("("), uppercase, tag(")")), multispace1)(i)
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

fn date(i: &str) -> IResult<&str, chrono::NaiveDate, Error<&str>> {
    terminated(date_fmt, multispace1)(i)
}

#[derive(Debug, PartialEq)]
enum Word<'a> {
    Tag(&'a str),
    Metadata((&'a str, &'a str)),
    Plain(&'a str),
    Raw(&'a str),
}

fn todo_tag(i: &str) -> IResult<&str, Word<'_>, Error<&str>> {
    let (i, _) = space0(i)?;
    let (i, t) = preceded(tag("+"), alphanumeric1)(i)?;
    Ok((i, Word::Tag(t)))
}

fn non_whitespace(input: &str) -> IResult<&str, &str, Error<&str>> {
    input.split_at_position1_complete(char::is_whitespace, ErrorKind::Alpha)
}

fn raw_string(i: &str) -> IResult<&str, Word<'_>, Error<&str>> {
    let (i, r) = recognize(tuple((space0, delimited(tag("`"), is_not("`"), tag("`")))))(i)?;
    Ok((i, Word::Raw(r)))
}

fn metadata(i: &str) -> IResult<&str, Word<'_>, Error<&str>> {
    let (i, _) = space0(i)?;
    let (i, key) = is_not(": \t\r\n")(i)?;
    let (i, _) = tag(":")(i)?;
    let (i, value) = non_whitespace(i)?;
    Ok((i, Word::Metadata((key, value))))
}

fn plain(i: &str) -> IResult<&str, Word<'_>, Error<&str>> {
    let (i, p) = recognize(tuple((space0, is_not(" \t\r\n"))))(i)?;
    Ok((i, Word::Plain(p)))
}

fn word(i: &str) -> IResult<&str, Word<'_>, Error<&str>> {
    alt((raw_string, todo_tag, metadata, plain))(i)
}

fn text(i: &str) -> IResult<&str, Vec<Word<'_>>, Error<&str>> {
    fold_many1(word, Vec::new, |mut acc: Vec<_>, item| {
        acc.push(item);
        acc
    })(i)
}

fn text_line(i: &str) -> IResult<&str, Vec<Word<'_>>, Error<&str>> {
    let (i, mut words) = text(i)?;
    let (i, ws) = recognize(many0(line_ending))(i)?;
    if !ws.is_empty() {
        words.push(Word::Plain(ws));
    }
    Ok((i, words))
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

#[derive(Builder, Clone, Debug, Default, PartialEq)]
pub struct TodoInfo {
    #[builder(default)]
    pub id: Option<TodoIdentifier>,

    #[builder(default)]
    pub priority: Option<char>,
    #[builder(default)]
    pub completion_date: Option<chrono::NaiveDate>,
    #[builder(default)]
    pub creation_date: Option<chrono::NaiveDate>,

    #[builder(default)]
    pub title: String,
    #[builder(default)]
    pub description: Option<String>,

    #[builder(default)]
    pub tags: Vec<String>,

    #[builder(default)]
    pub metadata: Metadata,
}

impl TryFrom<&str> for TodoInfo {
    type Error = Error<String>;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match todo(value) {
            Ok((_, t)) => Ok(t),
            Err(nom::Err::Error(e)) | Err(nom::Err::Failure(e)) => {
                Err(Error::Parser(format!("{:?}", e)))
            }
            Err(nom::Err::Incomplete(_)) => Err(Error::Parser("Incomplete input".to_string())),
        }
    }
}

pub fn todo(s: &str) -> IResult<&str, TodoInfo, Error<&str>> {
    let (i, id) = opt(identifier)(s)?;
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
    let mut tags: Vec<String> = Vec::new();
    let mut metadata = Metadata::new();

    for word in text {
        match word {
            Word::Plain(p) => title.push_str(p),
            Word::Raw(r) => title.push_str(r),
            Word::Tag(t) => tags.push(t.to_owned()),
            Word::Metadata((k, v)) => {
                // Metadata keys starting with an underscore are reserved for internal use.
                if !k.starts_with("_") {
                    metadata.set(k, v);
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
                        Word::Tag(t) => tags.push(t.to_owned()),
                        Word::Metadata((k, v)) => {
                            // Metadata keys starting with an underscore are reserved for internal use.
                            if !k.starts_with("_") {
                                metadata.set(k, v);
                            }
                        }
                    }
                }
            }
            Some(description.trim().to_string())
        }
        None => None,
    };

    // Deduplicate tags while preserving insertion order
    let mut seen = std::collections::HashSet::new();
    tags.retain(|t| seen.insert(t.clone()));

    Ok((
        i,
        TodoInfoBuilder::default()
            .id(id)
            .priority(priority)
            .completion_date(completion_date)
            .creation_date(creation_date)
            .title(title.trim().to_string())
            .description(description)
            .tags(tags)
            .metadata(metadata)
            .build()
            .unwrap(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identifier() {
        assert_eq!(identifier("#123 "), Ok(("", TodoIdentifier::Primary(123))));
        assert_eq!(
            identifier("&456 "),
            Ok(("", TodoIdentifier::Reference(456)))
        );
        assert_eq!(
            identifier("#123"),
            Err(nom::Err::Error(Error::Nom("", ErrorKind::Space)))
        );
        assert_eq!(
            identifier("123"),
            Err(nom::Err::Error(Error::Nom("123", ErrorKind::Tag)))
        );
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

    #[test]
    fn test_tag_parser() {
        assert_eq!(todo_tag("+test"), Ok(("", Word::Tag("test"))));
        assert_eq!(
            todo_tag("+test something"),
            Ok((" something", Word::Tag("test")))
        );
        assert_eq!(
            todo_tag("test"),
            Err(nom::Err::Error(Error::Nom("test", ErrorKind::Tag)))
        );
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

    #[test]
    fn test_plain() {
        assert_eq!(
            plain("Hello, World!"),
            Ok((" World!", Word::Plain("Hello,")))
        );
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

    #[test]
    fn test_word() {
        assert_eq!(word("word +tag"), Ok((" +tag", Word::Plain("word"))));
        assert_eq!(word(" word +tag"), Ok((" +tag", Word::Plain(" word"))));
        assert_eq!(word("Nospace"), Ok(("", Word::Plain("Nospace"))));
        assert_eq!(word("+tag"), Ok(("", Word::Tag("tag"))));
        assert_eq!(word(" +tag"), Ok(("", Word::Tag("tag"))));
        assert_eq!(word("+tag word"), Ok((" word", Word::Tag("tag"))));
        assert_eq!(
            word("key:value word"),
            Ok((" word", Word::Metadata(("key", "value"))))
        );
    }

    #[test]
    fn test_text() {
        assert_eq!(
            text("Hello, World!"),
            Ok(("", vec![Word::Plain("Hello,"), Word::Plain(" World!"),]))
        );
        assert_eq!(
            text("Hello, World! +tag1 +tag2"),
            Ok((
                "",
                vec![
                    Word::Plain("Hello,"),
                    Word::Plain(" World!"),
                    Word::Tag("tag1"),
                    Word::Tag("tag2"),
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

    #[test]
    fn test_text_line() {
        assert_eq!(
            text_line("Hello, World! +tag1 +tag2\n\n"),
            Ok((
                "",
                vec![
                    Word::Plain("Hello,"),
                    Word::Plain(" World!"),
                    Word::Tag("tag1"),
                    Word::Tag("tag2"),
                    Word::Plain("\n\n"),
                ]
            ))
        );
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

    #[test]
    fn test_text_multiline() {
        assert_eq!(
            text_multiline("Hello, World! +tag1 +tag2\n\nAnother line. meta:data"),
            Ok((
                "",
                vec![
                    vec![
                        Word::Plain("Hello,"),
                        Word::Plain(" World!"),
                        Word::Tag("tag1"),
                        Word::Tag("tag2"),
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

    #[test]
    fn test_todo() {
        assert_eq!(
            todo("This is a test todo"),
            Ok((
                "",
                TodoInfoBuilder::default()
                    .title("This is a test todo".to_string())
                    .build()
                    .unwrap()
            ))
        );
        assert_eq!(
            todo("(A) This is a test todo"),
            Ok((
                "",
                TodoInfoBuilder::default()
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
                TodoInfoBuilder::default()
                    .title("This is a test todo".to_string())
                    .id(Some(TodoIdentifier::Primary(123)))
                    .priority(Some('A'))
                    .build()
                    .unwrap()
            ))
        );
        assert_eq!(
            todo("(A) Test todo +tag1 +tag2 key:value"),
            Ok((
                "",
                TodoInfoBuilder::default()
                    .title("Test todo".to_string())
                    .priority(Some('A'))
                    .tags(vec!["tag1".to_string(), "tag2".to_string()])
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
                r#"(A) 2024-08-11 Test todo +tag1

This is a test todo with a description. more:data

With multiple paragraphs, and some paragraphs that contain tags. +extra"#
            ),
            Ok((
                "",
                TodoInfoBuilder::default()
                    .title("Test todo".to_string())
                    .priority(Some('A'))
                    .tags(vec!["tag1".to_string(), "extra".to_string()])
                    .creation_date(chrono::NaiveDate::from_ymd_opt(2024, 8, 11))
                    .metadata(
                        vec![("more".to_string(), "data".to_string())]
                            .into_iter()
                            .collect()
                    )
                    .description(Some(
                        "This is a test todo with a description.\n\nWith multiple paragraphs, and some paragraphs that contain tags."
                            .to_string()
                    ))
                    .build()
                    .unwrap()
            ))
        );
        assert_eq!(
            todo(
                r#"(A) 2024-08-14 2024-08-11 Test todo +tag1

- Can it handle indented lines?
  - Yes, it can. +extra"#
            ),
            Ok((
                "",
                TodoInfoBuilder::default()
                    .title("Test todo".to_string())
                    .priority(Some('A'))
                    .tags(vec!["tag1".to_string(), "extra".to_string()])
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
                TodoInfoBuilder::default()
                    .title("Meta data parsing interferes with code in todos".to_string())
                    .id(Some(TodoIdentifier::Primary(3)))
                    .priority(Some('C'))
                    .tags(vec!["bug".to_string()])
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
                TodoInfoBuilder::default()
                    .title("Meta data parsing interferes with code in todos".to_string())
                    .id(Some(TodoIdentifier::Primary(3)))
                    .priority(Some('C'))
                    .tags(vec!["bug".to_string()])
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
                TodoInfoBuilder::default()
                    .title("Meta data parsing interferes with code in todos".to_string())
                    .id(Some(TodoIdentifier::Primary(3)))
                    .priority(Some('C'))
                    .tags(vec!["bug".to_string()])
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

    #[test]
    fn test_todo_duplicate_tags_in_title_deduplicated() {
        // +tag appears twice in title
        let result = todo("Test todo +tag +tag +other");
        assert_eq!(
            result,
            Ok((
                "",
                TodoInfoBuilder::default()
                    .title("Test todo".to_string())
                    .tags(vec!["tag".to_string(), "other".to_string()])
                    .build()
                    .unwrap()
            ))
        );
    }

    #[test]
    fn test_todo_duplicate_tags_in_description_deduplicated() {
        // +tag appears twice in description only
        let result = todo("Test todo\n\nDescription +tag and +tag again");
        assert_eq!(
            result,
            Ok((
                "",
                TodoInfoBuilder::default()
                    .title("Test todo".to_string())
                    .tags(vec!["tag".to_string()])
                    .description(Some("Description and again".to_string()))
                    .build()
                    .unwrap()
            ))
        );
    }

    #[test]
    fn test_todo_duplicate_tags_across_title_and_description_deduplicated() {
        // +tag appears in title and again in description
        let result = todo("Test todo +tag\n\nDescription with +tag again");
        assert_eq!(
            result,
            Ok((
                "",
                TodoInfoBuilder::default()
                    .title("Test todo".to_string())
                    .tags(vec!["tag".to_string()])
                    .description(Some("Description with again".to_string()))
                    .build()
                    .unwrap()
            ))
        );
    }

    #[test]
    fn test_todo_reference() {
        assert_eq!(
            todo("&43 Reference title +tag"),
            Ok((
                "",
                TodoInfoBuilder::default()
                    .id(Some(TodoIdentifier::Reference(43)))
                    .title("Reference title".to_string())
                    .tags(vec!["tag".to_string()])
                    .build()
                    .unwrap()
            ))
        );
    }

    #[test]
    fn test_todo_reference_with_priority() {
        // Parser accepts this - validation happens during linking
        assert_eq!(
            todo("&43 (A) Reference with priority"),
            Ok((
                "",
                TodoInfoBuilder::default()
                    .id(Some(TodoIdentifier::Reference(43)))
                    .priority(Some('A'))
                    .title("Reference with priority".to_string())
                    .build()
                    .unwrap()
            ))
        );
    }

    #[test]
    fn test_todo_duplicate_metadata_keys_in_title() {
        // Multiple depends:X in title should collect into a list
        let result = todo("#43 Implement auth depends:42 depends:41");
        assert!(result.is_ok());
        let (_, info) = result.unwrap();
        assert_eq!(info.title, "Implement auth");
        assert_eq!(
            info.metadata.get("depends"),
            Some(vec!["42".to_string(), "41".to_string()].as_slice())
        );
    }

    #[test]
    fn test_todo_duplicate_metadata_keys_across_title_and_description() {
        // depends:X in title and another in description
        let result = todo("#43 Implement auth depends:42\n\nNeed to also depends:41");
        assert!(result.is_ok());
        let (_, info) = result.unwrap();
        assert_eq!(
            info.metadata.get("depends"),
            Some(vec!["42".to_string(), "41".to_string()].as_slice())
        );
    }

    #[test]
    fn test_todo_mixed_single_and_multi_value_metadata() {
        // Mix of single-value and multi-value metadata
        let result = todo("Test todo priority:high depends:1 depends:2 owner:alice");
        assert!(result.is_ok());
        let (_, info) = result.unwrap();

        // Single-value keys
        assert_eq!(info.metadata.get("priority").unwrap()[0], "high");
        assert_eq!(info.metadata.get("owner").unwrap()[0], "alice");

        // Multi-value key
        assert_eq!(
            info.metadata.get("depends"),
            Some(vec!["1".to_string(), "2".to_string()].as_slice())
        );
    }
}
