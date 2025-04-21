//! iSCP message types.

#[rustfmt::skip]
#[allow(clippy::all)]
mod proto {
    include!("proto/iscp2.v1.rs");

    #[path = "iscp2.v1.extensions.rs"]
    pub mod extensions;
}

pub use proto::*;

impl DataId {
    pub fn new<SN: ToString, ST: ToString>(name: SN, type_: ST) -> Self {
        Self {
            name: name.to_string(),
            type_: type_.to_string(),
        }
    }

    pub fn validate(&self) -> Result<(), DataIdParseError> {
        let list = ['#', '+', ':'];
        for &c in list.iter() {
            if self.name.contains(c) || self.type_.contains(c) {
                return Err(DataIdParseError::InvalidChar(c));
            }
        }

        Ok(())
    }
}

impl std::str::FromStr for DataId {
    type Err = DataIdParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut split = s.split(':');
        let Some(type_) = split.next() else {
            return Err(DataIdParseError::SplitterNotFound);
        };
        let Some(name) = split.next() else {
            return Err(DataIdParseError::SplitterNotFound);
        };
        if split.next().is_some() {
            return Err(DataIdParseError::MultipleSplitterFound);
        }

        let id = Self {
            type_: type_.to_owned(),
            name: name.to_owned(),
        };
        id.validate()?;
        Ok(id)
    }
}

impl std::fmt::Display for DataId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.type_, self.name)
    }
}

#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum DataIdParseError {
    #[error("splitter ':' not found")]
    SplitterNotFound,
    #[error("multiple splitter found")]
    MultipleSplitterFound,
    #[error("include invalid character")]
    InvalidChar(char),
}

#[derive(Debug, thiserror::Error)]
#[error("message convert error")]
pub struct MessageConvertError(pub Message);

macro_rules! impl_from {
    ($($member:ident,)*) => {
        $(
            impl From<$member> for Message {
                fn from(m: $member) -> Self {
                    Message {
                        message: Some(proto::message::Message::$member(m)),
                    }
                }
            }
            impl TryFrom<Message> for $member {
                type Error = MessageConvertError;
                fn try_from(m: Message) -> core::result::Result<Self, Self::Error> {
                    match m.message {
                        Some(proto::message::Message::$member(a)) => Ok(a),
                        _ => Err(MessageConvertError(m)),
                    }
                }
            }
        )*
    }
}

impl_from!(
    ConnectRequest,
    ConnectResponse,
    Disconnect,
    DownstreamCall,
    DownstreamChunk,
    DownstreamChunkAck,
    DownstreamChunkAckComplete,
    DownstreamCloseRequest,
    DownstreamCloseResponse,
    DownstreamMetadata,
    DownstreamMetadataAck,
    DownstreamOpenRequest,
    DownstreamOpenResponse,
    DownstreamResumeRequest,
    DownstreamResumeResponse,
    Ping,
    Pong,
    UpstreamCall,
    UpstreamCallAck,
    UpstreamChunk,
    UpstreamChunkAck,
    UpstreamCloseRequest,
    UpstreamCloseResponse,
    UpstreamMetadata,
    UpstreamMetadataAck,
    UpstreamOpenRequest,
    UpstreamOpenResponse,
    UpstreamResumeRequest,
    UpstreamResumeResponse,
);

mod private {
    pub trait Sealed {}
}

pub trait HasRequestId: private::Sealed {
    fn request_id(&self) -> u32;
    fn set_request_id(&mut self, request_id: u32);
}

macro_rules! impl_has_request_id {
    ($($member:ident,)*) => {
        $(
            impl private::Sealed for $member {}
            impl HasRequestId for $member {
                fn request_id(&self) -> u32 {
                    self.request_id
                }

                fn set_request_id(&mut self, request_id: u32) {
                    self.request_id = request_id;
                }
            }
        )*
    }
}

impl_has_request_id!(
    ConnectRequest,
    ConnectResponse,
    DownstreamCloseRequest,
    DownstreamCloseResponse,
    DownstreamMetadata,
    DownstreamMetadataAck,
    DownstreamOpenRequest,
    DownstreamOpenResponse,
    DownstreamResumeRequest,
    DownstreamResumeResponse,
    Ping,
    Pong,
    UpstreamCloseRequest,
    UpstreamCloseResponse,
    UpstreamMetadata,
    UpstreamMetadataAck,
    UpstreamOpenRequest,
    UpstreamOpenResponse,
    UpstreamResumeRequest,
    UpstreamResumeResponse,
);

pub trait HasResultCode: private::Sealed {
    fn result_code(&self) -> Option<ResultCode>;
    fn result_string(&self) -> &str;
}

macro_rules! impl_has_result_code {
    ($($member:ident,)*) => {
        $(
            impl HasResultCode for $member {
                fn result_code(&self) -> Option<ResultCode> {
                    self.result_code.try_into().ok()
                }

                fn result_string(&self) -> &str {
                    &self.result_string
                }
            }
        )*
    }
}

impl_has_result_code!(
    ConnectResponse,
    DownstreamCloseResponse,
    DownstreamMetadataAck,
    DownstreamOpenResponse,
    DownstreamResumeResponse,
    UpstreamCloseResponse,
    UpstreamMetadataAck,
    UpstreamOpenResponse,
    UpstreamResumeResponse,
);

pub trait RequestMessage: Into<Message> + HasRequestId {
    type Response: TryFrom<Message> + HasRequestId + HasResultCode;
}

impl RequestMessage for ConnectRequest {
    type Response = ConnectResponse;
}
impl RequestMessage for DownstreamCloseRequest {
    type Response = DownstreamCloseResponse;
}
impl RequestMessage for DownstreamMetadata {
    type Response = DownstreamMetadataAck;
}
impl RequestMessage for DownstreamOpenRequest {
    type Response = DownstreamOpenResponse;
}
impl RequestMessage for DownstreamResumeRequest {
    type Response = DownstreamResumeResponse;
}
impl RequestMessage for UpstreamCloseRequest {
    type Response = UpstreamCloseResponse;
}
impl RequestMessage for UpstreamMetadata {
    type Response = UpstreamMetadataAck;
}
impl RequestMessage for UpstreamOpenRequest {
    type Response = UpstreamOpenResponse;
}
impl RequestMessage for UpstreamResumeRequest {
    type Response = UpstreamResumeResponse;
}

impl Message {
    pub fn request_id(&self) -> Option<u32> {
        use proto::message::Message::*;
        let request_id = match self.message.as_ref()? {
            ConnectRequest(a) => a.request_id,
            ConnectResponse(a) => a.request_id,
            DownstreamCloseRequest(a) => a.request_id,
            DownstreamCloseResponse(a) => a.request_id,
            DownstreamMetadata(a) => a.request_id,
            DownstreamMetadataAck(a) => a.request_id,
            DownstreamOpenRequest(a) => a.request_id,
            DownstreamOpenResponse(a) => a.request_id,
            DownstreamResumeRequest(a) => a.request_id,
            DownstreamResumeResponse(a) => a.request_id,
            Ping(a) => a.request_id,
            Pong(a) => a.request_id,
            UpstreamCloseRequest(a) => a.request_id,
            UpstreamCloseResponse(a) => a.request_id,
            UpstreamMetadata(a) => a.request_id,
            UpstreamMetadataAck(a) => a.request_id,
            UpstreamOpenRequest(a) => a.request_id,
            UpstreamOpenResponse(a) => a.request_id,
            UpstreamResumeRequest(a) => a.request_id,
            UpstreamResumeResponse(a) => a.request_id,
            _ => {
                return None;
            }
        };
        Some(request_id)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn data_id_validate() {
        assert!(DataId::new("name", "type").validate().is_ok());
        assert!(DataId::new("na:me", "type").validate().is_err());
        assert!(DataId::new("na+me", "type").validate().is_err());
        assert!(DataId::new("na#me", "type").validate().is_err());
        assert!(DataId::new("name", "ty:pe").validate().is_err());
        assert!(DataId::new("name", "ty+pe").validate().is_err());
        assert!(DataId::new("name", "ty#pe").validate().is_err());
    }

    #[test]
    fn data_id_parse_str() {
        assert_eq!(
            DataId::from_str("type:name").unwrap(),
            DataId::new("name", "type"),
        );
        assert!(DataId::from_str("type-name").is_err());
        assert!(DataId::from_str("ty:pe:name").is_err());
        assert!(DataId::from_str("ty#pe:name").is_err());
        assert!(DataId::from_str("type:na+me").is_err());
    }

    #[test]
    fn data_id_format() {
        let id = DataId::from_str("type:name").unwrap();
        assert_eq!(format!("{}", id), "type:name");
    }
}
