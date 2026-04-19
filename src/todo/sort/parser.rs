use super::{error::Result, Direction, Property, PropertySorter, SortPipeline, Sorter, TagSorter};
use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::{alphanumeric1, space0},
    combinator::{all_consuming, cut, map, opt, value},
    error::{context, VerboseError},
    multi::many0,
    sequence::{delimited, preceded},
};

type IResult<'a, O> = nom::IResult<&'a str, O, VerboseError<&'a str>>;

fn property(i: &str) -> IResult<'_, Property> {
    context(
        "property (id, title, file, line_number, priority, creation_date, completion_date)",
        alt((
            value(Property::Id, tag("id")),
            value(Property::Title, tag("title")),
            value(Property::File, tag("file")),
            value(Property::LineNumber, tag("line_number")),
            value(Property::Priority, tag("priority")),
            value(Property::CreationDate, tag("creation_date")),
            value(Property::CompletionDate, tag("completion_date")),
        )),
    )(i)
}

fn direction(i: &str) -> IResult<'_, Direction> {
    context(
        "direction (asc, desc)",
        alt((
            value(Direction::Ascending, tag("asc")),
            value(Direction::Descending, tag("desc")),
        )),
    )(i)
}

fn property_sort(i: &str) -> IResult<'_, PropertySorter> {
    let (i, property) = property(i)?;
    let (i, dir) = opt(preceded(tag(":"), cut(direction)))(i)?;
    Ok((
        i,
        PropertySorter {
            property,
            direction: dir.unwrap_or(Direction::Ascending),
        },
    ))
}

fn tag_sort(i: &str) -> IResult<'_, TagSorter> {
    let (i, _) = tag("tag")(i)?;
    let (i, tag_name) = cut(context("tag name", preceded(tag(":"), alphanumeric1)))(i)?;
    let (i, d) = opt(preceded(tag(":"), cut(direction)))(i)?;
    let direction = d.unwrap_or(Direction::Ascending);
    Ok((
        i,
        TagSorter {
            tag_name: tag_name.to_string(),
            direction,
        },
    ))
}

fn term(i: &str) -> IResult<'_, Box<dyn Sorter>> {
    context(
        "sort term (property or tag:name)",
        alt((
            map(property_sort, |s| Box::new(s) as Box<dyn Sorter>),
            map(tag_sort, |s| Box::new(s) as Box<dyn Sorter>),
        )),
    )(i)
}

fn pipe(i: &str) -> IResult<'_, &str> {
    delimited(space0, tag(">"), space0)(i)
}

fn pipeline(i: &str) -> IResult<'_, Box<dyn Sorter>> {
    let (i, first) = term(i)?;
    let (i, rest) = many0(preceded(pipe, cut(term)))(i)?;
    let sorters = std::iter::once(first).chain(rest).collect();
    Ok((i, Box::new(SortPipeline::new(sorters))))
}

pub fn parse_expression(sort_def: &str) -> Result<Box<dyn Sorter>> {
    match all_consuming(pipeline)(sort_def) {
        Ok((_, sorter)) => Ok(sorter),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::todo::{Location, Todo, TodoInfoBuilder};

    #[test]
    fn test_property() {
        assert_eq!(property("id"), Ok(("", Property::Id)));
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
        assert_eq!(
            property_sort("id:desc"),
            Ok((
                "",
                PropertySorter {
                    property: Property::Id,
                    direction: Direction::Descending
                }
            ))
        );
    }

    #[test]
    fn test_id_sort_comparison() {
        use crate::todo::TodoIdentifier;

        let sorter = PropertySorter {
            property: Property::Id,
            direction: Direction::Ascending,
        };
        let a = Todo::new(
            TodoInfoBuilder::default()
                .id(Some(TodoIdentifier::Primary(1)))
                .build()
                .unwrap(),
            Location::default(),
        );
        let b = Todo::new(
            TodoInfoBuilder::default()
                .id(Some(TodoIdentifier::Primary(2)))
                .build()
                .unwrap(),
            Location::default(),
        );
        let c = Todo::new(
            TodoInfoBuilder::default().build().unwrap(),
            Location::default(),
        );
        assert_eq!(sorter.compare(&a, &b), std::cmp::Ordering::Less);
        assert_eq!(sorter.compare(&b, &a), std::cmp::Ordering::Greater);
        assert_eq!(sorter.compare(&a, &c), std::cmp::Ordering::Greater); // 1 > 0
        assert_eq!(sorter.compare(&c, &a), std::cmp::Ordering::Less); // 0 < 1
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

    #[test]
    fn test_parse_expression_invalid() {
        // Invalid property names
        assert_eq!(
            parse_expression("non").unwrap_err().to_string(),
            "expected sort term (property or tag:name) at 'non'"
        );
        assert_eq!(
            parse_expression("file > non").unwrap_err().to_string(),
            "expected sort term (property or tag:name) at 'non'"
        );
        assert_eq!(
            parse_expression("file > non > priority")
                .unwrap_err()
                .to_string(),
            "expected sort term (property or tag:name) at 'non > priority'"
        );

        // Empty or whitespace-only expressions
        assert_eq!(
            parse_expression("").unwrap_err().to_string(),
            "expected sort term (property or tag:name)"
        );
        assert_eq!(
            parse_expression("  ").unwrap_err().to_string(),
            "expected sort term (property or tag:name)"
        );

        // Malformed pipe syntax
        assert_eq!(
            parse_expression(">").unwrap_err().to_string(),
            "expected sort term (property or tag:name) at '>'"
        );
        assert_eq!(
            parse_expression("> file").unwrap_err().to_string(),
            "expected sort term (property or tag:name) at '> file'"
        );
        assert_eq!(
            parse_expression("file >").unwrap_err().to_string(),
            "expected sort term (property or tag:name)"
        );
        assert_eq!(
            parse_expression("file >>").unwrap_err().to_string(),
            "expected sort term (property or tag:name) at '>'"
        );

        // Invalid direction specifiers
        assert_eq!(
            parse_expression("file:up").unwrap_err().to_string(),
            "expected direction (asc, desc) at 'up'"
        );
        assert_eq!(
            parse_expression("file:").unwrap_err().to_string(),
            "expected direction (asc, desc)"
        );

        // Invalid tag syntax
        assert_eq!(
            parse_expression("tag").unwrap_err().to_string(),
            // "expected sort term (property or tag:name) at 'tag'"
            "expected tag name"
        );
        assert_eq!(
            parse_expression("tag:").unwrap_err().to_string(),
            "expected tag name"
        );
        assert_eq!(
            parse_expression("tag:feat:up").unwrap_err().to_string(),
            "expected direction (asc, desc) at 'up'"
        );
    }

    #[test]
    fn test_tag_sort_parser() {
        assert_eq!(
            tag_sort("tag:feature"),
            Ok((
                "",
                TagSorter {
                    tag_name: "feature".to_string(),
                    direction: Direction::Ascending
                }
            ))
        );
    }

    #[test]
    fn test_tag_sort_with_direction() {
        assert_eq!(
            tag_sort("tag:feature:desc"),
            Ok((
                "",
                TagSorter {
                    tag_name: "feature".to_string(),
                    direction: Direction::Descending
                }
            ))
        );
        assert_eq!(
            tag_sort("tag:fix:asc"),
            Ok((
                "",
                TagSorter {
                    tag_name: "fix".to_string(),
                    direction: Direction::Ascending
                }
            ))
        );
    }

    #[test]
    fn test_tag_sort_comparison() {
        let sorter = TagSorter {
            tag_name: "feature".to_string(),
            direction: Direction::Ascending,
        };
        let a = Todo::new(
            TodoInfoBuilder::default()
                .title("A".to_string())
                .tags(vec!["feature".to_string()])
                .build()
                .unwrap(),
            Location::default(),
        );
        let b = Todo::new(
            TodoInfoBuilder::default()
                .title("B".to_string())
                .tags(vec![])
                .build()
                .unwrap(),
            Location::default(),
        );
        assert_eq!(sorter.compare(&a, &b), std::cmp::Ordering::Less);
        assert_eq!(sorter.compare(&b, &a), std::cmp::Ordering::Greater);
    }

    #[test]
    fn test_tag_sort_both_have() {
        let sorter = TagSorter {
            tag_name: "feature".to_string(),
            direction: Direction::Ascending,
        };
        let a = Todo::new(
            TodoInfoBuilder::default()
                .title("A".to_string())
                .tags(vec!["feature".to_string()])
                .build()
                .unwrap(),
            Location::default(),
        );
        let b = Todo::new(
            TodoInfoBuilder::default()
                .title("B".to_string())
                .tags(vec!["feature".to_string()])
                .build()
                .unwrap(),
            Location::default(),
        );
        assert_eq!(sorter.compare(&a, &b), std::cmp::Ordering::Equal);
    }

    #[test]
    fn test_tag_sort_neither_have() {
        let sorter = TagSorter {
            tag_name: "feature".to_string(),
            direction: Direction::Ascending,
        };
        let a = Todo::new(
            TodoInfoBuilder::default()
                .title("A".to_string())
                .tags(vec![])
                .build()
                .unwrap(),
            Location::default(),
        );
        let b = Todo::new(
            TodoInfoBuilder::default()
                .title("B".to_string())
                .tags(vec![])
                .build()
                .unwrap(),
            Location::default(),
        );
        assert_eq!(sorter.compare(&a, &b), std::cmp::Ordering::Equal);
    }

    #[test]
    fn test_tag_sort_descending() {
        let sorter = TagSorter {
            tag_name: "feature".to_string(),
            direction: Direction::Descending,
        };
        let a = Todo::new(
            TodoInfoBuilder::default()
                .title("A".to_string())
                .tags(vec!["feature".to_string()])
                .build()
                .unwrap(),
            Location::default(),
        );
        let b = Todo::new(
            TodoInfoBuilder::default()
                .title("B".to_string())
                .tags(vec![])
                .build()
                .unwrap(),
            Location::default(),
        );
        assert_eq!(sorter.compare(&a, &b), std::cmp::Ordering::Greater);
        assert_eq!(sorter.compare(&b, &a), std::cmp::Ordering::Less);
    }

    #[test]
    fn test_tag_sort_in_pipeline() {
        let (i, s) = pipeline("tag:project1 > tag:fix > priority").expect("Failed to parse");
        assert_eq!(i, "");

        let a = Todo::new(
            TodoInfoBuilder::default()
                .title("A".to_string())
                .tags(vec!["project1".to_string(), "fix".to_string()])
                .priority(Some('A'))
                .build()
                .unwrap(),
            Location::default(),
        );
        let b = Todo::new(
            TodoInfoBuilder::default()
                .title("B".to_string())
                .tags(vec!["project2".to_string(), "fix".to_string()])
                .build()
                .unwrap(),
            Location::default(),
        );
        let c = Todo::new(
            TodoInfoBuilder::default()
                .title("C".to_string())
                .tags(vec!["project1".to_string()])
                .build()
                .unwrap(),
            Location::default(),
        );
        let d = Todo::new(
            TodoInfoBuilder::default()
                .title("D".to_string())
                .tags(vec![])
                .build()
                .unwrap(),
            Location::default(),
        );

        let mut todos = vec![d.clone(), c.clone(), b.clone(), a.clone()];
        todos.sort_by(|x, y| s.compare(x, y));

        // Expected order: A, C, B, D
        // - project1 todos first (A, C), then non-project1 (B, D)
        // - Within project1: fix-tagged A before non-fix C
        // - Within non-project1: B has project2 but that doesn't matter for this sort,
        //   but B has fix tag and D doesn't, so B comes before D
        assert_eq!(todos[0].title, "A");
        assert_eq!(todos[1].title, "C");
        assert_eq!(todos[2].title, "B");
        assert_eq!(todos[3].title, "D");
    }
}
