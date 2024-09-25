use thiserror::Error;

#[derive(Error, Copy, Clone, Eq, PartialEq, Debug)]
pub enum CrowdError {
    #[error("cannot create effect in a inactive scope")]
    InactiveScope,
    #[error("expect a Pluggable (FnOnce(Arc<Cortex>) -> color_eyre::Result, e.g.)")]
    InvalidPlug
}

pub(crate) type Result<T, E = Error> = ::std::result::Result<T, E>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Crowd(#[from] CrowdError),
    #[error("pnp panic: {0}")]
    PnpPanic(String),
    #[error("{0}")]
    Other(#[from] color_eyre::Report),
}