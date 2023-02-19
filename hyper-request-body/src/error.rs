use core::fmt;

use hyper::Error as HyperError;

//
#[derive(Debug)]
pub enum Error {
    HyperError(HyperError),
    Other(Box<dyn std::error::Error + Send + Sync + 'static>),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

//
impl From<HyperError> for Error {
    fn from(err: HyperError) -> Self {
        Self::HyperError(err)
    }
}
