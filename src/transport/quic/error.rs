impl From<quinn::ConnectError> for crate::error::Error {
    fn from(e: quinn::ConnectError) -> Self {
        match e {
            quinn::ConnectError::InvalidDnsName(e) => Self::invalid_value(e),
            quinn::ConnectError::InvalidRemoteAddress(e) => Self::invalid_value(e),
            _ => Self::connect(e),
        }
    }
}

impl From<quinn::ConnectionError> for crate::error::Error {
    fn from(e: quinn::ConnectionError) -> Self {
        match e {
            quinn::ConnectionError::ApplicationClosed(_)
            | quinn::ConnectionError::LocallyClosed
            | quinn::ConnectionError::ConnectionClosed(_) => Self::ConnectionClosed("".into()),
            _ => Self::unexpected(e),
        }
    }
}

impl From<quinn::WriteError> for crate::error::Error {
    fn from(e: quinn::WriteError) -> Self {
        match e {
            quinn::WriteError::ConnectionLost(e) => Self::ConnectionClosed(format!("{:?}", e)),
            _ => Self::unexpected(e),
        }
    }
}
