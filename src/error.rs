//! iSCPのエラー型を定義するモジュールです。

use std::fmt::Display;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc::error as mpsc, oneshot};
use tungstenite::error as wserror;

use crate::message;

/// iSCPのエラーをもつ`Result`です。
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// iSCPのエラー型です。
#[derive(Error, Clone, PartialEq, Debug)]
pub enum Error {
    // Connect represents an error on connecting to a host. The detail why is shown in the String.
    #[error("Connect: `{0}`")]
    Connect(String),

    // ConnectionClosed represents connection had already been closed.
    #[error("ConnectionClosed")]
    ConnectionClosed(String),

    // TimeOut represents the request was failed because timeout.
    #[error("TimeOut")]
    TimeOut,

    // StreamNotFound represents the target stream wat not found.
    #[error("Stream Not Found")]
    StreamNotFound,

    // Unexpected represents the unexpected error occurred. The detail why is shown in the String.
    // This error is almost internal client error.
    #[error("Unexpected: `{0}`")]
    Unexpected(String),

    // Encode represents the error occurred while encoding message. The detail why is shown in the String.
    // This error is almost internal client error.
    #[error("Encode: `{0}`")]
    Encode(String),

    // Decode represents the error occurred while encoding message. The detail why is shown in the String.
    // If this error occurred, the connecting broker may not be comply with ISCP protocol.
    #[error("Decode: `{0}`")]
    MalformedMessage(String),

    // FailedMessage represents the error in the ISCP protocol.
    #[error("Failed Message (ResultCode: {code:?}, detail: {detail:?})")]
    FailedMessage {
        code: message::ResultCode,
        detail: String,
    },

    // InvalidValue represents the value is invalid. The detail why is shown in the String.
    #[error("Invalid Value `{0}`")]
    InvalidValue(String),

    // MaxDataPointCount represents the current data point count was reached to max.
    // If more data points should be sent using upstream, current upstream must be closed and new upstream must be opened.
    #[error("Max Data Point Count")]
    MaxDataPointCount,

    // MaxSequenceNumber represents the current sequence number was reached to max.
    // If more data points should be sent using upstream, current upstream must be closed and new upstream must be opened.
    #[error("Max MaxSequence Number")]
    MaxSequenceNumber,

    // TLS error
    #[error("tls error")]
    Certificate(#[from] rustls::Error),

    // Certificate load failed
    #[error("invalid certificate")]
    CertificateLoad(String),
}

impl From<wserror::Error> for Error {
    fn from(e: wserror::Error) -> Self {
        use wserror::Error as err;
        match e {
            err::AlreadyClosed | err::ConnectionClosed => Self::ConnectionClosed("".into()),
            _ => Self::unexpected(e),
        }
    }
}

impl<T> From<mpsc::SendError<T>> for Error {
    fn from(e: mpsc::SendError<T>) -> Self {
        Error::Unexpected(e.to_string()) // channel closed
    }
}

impl From<oneshot::error::RecvError> for Error {
    fn from(e: oneshot::error::RecvError) -> Self {
        Error::unexpected(e) // channel closed
    }
}

impl From<broadcast::error::RecvError> for Error {
    fn from(e: broadcast::error::RecvError) -> Self {
        Error::unexpected(e)
    }
}

impl From<tokio::time::error::Elapsed> for Error {
    fn from(_e: tokio::time::error::Elapsed) -> Self {
        Error::TimeOut
    }
}

impl Error {
    pub fn unexpected<T: Display>(s: T) -> Self {
        Error::Unexpected(s.to_string())
    }

    pub fn invalid_value<T: Display>(s: T) -> Self {
        Error::InvalidValue(s.to_string())
    }

    pub fn failed_message<T: Display>(code: message::ResultCode, s: T) -> Self {
        Error::FailedMessage {
            code,
            detail: s.to_string(),
        }
    }

    pub fn connect<T: Display>(s: T) -> Self {
        Error::Connect(s.to_string())
    }

    pub fn encode<T: ToString>(s: T) -> Self {
        Error::Encode(s.to_string())
    }

    pub fn malformed_message<T: ToString>(s: T) -> Self {
        Error::MalformedMessage(s.to_string())
    }
}
