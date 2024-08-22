use nom::{
    branch::alt,
    bytes::complete::{tag, take, take_till, take_until},
    combinator::eof,
    error::{Error, ErrorKind},
    multi::fold_many0,
    IResult,
};

fn comment_singleline(i: &str) -> IResult<&str, &str> {
    let (i, _) = tag("//")(i)?;
    let (i, comment) = take_till(|c| c == '\n')(i)?;
    let (i, _) = alt((take(1u8), eof))(i)?;
    Ok((i, comment.trim()))
}

#[test]
fn test_comment_singleline() {
    assert_eq!(comment_singleline("// test\n"), Ok(("", "test")));
    assert_eq!(comment_singleline("// eof test"), Ok(("", "eof test")));
    assert_eq!(
        comment_singleline("let x = 1; // test"),
        Err(nom::Err::Error(Error::new(
            "let x = 1; // test",
            ErrorKind::Tag
        )))
    );
}

fn comment_multiline(i: &str) -> IResult<&str, &str> {
    let (i, _) = tag("/*")(i)?;
    let (i, comment) = take_until("*/")(i)?;
    let (i, _) = tag("*/")(i)?;
    Ok((i, comment.trim()))
}

#[test]
fn test_comment_multiline() {
    assert_eq!(comment_multiline("/* test */"), Ok(("", "test")));
    assert_eq!(comment_multiline("/*\n * test\n */"), Ok(("", "* test")));
    assert_eq!(
        comment_multiline("/* no closing tag test"),
        Err(nom::Err::Error(Error::new(
            " no closing tag test",
            ErrorKind::TakeUntil
        )))
    );
}

fn parse_comments(i: &str) -> IResult<&str, Vec<&str>> {
    let (i, comments) = fold_many0(
        alt((comment_singleline, comment_multiline, take(1u8))),
        Vec::new,
        |mut v: Vec<_>, c| {
            if c.len() > 1 {
                v.push(c);
            }
            v
        },
    )(i)?;
    Ok((i, comments))
}

#[test]
fn test_parse_comments() {
    assert_eq!(
        parse_comments("// test\n// test2\n"),
        Ok(("", vec!["test", "test2"]))
    );
    assert_eq!(parse_comments("/* test */"), Ok(("", vec!["test"])));
    assert_eq!(
        parse_comments("/* test\n * test2\n */"),
        Ok(("", vec!["test\n * test2"]))
    );
    assert_eq!(parse_comments("let x = 1; // test"), Ok(("", vec!["test"])));
}
