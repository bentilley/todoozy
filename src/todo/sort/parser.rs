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

fn direction(i: &str) -> IResult<&str, Direction> {
    let (i, d) = alt((tag("asc"), tag("desc")))(i)?;
    match d {
        "asc" => Ok((i, Direction::Ascending)),
        "desc" => Ok((i, Direction::Descending)),
        _ => unreachable!(),
    }
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

fn term(i: &str) -> IResult<&str, Box<dyn Sorter>> {
    let (i, f) = property_sort(i)?;
    Ok((i, Box::new(f)))
}

fn pipe(i: &str) -> IResult<&str, &str> {
    delimited(space0, tag(">"), space0)(i)
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

pub fn parse_expression(sort_def: &str) -> Result<Box<dyn Sorter>, String> {
    match pipeline(sort_def) {
        Ok((_, sorter)) => Ok(sorter),
        Err(e) => Err(format!("{:?}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::todo::{parser::TodoInfoBuilder, Location, Todo};

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

    #[test]
    fn test_direction() {
        assert_eq!(direction("asc"), Ok(("", Direction::Ascending)));
        assert_eq!(direction("desc"), Ok(("", Direction::Descending)));
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

    #[test]
    fn test_term() {
        let (i, s) = term("priority").expect("Failed to parse");
        assert_eq!(i, "");
        let a = Todo::new(
            TodoInfoBuilder::default()
                .priority(Some('A'))
                .build()
                .unwrap(),
            Location::default(),
        );
        let b = Todo::new(
            TodoInfoBuilder::default()
                .priority(Some('B'))
                .build()
                .unwrap(),
            Location::default(),
        );
        assert_eq!(s.compare(&a, &b), std::cmp::Ordering::Less);

        let (i, s) = term("file:desc > line_number").expect("Failed to parse");
        assert_eq!(i, " > line_number");
        let a = Todo::new(
            TodoInfoBuilder::default().build().unwrap(),
            Location::new(Some("a.txt".to_string()), 1, 1),
        );
        let b = Todo::new(
            TodoInfoBuilder::default().build().unwrap(),
            Location::new(Some("b.txt".to_string()), 1, 1),
        );
        assert_eq!(s.compare(&a, &b), std::cmp::Ordering::Greater);
    }

    #[test]
    fn test_pipe() {
        assert_eq!(pipe("> "), Ok(("", ">")));
        assert_eq!(pipe(" > "), Ok(("", ">")));
        assert_eq!(pipe("  >  "), Ok(("", ">")));
        assert_eq!(pipe("  > "), Ok(("", ">")));
    }

    #[test]
    fn test_pipeline() {
        let (i, s) = pipeline("file > line_number").expect("Failed to parse");
        assert_eq!(i, "");
        let mut todos = vec![
            Todo::new(
                TodoInfoBuilder::default()
                    .title("a".to_string())
                    .build()
                    .unwrap(),
                Location::new(Some("b.txt".to_string()), 1, 1),
            ),
            Todo::new(
                TodoInfoBuilder::default()
                    .title("b".to_string())
                    .build()
                    .unwrap(),
                Location::new(Some("a.txt".to_string()), 10, 10),
            ),
            Todo::new(
                TodoInfoBuilder::default()
                    .title("c".to_string())
                    .build()
                    .unwrap(),
                Location::new(Some("a.txt".to_string()), 1, 1),
            ),
        ];
        todos.sort_unstable_by(|a, b| s.compare(&a, &b));
        assert_eq!(todos[0].title, "c".to_string());
        assert_eq!(todos[1].title, "b".to_string());
        assert_eq!(todos[2].title, "a".to_string());
    }
}
