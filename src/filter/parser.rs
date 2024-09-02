use super::{ConjunctionFilter, DisjunctionFilter, Filter, Property, PropertyFilter, Relation};
use nom::{
    branch::alt,
    bytes::complete::{is_not, tag},
    character::complete::{char, space0, space1},
    multi::many0,
    sequence::{delimited, tuple},
    IResult,
};

fn property(i: &str) -> IResult<&str, Property> {
    let (i, p) = alt((
        tag("file"),
        tag("priority"),
        tag("project"),
        tag("context"),
        tag("creation_date"),
        tag("completion_date"),
    ))(i)?;
    match p {
        "file" => Ok((i, Property::File)),
        "priority" => Ok((i, Property::Priority)),
        "project" => Ok((i, Property::Project)),
        "context" => Ok((i, Property::Context)),
        "creation_date" => Ok((i, Property::CreationDate)),
        "completion_date" => Ok((i, Property::CompletionDate)),
        _ => unreachable!(),
    }
}

#[test]
fn test_property() {
    assert_eq!(property("file"), Ok(("", Property::File)));
    assert_eq!(property("priority"), Ok(("", Property::Priority)));
    assert_eq!(property("project"), Ok(("", Property::Project)));
    assert_eq!(property("context"), Ok(("", Property::Context)));
    assert_eq!(property("creation_date"), Ok(("", Property::CreationDate)));
    assert_eq!(
        property("completion_date"),
        Ok(("", Property::CompletionDate))
    );
}

fn relation(i: &str) -> IResult<&str, Relation> {
    let (i, p) = alt((
        tag("="),
        tag("!="),
        tag(">="),
        tag(">"),
        tag("<="),
        tag("<"),
    ))(i)?;
    match p {
        "=" => Ok((i, Relation::Equal)),
        "!=" => Ok((i, Relation::NotEqual)),
        ">" => Ok((i, Relation::Greater)),
        ">=" => Ok((i, Relation::GreaterEqual)),
        "<" => Ok((i, Relation::Less)),
        "<=" => Ok((i, Relation::LessEqual)),
        _ => unreachable!(),
    }
}

#[test]
fn test_relation() {
    assert_eq!(relation("="), Ok(("", Relation::Equal)));
    assert_eq!(relation("!="), Ok(("", Relation::NotEqual)));
    assert_eq!(relation(">"), Ok(("", Relation::Greater)));
    assert_eq!(relation(">="), Ok(("", Relation::GreaterEqual)));
    assert_eq!(relation("<"), Ok(("", Relation::Less)));
    assert_eq!(relation("<="), Ok(("", Relation::LessEqual)));
}

fn property_filter(i: &str) -> IResult<&str, PropertyFilter> {
    let (i, (p, r, v)) = tuple((property, relation, is_not(" )")))(i)?;
    Ok((
        i,
        PropertyFilter {
            property: p,
            relation: r,
            value: v.to_owned(),
        },
    ))
}

#[test]
fn test_property_filter() {
    assert_eq!(
        property_filter("project=Test"),
        Ok((
            "",
            PropertyFilter {
                property: Property::Project,
                relation: Relation::Equal,
                value: "Test".to_owned()
            }
        ))
    );
    assert_eq!(
        property_filter("context=Test"),
        Ok((
            "",
            PropertyFilter {
                property: Property::Context,
                relation: Relation::Equal,
                value: "Test".to_owned()
            }
        ))
    );
    assert_eq!(
        property_filter("file=Test"),
        Ok((
            "",
            PropertyFilter {
                property: Property::File,
                relation: Relation::Equal,
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
    let (i, f) = term("project=Test").expect("Failed to parse");
    assert_eq!(i, "");
    let todo = crate::todo::TodoBuilder::default()
        .projects(vec!["Test".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));

    let (i, f) = term("project=Test and project=Else").expect("Failed to parse");
    assert_eq!(i, " and project=Else");
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
    let (i, f) = disjunction("project=p1 or project=p2").expect("Failed to parse");
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
    let (i, f) = conjunction("project=p1 and project=p2").expect("Failed to parse");
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
    let (i, f) = expression("priority=A").expect("Failed to parse");
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

    let (i, f) = expression("project=p1 and project=p2").expect("Failed to parse");
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

    let (i, f) = expression("project=p1 or project=p2").expect("Failed to parse");
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

    let (i, f) = expression("project=p1 and project=p2 or project=p3").expect("Failed to parse");
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

    let (i, f) = expression("(project=p1 and project=p2) or project=p3").expect("Failed to parse");
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

    let (i, f) = expression("(project=p1 and (project=p2 or project=p3)) or project=p4")
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

    let (i, f) = expression("creation_date>=2024-08-22").expect("Failed to parse");
    assert_eq!(i, "");
    let todo = crate::todo::TodoBuilder::default()
        .creation_date(chrono::NaiveDate::from_ymd_opt(2024, 08, 23))
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .creation_date(chrono::NaiveDate::from_ymd_opt(2024, 08, 22))
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .creation_date(chrono::NaiveDate::from_ymd_opt(2024, 08, 21))
        .build()
        .unwrap();
    assert!(!f.filter(&todo));
}
