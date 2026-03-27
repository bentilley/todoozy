use super::{Conjunction, Disjunction, Filter, Negation, Property, PropertyFilter, Relation};
use nom::{
    branch::alt,
    bytes::complete::{is_not, tag},
    character::complete::{char, space0, space1},
    combinator::opt,
    multi::many0,
    sequence::{delimited, tuple},
    IResult,
};

fn property(i: &str) -> IResult<&str, Property> {
    let (i, p) = alt((
        tag("file"),
        tag("priority"),
        tag("tag"),
        tag("creation_date"),
        tag("completion_date"),
    ))(i)?;
    match p {
        "file" => Ok((i, Property::File)),
        "priority" => Ok((i, Property::Priority)),
        "tag" => Ok((i, Property::Tag)),
        "creation_date" => Ok((i, Property::CreationDate)),
        "completion_date" => Ok((i, Property::CompletionDate)),
        _ => unreachable!(),
    }
}

#[test]
fn test_property() {
    assert_eq!(property("file"), Ok(("", Property::File)));
    assert_eq!(property("priority"), Ok(("", Property::Priority)));
    assert_eq!(property("tag"), Ok(("", Property::Tag)));
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
        property_filter("tag=Test"),
        Ok((
            "",
            PropertyFilter {
                property: Property::Tag,
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
    let (i, f) = term("tag=Test").expect("Failed to parse");
    assert_eq!(i, "");
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec!["Test".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));

    let (i, f) = term("tag=Test and tag=Else").expect("Failed to parse");
    assert_eq!(i, " and tag=Else");
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec!["Test".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
}

fn parens(i: &str) -> IResult<&str, Box<dyn Filter>> {
    delimited(space0, delimited(char('('), conjunction, char(')')), space0)(i)
}

fn not(i: &str) -> IResult<&str, &str> {
    delimited(space0, tag("not"), space1)(i)
}

#[test]
fn test_not() {
    assert_eq!(not("not "), Ok(("", "not")));
    assert_eq!(not(" not "), Ok(("", "not")));
    assert_eq!(not("  not  "), Ok(("", "not")));
    assert_eq!(not("  not "), Ok(("", "not")));
}

fn clause(i: &str) -> IResult<&str, Box<dyn Filter>> {
    let (i, n) = opt(not)(i)?;
    let (i, f) = alt((parens, term))(i)?;
    if n.is_some() {
        Ok((i, Box::new(Negation { filter: f })))
    } else {
        Ok((i, f))
    }
}

#[test]
fn test_clause() {
    let (i, f) = clause("tag=Test").expect("Failed to parse");
    assert_eq!(i, "");
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec!["Test".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));

    let (i, f) = clause("not tag=Test").expect("Failed to parse");
    assert_eq!(i, "");
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec!["Else".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));

    let (i, f) = clause("not (tag=Test)").expect("Failed to parse");
    assert_eq!(i, "");
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec!["Else".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));

    let (i, f) = clause("not (tag=Test and tag=Else)").expect("Failed to parse");
    assert_eq!(i, "");
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec!["Test".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));

    let (i, f) = clause("not (tag=Test or tag=Else)").expect("Failed to parse");
    assert_eq!(i, "");
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec!["Test".to_owned()])
        .build()
        .unwrap();
    assert!(!f.filter(&todo));
}

fn disjunction(i: &str) -> IResult<&str, Box<dyn Filter>> {
    let (i, s) = clause(i)?;
    let (i, exprs) = many0(tuple((or, clause)))(i)?;
    let mut filter = Disjunction { filters: vec![s] };
    for (_, s) in exprs {
        filter.filters.push(s);
    }
    Ok((i, Box::new(filter)))
}

#[test]
fn test_disjunction() {
    let (i, f) = disjunction("tag=p1 or tag=p2").expect("Failed to parse");
    assert_eq!(i, "");
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec!["p1".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
}

fn conjunction(i: &str) -> IResult<&str, Box<dyn Filter>> {
    let (i, s) = disjunction(i)?;
    let (i, exprs) = many0(tuple((and, disjunction)))(i)?;
    let mut filter = Conjunction { filters: vec![s] };
    for (_, s) in exprs {
        filter.filters.push(s);
    }
    Ok((i, Box::new(filter)))
}

#[test]
fn test_conjunction() {
    let (i, f) = conjunction("priority=A").expect("Failed to parse");
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

    let (i, f) = conjunction("tag=p1 and tag=p2").expect("Failed to parse");
    assert_eq!(i, "");
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec!["p1".to_owned(), "p2".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec!["p2".to_owned()])
        .build()
        .unwrap();
    assert!(!f.filter(&todo));

    let (i, f) = conjunction("tag=p1 or tag=p2").expect("Failed to parse");
    assert_eq!(i, "");
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec!["p1".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec!["p2".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default().build().unwrap();
    assert!(!f.filter(&todo));

    let (i, f) = conjunction("tag=p1 and tag=p2 or tag=p3").expect("Failed to parse");
    assert_eq!(i, "");
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec!["p1".to_owned(), "p2".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec!["p1".to_owned(), "p3".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec!["p1".to_owned()])
        .build()
        .unwrap();
    assert!(!f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec!["p2".to_owned()])
        .build()
        .unwrap();
    assert!(!f.filter(&todo));

    let (i, f) = conjunction("(tag=A and tag=B) and (tag=C and tag=D)")
        .expect("Failed to parse");
    assert_eq!(i, "");
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec![
            "A".to_owned(),
            "B".to_owned(),
            "C".to_owned(),
            "D".to_owned(),
        ])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec!["A".to_owned(), "B".to_owned(), "C".to_owned()])
        .build()
        .unwrap();
    assert!(!f.filter(&todo));

    let (i, f) = conjunction("(tag=p1 and tag=p2) or tag=p3").expect("Failed to parse");
    assert_eq!(i, "");
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec!["p1".to_owned(), "p2".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec!["p1".to_owned(), "p3".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec!["p3".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec!["p1".to_owned()])
        .build()
        .unwrap();
    assert!(!f.filter(&todo));

    let (i, f) = conjunction("(tag=p1 and (tag=p2 or tag=p3)) or tag=p4")
        .expect("Failed to parse");
    assert_eq!(i, "");
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec!["p1".to_owned(), "p2".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec!["p1".to_owned(), "p3".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec!["p4".to_owned()])
        .build()
        .unwrap();
    assert!(f.filter(&todo));
    let todo = crate::todo::TodoBuilder::default()
        .tags(vec!["p1".to_owned()])
        .build()
        .unwrap();
    assert!(!f.filter(&todo));

    let (i, f) = conjunction("creation_date>=2024-08-22").expect("Failed to parse");
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

pub fn parse_expression(filter_def: &str) -> Result<Box<dyn Filter>, String> {
    match conjunction(filter_def) {
        Ok((_, f)) => Ok(f),
        Err(e) => Err(format!("Failed to parse filter conjunction: {:?}", e)),
    }
}
