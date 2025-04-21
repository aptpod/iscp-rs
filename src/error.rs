use std::borrow::Cow;

use thiserror::Error;

/// iSCP error type.
#[non_exhaustive]
#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Transport(#[from] crate::transport::TransportError),

    #[error("{0}")]
    TokenSource(#[from] crate::token_source::TokenSourceError),

    /// Receive result code that is not succeeded.
    #[error("result code is: {result_code:?}, detail: {detail:?})")]
    FailedMessage {
        result_code: crate::message::ResultCode,
        detail: String,
    },

    /// The request was failed because timeout.
    #[error("timeout {0}")]
    Timeout(Cow<'static, str>),

    /// Connection had already been closed.
    #[error("connection closed")]
    ConnectionClosed,

    /// Operation is cancelled because of closed.
    #[error("cancelled by close")]
    CancelledByClose,

    /// The stream had already been closed.
    #[error("stream closed")]
    StreamClosed,

    ///The unexpected error occurred.
    #[error("unexpected: {0}")]
    Unexpected(Cow<'static, str>),

    /// The value is invalid. The detail why is shown in the String.
    #[error("invalid value `{0}`")]
    InvalidValue(Cow<'static, str>),

    /// Reordering error
    #[error("cannot wait chunk in reordering, the upstream id is `{0}`")]
    Reordering(uuid::Uuid),
}

impl Error {
    pub(crate) fn can_retry(&self) -> bool {
        matches!(
            self,
            Error::Transport(..) | Error::ConnectionClosed | Error::CancelledByClose
        )
    }

    pub(crate) fn result_code(&self) -> Option<crate::message::ResultCode> {
        match self {
            Error::FailedMessage { result_code, .. } => Some(*result_code),
            _ => None,
        }
    }

    pub(crate) fn timeout<T: Into<Cow<'static, str>>>(msg: T) -> Self {
        Self::Timeout(msg.into())
    }

    pub(crate) fn unexpected<T: Into<Cow<'static, str>>>(msg: T) -> Self {
        Self::Unexpected(msg.into())
    }

    pub(crate) fn invalid_value<T: Into<Cow<'static, str>>>(msg: T) -> Self {
        Self::InvalidValue(msg.into())
    }
}
