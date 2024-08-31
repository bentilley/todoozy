use super::{Direction, Property, PropertySorter, SortPipeline, Sorter};
use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::space0,
    combinator::opt,
    multi::many0,
    sequence::{delimited, preceded, tuple},
    IResult,
};

fn property(i: &str) -> IResult<&str, Property> {
    let (i, p) = alt((
        tag("title"),
        tag("file"),
        tag("line_number"),
        tag("priority"),
        tag("creation_date"),
        tag("completion_date"),
    ))(i)?;
    match p {
        "title" => Ok((i, Property::Title)),
        "file" => Ok((i, Property::File)),
        "line_number" => Ok((i, Property::LineNumber)),
        "priority" => Ok((i, Property::Priority)),
        "creation_date" => Ok((i, Property::CreationDate)),
        "completion_date" => Ok((i, Property::CompletionDate)),
        _ => unreachable!(),
    }
}

#[test]
fn test_property() {
    assert_eq!(property("title"), Ok(("", Property::Title)));
    assert_eq!(property("file"), Ok(("", Property::File)));
    assert_eq!(property("line_number"), Ok(("", Property::LineNumber)));
    assert_eq!(property("priority"), Ok(("", Property::Priority)));
    assert_eq!(property("creation_date"), Ok(("", Property::CreationDate)));
    assert_eq!(
        property("completion_date"),
        Ok(("", Property::CompletionDate))
    );
}

fn direction(i: &str) -> IResult<&str, Direction> {
    let (i, d) = alt((tag("asc"), tag("desc")))(i)?;
    match d {
        "asc" => Ok((i, Direction::Ascending)),
        "desc" => Ok((i, Direction::Descending)),
        _ => unreachable!(),
    }
}

#[test]
fn test_direction() {
    assert_eq!(direction("asc"), Ok(("", Direction::Ascending)));
    assert_eq!(direction("desc"), Ok(("", Direction::Descending)));
}

fn property_sort(i: &str) -> IResult<&str, PropertySorter> {
    let (i, p) = property(i)?;
    let (i, d) = opt(preceded(tag(":"), direction))(i)?;
    let direction = match d {
        Some(v) => v,
        None => Direction::Ascending,
    };
    Ok((
        i,
        PropertySorter {
            property: p,
            direction,
        },
    ))
}

#[test]
fn test_property_sort() {
    assert_eq!(
        property_sort("title"),
        Ok((
            "",
            PropertySorter {
                property: Property::Title,
                direction: Direction::Ascending
            }
        ))
    );
    assert_eq!(
        property_sort("file:asc"),
        Ok((
            "",
            PropertySorter {
                property: Property::File,
                direction: Direction::Ascending
            }
        ))
    );
    assert_eq!(
        property_sort("line_number:desc"),
        Ok((
            "",
            PropertySorter {
                property: Property::LineNumber,
                direction: Direction::Descending
            }
        ))
    );
}

fn term(i: &str) -> IResult<&str, Box<dyn Sorter>> {
    let (i, f) = property_sort(i)?;
    Ok((i, Box::new(f)))
}

#[test]
fn test_term() {
    let (i, s) = term("priority").expect("Failed to parse");
    assert_eq!(i, "");
    let a = crate::todo::TodoBuilder::default()
        .priority(Some('A'))
        .build()
        .unwrap();
    let b = crate::todo::TodoBuilder::default()
        .priority(Some('B'))
        .build()
        .unwrap();
    assert_eq!(s.compare(&a, &b), std::cmp::Ordering::Less);

    let (i, s) = term("file:desc > line_number").expect("Failed to parse");
    assert_eq!(i, " > line_number");
    let a = crate::todo::TodoBuilder::default()
        .file(Some("a.txt".to_string()))
        .build()
        .unwrap();
    let b = crate::todo::TodoBuilder::default()
        .file(Some("b.txt".to_string()))
        .build()
        .unwrap();
    assert_eq!(s.compare(&a, &b), std::cmp::Ordering::Greater);
}

fn pipe(i: &str) -> IResult<&str, &str> {
    delimited(space0, tag(">"), space0)(i)
}

#[test]
fn test_pipe() {
    assert_eq!(pipe("> "), Ok(("", ">")));
    assert_eq!(pipe(" > "), Ok(("", ">")));
    assert_eq!(pipe("  >  "), Ok(("", ">")));
    assert_eq!(pipe("  > "), Ok(("", ">")));
}

fn pipeline(i: &str) -> IResult<&str, Box<dyn Sorter>> {
    let (i, s) = term(i)?;
    let (i, exprs) = many0(tuple((pipe, term)))(i)?;
    let mut sorter = SortPipeline::new(vec![s]);
    for (_, s) in exprs {
        sorter.add_sorter(s);
    }
    Ok((i, Box::new(sorter)))
}

#[test]
fn test_pipeline() {
    let (i, s) = pipeline("file > line_number").expect("Failed to parse");
    assert_eq!(i, "");
    use crate::todo::TodoBuilder;
    let mut todos = vec![
        TodoBuilder::default()
            .title("a".to_string())
            .file(Some("b.txt".to_string()))
            .line_number(Some(1))
            .build()
            .unwrap(),
        TodoBuilder::default()
            .title("b".to_string())
            .file(Some("a.txt".to_string()))
            .line_number(Some(10))
            .build()
            .unwrap(),
        TodoBuilder::default()
            .title("c".to_string())
            .file(Some("a.txt".to_string()))
            .line_number(Some(1))
            .build()
            .unwrap(),
    ];
    todos.sort_unstable_by(|a, b| s.compare(&a, &b));
    assert_eq!(todos[0].title, "c".to_string());
    assert_eq!(todos[1].title, "b".to_string());
    assert_eq!(todos[2].title, "a".to_string());
}

pub fn parse_expression(sort_def: &str) -> Result<Box<dyn Sorter>, String> {
    match pipeline(sort_def) {
        Ok((_, sorter)) => Ok(sorter),
        Err(e) => Err(format!("{:?}", e)),
    }
}
