mod error;
mod git;
mod local;

use crate::todo::Todo;
use error::Result;

pub use git::GitIDStrategy;
pub use local::MergeFileIDStrategy;

pub trait IDStrategy {
    fn next(&mut self, todo: &Todo) -> Result<u32>;
}
