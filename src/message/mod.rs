//! iSCPのメッセージ構造体を格納したモジュールです。

mod connect;
mod data;
mod downstream;
mod e2e;
mod filter;
mod metadata;
mod ping_pong;
mod qos;
mod result;
mod upstream;
pub use connect::*;
pub use data::*;
pub use downstream::*;
pub use e2e::*;
pub use filter::*;
pub use metadata::*;
pub use ping_pong::*;
pub use qos::*;
pub use result::*;
pub use upstream::*;

use std::collections::HashMap;

use crate::error::{Error, Result};

#[derive(Clone, Copy, Eq, PartialEq, Hash, Default, Debug)]
pub struct RequestId(u32);
impl RequestId {
    pub fn new(v: u32) -> Self {
        Self(v)
    }
    pub fn new_as_edge() -> Self {
        Self(0)
    }
    pub fn value(&self) -> u32 {
        self.0
    }
    pub fn set(&mut self, v: u32) {
        self.0 = v
    }
    pub fn increment(&mut self) -> &mut Self {
        self.0 += 2;
        self
    }
}
impl From<u32> for RequestId {
    fn from(v: u32) -> Self {
        Self::new(v)
    }
}

pub use crate::encoding::internal::autogen::DataId;

impl DataId {
    pub fn new<T1: Into<String>, T2: Into<String>>(name: T1, f_type: T2) -> Self {
        Self {
            name: name.into(),
            r#type: f_type.into(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        let list = ["#", "+", ":"];
        for c in list.iter() {
            if self.name.contains(c) || self.r#type.contains(c) {
                return Err(Error::invalid_value(format!("cannot use {:?}", list)));
            }
        }

        Ok(())
    }

    pub fn parse_str(data_id: &str) -> Result<Self> {
        let split = data_id.split(':').collect::<Vec<_>>();
        if split.len() != 2 {
            return Err(Error::invalid_value("splitter ':' not found"));
        }
        let id = Self {
            r#type: split[0].to_string(),
            name: split[1].to_string(),
        };
        if let Err(err) = id.validate() {
            return Err(err);
        }
        Ok(id)
    }
}

impl std::str::FromStr for DataId {
    type Err = crate::error::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_str(s)
    }
}

impl std::fmt::Display for DataId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.r#type, self.name)
    }
}

pub type DataIdAliasMap = HashMap<u32, DataId>;

#[derive(Clone, PartialEq, Debug)]
pub enum Message {
    // connect
    ConnectRequest(ConnectRequest),
    ConnectResponse(ConnectResponse),
    Disconnect(Disconnect),

    // upstream control
    UpstreamOpenRequest(UpstreamOpenRequest),
    UpstreamOpenResponse(UpstreamOpenResponse),
    UpstreamResumeRequest(UpstreamResumeRequest),
    UpstreamResumeResponse(UpstreamResumeResponse),
    UpstreamCloseRequest(UpstreamCloseRequest),
    UpstreamCloseResponse(UpstreamCloseResponse),

    // downstream control
    DownstreamOpenRequest(DownstreamOpenRequest),
    DownstreamOpenResponse(DownstreamOpenResponse),
    DownstreamResumeRequest(DownstreamResumeRequest),
    DownstreamResumeResponse(DownstreamResumeResponse),
    DownstreamCloseRequest(DownstreamCloseRequest),
    DownstreamCloseResponse(DownstreamCloseResponse),

    // e2e
    UpstreamCall(UpstreamCall),
    UpstreamCallAck(UpstreamCallAck),
    DownstreamCall(DownstreamCall),

    // ping pong
    Ping(Ping),
    Pong(Pong),

    // stream messages
    UpstreamChunk(UpstreamChunk),
    UpstreamChunkAck(UpstreamChunkAck),
    DownstreamChunk(DownstreamChunk),
    DownstreamChunkAck(DownstreamChunkAck),
    DownstreamChunkAckComplete(DownstreamChunkAckComplete),

    // metadata
    UpstreamMetadata(UpstreamMetadata),
    UpstreamMetadataAck(UpstreamMetadataAck),
    DownstreamMetadata(DownstreamMetadata),
    DownstreamMetadataAck(DownstreamMetadataAck),
}

// from
macro_rules! impl_from {
    ($($member:ident,)*) => {
        $(
            impl From<$member> for Message {
                fn from(m: $member) -> Self {
                    Self::$member(m)
                }
            }
            impl TryFrom<Message> for $member {
                type Error = crate::error::Error;
                fn try_from(m: Message) -> core::result::Result<Self, Self::Error> {
                    match m {
                        Message::$member(p) => Ok(p),
                        _ => Err(Error::unexpected("unmatched message")),
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
    UpstreamOpenRequest,
    UpstreamOpenResponse,
    UpstreamResumeRequest,
    UpstreamResumeResponse,
    UpstreamCloseRequest,
    UpstreamCloseResponse,
    DownstreamOpenRequest,
    DownstreamOpenResponse,
    DownstreamResumeRequest,
    DownstreamResumeResponse,
    DownstreamCloseRequest,
    DownstreamCloseResponse,
    UpstreamCall,
    UpstreamCallAck,
    DownstreamCall,
    Ping,
    Pong,
    UpstreamChunk,
    UpstreamChunkAck,
    DownstreamChunk,
    DownstreamChunkAck,
    DownstreamChunkAckComplete,
    UpstreamMetadata,
    UpstreamMetadataAck,
    DownstreamMetadata,
    DownstreamMetadataAck,
);

// error
macro_rules! impl_error {
    ($($member:ident,)*) => {
        $(
            impl From<$member> for Error {
                fn from(m: $member) -> Error {
                    Error::FailedMessage {
                        code: m.result_code,
                        detail: m.result_string,
                    }
                }
            }
        )*
    }
}
impl_error!(
    ConnectResponse,
    Disconnect,
    UpstreamOpenResponse,
    UpstreamResumeResponse,
    UpstreamCloseResponse,
    UpstreamChunkResult,
    UpstreamMetadataAck,
    DownstreamOpenResponse,
    DownstreamResumeResponse,
    DownstreamCloseResponse,
    DownstreamChunkResult,
    DownstreamMetadataAck,
);

impl Message {
    pub fn request_id(&self) -> Option<RequestId> {
        match &self {
            // connect
            Self::ConnectRequest(m) => Some(m.request_id),
            Self::ConnectResponse(m) => Some(m.request_id),

            // upstream
            Self::UpstreamOpenRequest(m) => Some(m.request_id),
            Self::UpstreamOpenResponse(m) => Some(m.request_id),
            Self::UpstreamResumeRequest(m) => Some(m.request_id),
            Self::UpstreamResumeResponse(m) => Some(m.request_id),
            Self::UpstreamCloseRequest(m) => Some(m.request_id),
            Self::UpstreamCloseResponse(m) => Some(m.request_id),

            // downstream
            Self::DownstreamOpenRequest(m) => Some(m.request_id),
            Self::DownstreamOpenResponse(m) => Some(m.request_id),
            Self::DownstreamResumeRequest(m) => Some(m.request_id),
            Self::DownstreamResumeResponse(m) => Some(m.request_id),
            Self::DownstreamCloseRequest(m) => Some(m.request_id),
            Self::DownstreamCloseResponse(m) => Some(m.request_id),

            // ping pong
            Self::Ping(m) => Some(m.request_id),
            Self::Pong(m) => Some(m.request_id),

            // metadata
            Self::UpstreamMetadata(m) => Some(m.request_id),
            Self::UpstreamMetadataAck(m) => Some(m.request_id),
            Self::DownstreamMetadata(m) => Some(m.request_id),
            Self::DownstreamMetadataAck(m) => Some(m.request_id),
            _ => None,
        }
    }

    pub fn upstream_id_alias(&self) -> Option<u32> {
        match &self {
            Self::UpstreamChunk(m) => Some(m.stream_id_alias),
            Self::UpstreamChunkAck(m) => Some(m.stream_id_alias),
            _ => None,
        }
    }

    pub fn downstream_id_alias(&self) -> Option<u32> {
        match &self {
            Self::DownstreamChunk(m) => Some(m.stream_id_alias),
            Self::DownstreamChunkAck(m) => Some(m.stream_id_alias),
            Self::DownstreamChunkAckComplete(m) => Some(m.stream_id_alias),
            _ => None,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn request_id_and_stream_id_alias() {
        let requests: Vec<Message> = vec![
            Message::from(ConnectRequest::default()),
            Message::from(ConnectResponse::default()),
            // Message::from(Disconnect::default()),
            Message::from(UpstreamOpenRequest::default()),
            Message::from(UpstreamOpenResponse::default()),
            Message::from(UpstreamResumeRequest::default()),
            Message::from(UpstreamResumeResponse::default()),
            Message::from(UpstreamCloseRequest::default()),
            Message::from(UpstreamCloseResponse::default()),
            Message::from(DownstreamOpenRequest::default()),
            Message::from(DownstreamOpenResponse::default()),
            Message::from(DownstreamResumeRequest::default()),
            Message::from(DownstreamResumeResponse::default()),
            Message::from(DownstreamCloseRequest::default()),
            Message::from(DownstreamCloseResponse::default()),
            Message::from(Ping::default()),
            Message::from(Pong::default()),
            Message::from(UpstreamMetadata::default()),
            Message::from(UpstreamMetadataAck::default()),
            Message::from(DownstreamMetadata::default()),
            Message::from(DownstreamMetadataAck::default()),
        ];

        requests.into_iter().for_each(|m| {
            m.request_id().expect("exist request id");
            assert!(m.upstream_id_alias().is_none());
            assert!(m.downstream_id_alias().is_none());
        });

        // stream message
        let upstreams: Vec<Message> = vec![
            UpstreamChunk::default().into(),
            UpstreamChunkAck::default().into(),
        ];
        let downstreams: Vec<Message> = vec![
            DownstreamChunk::default().into(),
            DownstreamChunkAck::default().into(),
        ];
        upstreams.into_iter().for_each(|m| {
            assert!(m.upstream_id_alias().is_some());
            assert!(m.downstream_id_alias().is_none());
            assert!(m.request_id().is_none());
        });
        downstreams.into_iter().for_each(|m| {
            assert!(m.upstream_id_alias().is_none());
            assert!(m.downstream_id_alias().is_some());
            assert!(m.request_id().is_none());
        });
    }

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
            DataId::parse_str("type:name").unwrap(),
            DataId::new("name", "type"),
        );
        assert!(DataId::parse_str("type-name").is_err());
        assert!(DataId::parse_str("ty:pe:name").is_err());
        assert!(DataId::parse_str("ty#pe:name").is_err());
        assert!(DataId::parse_str("type:na+me").is_err());
    }
    #[test]
    fn data_id_format() {
        let id = DataId::parse_str("type:name").unwrap();
        assert_eq!(format!("{}", id), "type:name");
    }
}
