pub type Result<T> = core::result::Result<T, Error>;
// pub type Error = Box<dyn std::error::Error>;

/// Errors that can occur when interacting with VCS.
#[derive(Debug)]
pub enum Error {
    Custom(String),

    /// The path is not within a VCS repository.
    NotARepository,
    /// An error occurred while interacting with git.
    GitError(String),
    /// An error occurred while reading/writing the cache.
    CacheError(String),
    /// An error occurred while parsing VCS data.
    ParseError(String),
    /// An error occurred while constructing data.
    DataError(String),
    VersionError(String),
}

impl Error {
    pub fn custom(value: impl std::fmt::Display) -> Self {
        Self::Custom(value.to_string())
    }
}

impl From<&str> for Error {
    fn from(value: &str) -> Self {
        Self::Custom(value.to_string())
    }
}

impl From<Box<dyn std::error::Error>> for Error {
    fn from(value: Box<dyn std::error::Error>) -> Self {
        Self::Custom(value.to_string())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use Error::*;
        match self {
            Custom(msg) => write!(f, "{}", msg),
            NotARepository => write!(f, "not a VCS repository"),
            GitError(msg) => write!(f, "git error: {}", msg),
            CacheError(msg) => write!(f, "cache error: {}", msg),
            ParseError(msg) => write!(f, "parse error: {}", msg),
            DataError(msg) => write!(f, "data error: {}", msg),
            VersionError(msg) => write!(f, "version error: {}", msg),
        }
    }
}

impl std::error::Error for Error {}

impl From<git2::Error> for Error {
    fn from(err: git2::Error) -> Self {
        Error::GitError(err.message().to_string())
    }
}

impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Self {
        Error::CacheError(err.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::CacheError(err.to_string())
    }
}

impl From<crate::todo::syntax::TodoInfoBuilderError> for Error {
    fn from(err: crate::todo::syntax::TodoInfoBuilderError) -> Self {
        Error::DataError(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vcs_error_display() {
        assert_eq!(format!("{}", Error::NotARepository), "not a VCS repository");
        assert_eq!(
            format!("{}", Error::GitError("test".to_string())),
            "git error: test"
        );
        assert_eq!(
            format!("{}", Error::CacheError("test".to_string())),
            "cache error: test"
        );
        assert_eq!(
            format!("{}", Error::ParseError("test".to_string())),
            "parse error: test"
        );
        assert_eq!(
            format!("{}", Error::DataError("test".to_string())),
            "data error: test"
        );
    }

}
