pub type Result<T> = std::result::Result<T, Error>;
pub type Error = Box<dyn std::error::Error>;

// #[derive(Debug)]
// pub enum Error {
//     Custom(String),
// 
//     IOError(std::io::Error),
// }

// impl Error {
//  pub fn custom(value: impl std::fmt::Display) -> Self {
//      Self::Custom(value.to_string())
//  }
// }

// impl From<&str> for Error {
//  fn from(value: &str) -> Self {
//      Self::Custom(value.to_string())
//  }
// }

// impl std::fmt::Display for Error {
//  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//      use Error::*;
//      match self {
//          Custom(msg) => write!(f, "{}", msg),
//          IOError(msg) => write!(f, "io error: {}", msg),
//      }
//  }
// }

// impl std::error::Error for Error {}

// impl From<std::io::Error> for Error {
//  fn from(err: std::io::Error) -> Self {
//      Error::IOError(err.message().to_string())
//  }
// }
