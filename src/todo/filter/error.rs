use nom::error::{VerboseError, VerboseErrorKind};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    ParserError { message: String },
}

impl std::error::Error for Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::ParserError { message } => write!(f, "{}", message),
        }
    }
}

impl From<nom::Err<VerboseError<&str>>> for Error {
    fn from(value: nom::Err<VerboseError<&str>>) -> Self {
        let message = match &value {
            nom::Err::Error(e) | nom::Err::Failure(e) => {
                // Find context messages (innermost first in nom's VerboseError)
                let contexts: Vec<&str> = e
                    .errors
                    .iter()
                    .filter_map(|(_, kind)| match kind {
                        VerboseErrorKind::Context(ctx) => Some(*ctx),
                        _ => None,
                    })
                    .collect();

                let input = e
                    .errors
                    .first()
                    .map(|(i, _)| *i)
                    .unwrap_or("")
                    .chars()
                    .take(20)
                    .collect::<String>();

                let input_msg = if input.trim().is_empty() {
                    "".to_string()
                } else {
                    format!(" at '{}'", input)
                };

                if let Some(ctx) = contexts.first() {
                    format!("expected {}{}", ctx, input_msg)
                } else {
                    format!("unexpected '{}' in filter expression", input)
                }
            }
            nom::Err::Incomplete(_) => "filter expression ended unexpectedly".to_string(),
        };
        Self::ParserError { message }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nom::error::ErrorKind;

    #[test]
    fn test_verbose_error_with_context() {
        let verbose_err = VerboseError {
            errors: vec![
                ("bad input", VerboseErrorKind::Nom(ErrorKind::Tag)),
                (
                    "bad input",
                    VerboseErrorKind::Context("property (file, priority, tag, creation_date, completion_date)"),
                ),
            ],
        };
        let nom_err: nom::Err<VerboseError<&str>> = nom::Err::Error(verbose_err);
        let err: Error = nom_err.into();
        assert_eq!(
            err.to_string(),
            "expected property (file, priority, tag, creation_date, completion_date) at 'bad input'"
        );
    }

    #[test]
    fn test_verbose_error_without_context() {
        let verbose_err = VerboseError {
            errors: vec![("bad input", VerboseErrorKind::Nom(ErrorKind::Tag))],
        };
        let nom_err: nom::Err<VerboseError<&str>> = nom::Err::Error(verbose_err);
        let err: Error = nom_err.into();
        assert_eq!(err.to_string(), "unexpected 'bad input' in filter expression");
    }

    #[test]
    fn test_verbose_error_empty_input() {
        let verbose_err = VerboseError {
            errors: vec![
                ("", VerboseErrorKind::Nom(ErrorKind::Tag)),
                ("", VerboseErrorKind::Context("property filter")),
            ],
        };
        let nom_err: nom::Err<VerboseError<&str>> = nom::Err::Error(verbose_err);
        let err: Error = nom_err.into();
        assert_eq!(err.to_string(), "expected property filter");
    }

    #[test]
    fn test_from_nom_incomplete_error() {
        let nom_err: nom::Err<VerboseError<&str>> = nom::Err::Incomplete(nom::Needed::Unknown);
        let err: Error = nom_err.into();
        assert_eq!(err.to_string(), "filter expression ended unexpectedly");
    }
}
