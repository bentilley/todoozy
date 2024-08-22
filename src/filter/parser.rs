use super::{ConjunctionFilter, DisjunctionFilter, Filter, FilterProperty, PropertyFilter};
use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::{alphanumeric1, char, space0, space1},
    multi::many0,
    sequence::{delimited, tuple},
    IResult,
};

fn property(i: &str) -> IResult<&str, FilterProperty> {
    let (i, p) = alt((tag("project"), tag("priority"), tag("context"), tag("file")))(i)?;
    match p {
        "project" => Ok((i, FilterProperty::Project)),
        "priority" => Ok((i, FilterProperty::Priority)),
        "context" => Ok((i, FilterProperty::Context)),
        "file" => Ok((i, FilterProperty::File)),
        _ => unreachable!(),
    }
}

#[test]
fn test_property() {
    assert_eq!(property("project"), Ok(("", FilterProperty::Project)));
    assert_eq!(property("context"), Ok(("", FilterProperty::Context)));
    assert_eq!(property("file"), Ok(("", FilterProperty::File)));
}

fn property_filter(i: &str) -> IResult<&str, PropertyFilter> {
    let (i, (p, _, v)) = tuple((property, tag(":"), alphanumeric1))(i)?;
    Ok((
        i,
        PropertyFilter {
            property: p,
            value: v.to_owned(),
        },
    ))
}

#[test]
fn test_property_filter() {
    assert_eq!(
        property_filter("project:Test"),
        Ok((
            "",
            PropertyFilter {
                property: FilterProperty::Project,
                value: "Test".to_owned()
            }
        ))
    );
    assert_eq!(
        property_filter("context:Test"),
        Ok((
            "",
            PropertyFilter {
                property: FilterProperty::Context,
                value: "Test".to_owned()
            }
        ))
    );
    assert_eq!(
        property_filter("file:Test"),
        Ok((
            "",
            PropertyFilter {
                property: FilterProperty::File,
                value: "Test".to_owned()
            }
        ))
    );
}

fn and(i: &str) -> IResult<&str, &str> {
    delimited(space0, tag("and"), space1)(i)
}

#[test]
fn test_and() {
    assert_eq!(and("and "), Ok(("", "and")));
    assert_eq!(and(" and "), Ok(("", "and")));
    assert_eq!(and("  and  "), Ok(("", "and")));
    assert_eq!(and("  and "), Ok(("", "and")));
}

fn or(i: &str) -> IResult<&str, &str> {
    delimited(space0, tag("or"), space1)(i)
}

#[test]
fn test_or() {
    assert_eq!(or("or "), Ok(("", "or")));
    assert_eq!(or(" or "), Ok(("", "or")));
    assert_eq!(or("  or  "), Ok(("", "or")));
    assert_eq!(or("  or "), Ok(("", "or")));
}

fn term(i: &str) -> IResult<&str, Box<dyn Filter>> {
    let (i, f) = property_filter(i)?;
    Ok((i, Box::new(f)))
}

#[test]
fn test_term() {
    let (i, f) = term("project:Test").expect("Failed to parse");
    assert_eq!(i, "");
    let todo = crate::todo::TodoBuilder::default()
        .projects(vec!["Test".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));

    let (i, f) = term("project:Test and project:Else").expect("Failed to parse");
    assert_eq!(i, " and project:Else");
    let todo = crate::todo::TodoBuilder::default()
        .projects(vec!["Test".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
}

fn parens(i: &str) -> IResult<&str, Box<dyn Filter>> {
    delimited(space0, delimited(char('('), conjunction, char(')')), space0)(i)
}

fn clause(i: &str) -> IResult<&str, Box<dyn Filter>> {
    alt((parens, term))(i)
}

fn disjunction(i: &str) -> IResult<&str, Box<dyn Filter>> {
    let (i, s) = clause(i)?;
    let (i, exprs) = many0(tuple((or, clause)))(i)?;
    let mut filter = DisjunctionFilter { filters: vec![s] };
    for (_, s) in exprs {
        filter.filters.push(s);
    }
    Ok((i, Box::new(filter)))
}

#[test]
fn test_disjunction() {
    let (i, f) = disjunction("project:p1 or project:p2").expect("Failed to parse");
    assert_eq!(i, "");
    let todo = crate::todo::TodoBuilder::default()
        .projects(vec!["p1".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
}

fn conjunction(i: &str) -> IResult<&str, Box<dyn Filter>> {
    let (i, s) = disjunction(i)?;
    let (i, exprs) = many0(tuple((and, disjunction)))(i)?;
    let mut filter = ConjunctionFilter { filters: vec![s] };
    for (_, s) in exprs {
        filter.filters.push(s);
    }
    Ok((i, Box::new(filter)))
}

#[test]
fn test_conjunction() {
    let (i, f) = conjunction("project:p1 and project:p2").expect("Failed to parse");
    assert_eq!(i, "");
    let todo = crate::todo::TodoBuilder::default()
        .projects(vec!["p1".to_owned(), "p2".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
}

fn expression(i: &str) -> IResult<&str, Box<dyn Filter>> {
    alt((conjunction, disjunction, term))(i)
}

pub fn parse_expression(filter_def: &str) -> Result<Box<dyn Filter>, String> {
    match expression(filter_def) {
        Ok((_, f)) => Ok(f),
        Err(e) => Err(format!("Failed to parse filter expression: {:?}", e)),
    }
}

#[test]
fn test_expression() {
    let (i, f) = expression("priority:A").expect("Failed to parse");
    assert_eq!(i, "");
    let todo = crate::todo::TodoBuilder::default()
        .priority(Some('A'))
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .priority(Some('B'))
        .build()
        .unwrap();
    assert!(!f.filter(&todo));

    let (i, f) = expression("project:p1 and project:p2").expect("Failed to parse");
    assert_eq!(i, "");
    let todo = crate::todo::TodoBuilder::default()
        .projects(vec!["p1".to_owned(), "p2".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .projects(vec!["p2".to_owned()])
        .build()
        .unwrap();
    assert!(!f.filter(&todo));

    let (i, f) = expression("project:p1 or project:p2").expect("Failed to parse");
    assert_eq!(i, "");
    let todo = crate::todo::TodoBuilder::default()
        .projects(vec!["p1".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .projects(vec!["p2".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default().build().unwrap();
    assert!(!f.filter(&todo));

    let (i, f) = expression("project:p1 and project:p2 or project:p3").expect("Failed to parse");
    assert_eq!(i, "");
    let todo = crate::todo::TodoBuilder::default()
        .projects(vec!["p1".to_owned(), "p2".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .projects(vec!["p1".to_owned(), "p3".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .projects(vec!["p1".to_owned()])
        .build()
        .unwrap();
    assert!(!f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .projects(vec!["p2".to_owned()])
        .build()
        .unwrap();
    assert!(!f.filter(&todo));

    let (i, f) = expression("(project:p1 and project:p2) or project:p3").expect("Failed to parse");
    assert_eq!(i, "");
    let todo = crate::todo::TodoBuilder::default()
        .projects(vec!["p1".to_owned(), "p2".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .projects(vec!["p1".to_owned(), "p3".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .projects(vec!["p3".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .projects(vec!["p1".to_owned()])
        .build()
        .unwrap();
    assert!(!f.filter(&todo));

    let (i, f) = expression("(project:p1 and (project:p2 or project:p3)) or project:p4")
        .expect("Failed to parse");
    assert_eq!(i, "");
    let todo = crate::todo::TodoBuilder::default()
        .projects(vec!["p1".to_owned(), "p2".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .projects(vec!["p1".to_owned(), "p3".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .projects(vec!["p4".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .projects(vec!["p1".to_owned()])
        .build()
        .unwrap();
    assert!(!f.filter(&todo));
}
