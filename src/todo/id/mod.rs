mod error;
mod git;
mod local;

use error::Result;

pub trait IDStrategy {
    fn next(&mut self) -> Result<u32>;
}
