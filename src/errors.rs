use aws_smithy_client::SdkError;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

/// Backing errors for all AWS operations.
#[derive(Error, Debug)]
pub enum Error {
    #[error("failed API (message: {message:?}, retryable: {retryable:?})")]
    API { message: String, retryable: bool },
    #[error("failed for other reasons (message: {message:?}, retryable: {retryable:?})")]
    Other { message: String, retryable: bool },
}

impl Error {
    /// Returns the error message in "String".
    #[inline]
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Error::API { message, .. } | Error::Other { message, .. } => message.clone(),
        }
    }

    /// Returns if the error is retryable.
    #[inline]
    #[must_use]
    pub fn retryable(&self) -> bool {
        match self {
            Error::API { retryable, .. } | Error::Other { retryable, .. } => *retryable,
        }
    }
}

#[inline]
pub fn is_sdk_err_retryable<E>(e: &SdkError<E>) -> bool {
    match e {
        SdkError::TimeoutError(_) | SdkError::ResponseError { .. } => true,
        SdkError::DispatchFailure(e) => e.is_timeout() || e.is_io(),
        _ => false,
    }
}
