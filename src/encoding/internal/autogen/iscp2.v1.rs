#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StreamChunk {
    #[prost(uint32, tag = "1")]
    pub sequence_number: u32,
    #[prost(message, repeated, tag = "2")]
    pub data_point_groups: ::prost::alloc::vec::Vec<DataPointGroup>,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DataPointGroup {
    #[prost(message, repeated, tag = "3")]
    pub data_points: ::prost::alloc::vec::Vec<DataPoint>,
    #[prost(oneof = "data_point_group::DataIdOrAlias", tags = "1, 2")]
    pub data_id_or_alias: ::core::option::Option<data_point_group::DataIdOrAlias>,
}
/// Nested message and enum types in `DataPointGroup`.
pub mod data_point_group {
    #[derive(serde::Serialize, serde::Deserialize)]
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum DataIdOrAlias {
        #[prost(message, tag = "1")]
        DataId(super::DataId),
        #[prost(uint32, tag = "2")]
        DataIdAlias(u32),
    }
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DataPoint {
    #[prost(sint64, tag = "1")]
    pub elapsed_time: i64,
    #[prost(bytes = "bytes", tag = "2")]
    pub payload: ::prost::bytes::Bytes,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[derive(PartialOrd, Ord, Eq, Hash)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DataId {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    #[serde(rename = "type")]
    pub type_: ::prost::alloc::string::String,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DownstreamFilter {
    #[prost(string, tag = "1")]
    pub source_node_id: ::prost::alloc::string::String,
    #[prost(message, repeated, tag = "2")]
    pub data_filters: ::prost::alloc::vec::Vec<DataFilter>,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[derive(PartialOrd, Ord, Eq, Hash)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DataFilter {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    #[serde(rename = "type")]
    pub type_: ::prost::alloc::string::String,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[derive(num_derive::FromPrimitive, num_derive::ToPrimitive)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum QoS {
    Unreliable = 0,
    Reliable = 1,
    Partial = 2,
}
impl QoS {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            QoS::Unreliable => "UNRELIABLE",
            QoS::Reliable => "RELIABLE",
            QoS::Partial => "PARTIAL",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "UNRELIABLE" => Some(Self::Unreliable),
            "RELIABLE" => Some(Self::Reliable),
            "PARTIAL" => Some(Self::Partial),
            _ => None,
        }
    }
}
#[derive(serde::Serialize, serde::Deserialize)]
#[derive(num_derive::FromPrimitive, num_derive::ToPrimitive)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum ResultCode {
    Succeeded = 0,
    IncompatibleVersion = 1,
    MaximumDataIdAlias = 2,
    MaximumUpstreamAlias = 3,
    UnspecifiedError = 64,
    NoNodeId = 65,
    AuthFailed = 66,
    ConnectTimeout = 67,
    MalformedMessage = 68,
    ProtocolError = 69,
    AckTimeout = 70,
    InvalidPayload = 71,
    InvalidDataId = 72,
    InvalidDataIdAlias = 73,
    InvalidDataFilter = 74,
    StreamNotFound = 75,
    ResumeRequestConflict = 76,
    ProcessFailed = 77,
    DesiredQosNotSupported = 78,
    PingTimeout = 79,
    TooLargeMessageSize = 80,
    TooManyDataIdAliases = 81,
    TooManyStreams = 82,
    TooLongAckInterval = 83,
    TooManyDownstreamFilters = 84,
    TooManyDataFilters = 85,
    TooLongExpiryInterval = 86,
    TooLongPingTimeout = 87,
    TooShortPingInterval = 88,
    TooShortPingTimeout = 89,
    RateLimitReached = 90,
    NodeIdMismatch = 128,
    SessionNotFound = 129,
    SessionAlreadyClosed = 130,
    SessionCannotClosed = 131,
}
impl ResultCode {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            ResultCode::Succeeded => "SUCCEEDED",
            ResultCode::IncompatibleVersion => "INCOMPATIBLE_VERSION",
            ResultCode::MaximumDataIdAlias => "MAXIMUM_DATA_ID_ALIAS",
            ResultCode::MaximumUpstreamAlias => "MAXIMUM_UPSTREAM_ALIAS",
            ResultCode::UnspecifiedError => "UNSPECIFIED_ERROR",
            ResultCode::NoNodeId => "NO_NODE_ID",
            ResultCode::AuthFailed => "AUTH_FAILED",
            ResultCode::ConnectTimeout => "CONNECT_TIMEOUT",
            ResultCode::MalformedMessage => "MALFORMED_MESSAGE",
            ResultCode::ProtocolError => "PROTOCOL_ERROR",
            ResultCode::AckTimeout => "ACK_TIMEOUT",
            ResultCode::InvalidPayload => "INVALID_PAYLOAD",
            ResultCode::InvalidDataId => "INVALID_DATA_ID",
            ResultCode::InvalidDataIdAlias => "INVALID_DATA_ID_ALIAS",
            ResultCode::InvalidDataFilter => "INVALID_DATA_FILTER",
            ResultCode::StreamNotFound => "STREAM_NOT_FOUND",
            ResultCode::ResumeRequestConflict => "RESUME_REQUEST_CONFLICT",
            ResultCode::ProcessFailed => "PROCESS_FAILED",
            ResultCode::DesiredQosNotSupported => "DESIRED_QOS_NOT_SUPPORTED",
            ResultCode::PingTimeout => "PING_TIMEOUT",
            ResultCode::TooLargeMessageSize => "TOO_LARGE_MESSAGE_SIZE",
            ResultCode::TooManyDataIdAliases => "TOO_MANY_DATA_ID_ALIASES",
            ResultCode::TooManyStreams => "TOO_MANY_STREAMS",
            ResultCode::TooLongAckInterval => "TOO_LONG_ACK_INTERVAL",
            ResultCode::TooManyDownstreamFilters => "TOO_MANY_DOWNSTREAM_FILTERS",
            ResultCode::TooManyDataFilters => "TOO_MANY_DATA_FILTERS",
            ResultCode::TooLongExpiryInterval => "TOO_LONG_EXPIRY_INTERVAL",
            ResultCode::TooLongPingTimeout => "TOO_LONG_PING_TIMEOUT",
            ResultCode::TooShortPingInterval => "TOO_SHORT_PING_INTERVAL",
            ResultCode::TooShortPingTimeout => "TOO_SHORT_PING_TIMEOUT",
            ResultCode::RateLimitReached => "RATE_LIMIT_REACHED",
            ResultCode::NodeIdMismatch => "NODE_ID_MISMATCH",
            ResultCode::SessionNotFound => "SESSION_NOT_FOUND",
            ResultCode::SessionAlreadyClosed => "SESSION_ALREADY_CLOSED",
            ResultCode::SessionCannotClosed => "SESSION_CANNOT_CLOSED",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "SUCCEEDED" => Some(Self::Succeeded),
            "INCOMPATIBLE_VERSION" => Some(Self::IncompatibleVersion),
            "MAXIMUM_DATA_ID_ALIAS" => Some(Self::MaximumDataIdAlias),
            "MAXIMUM_UPSTREAM_ALIAS" => Some(Self::MaximumUpstreamAlias),
            "UNSPECIFIED_ERROR" => Some(Self::UnspecifiedError),
            "NO_NODE_ID" => Some(Self::NoNodeId),
            "AUTH_FAILED" => Some(Self::AuthFailed),
            "CONNECT_TIMEOUT" => Some(Self::ConnectTimeout),
            "MALFORMED_MESSAGE" => Some(Self::MalformedMessage),
            "PROTOCOL_ERROR" => Some(Self::ProtocolError),
            "ACK_TIMEOUT" => Some(Self::AckTimeout),
            "INVALID_PAYLOAD" => Some(Self::InvalidPayload),
            "INVALID_DATA_ID" => Some(Self::InvalidDataId),
            "INVALID_DATA_ID_ALIAS" => Some(Self::InvalidDataIdAlias),
            "INVALID_DATA_FILTER" => Some(Self::InvalidDataFilter),
            "STREAM_NOT_FOUND" => Some(Self::StreamNotFound),
            "RESUME_REQUEST_CONFLICT" => Some(Self::ResumeRequestConflict),
            "PROCESS_FAILED" => Some(Self::ProcessFailed),
            "DESIRED_QOS_NOT_SUPPORTED" => Some(Self::DesiredQosNotSupported),
            "PING_TIMEOUT" => Some(Self::PingTimeout),
            "TOO_LARGE_MESSAGE_SIZE" => Some(Self::TooLargeMessageSize),
            "TOO_MANY_DATA_ID_ALIASES" => Some(Self::TooManyDataIdAliases),
            "TOO_MANY_STREAMS" => Some(Self::TooManyStreams),
            "TOO_LONG_ACK_INTERVAL" => Some(Self::TooLongAckInterval),
            "TOO_MANY_DOWNSTREAM_FILTERS" => Some(Self::TooManyDownstreamFilters),
            "TOO_MANY_DATA_FILTERS" => Some(Self::TooManyDataFilters),
            "TOO_LONG_EXPIRY_INTERVAL" => Some(Self::TooLongExpiryInterval),
            "TOO_LONG_PING_TIMEOUT" => Some(Self::TooLongPingTimeout),
            "TOO_SHORT_PING_INTERVAL" => Some(Self::TooShortPingInterval),
            "TOO_SHORT_PING_TIMEOUT" => Some(Self::TooShortPingTimeout),
            "RATE_LIMIT_REACHED" => Some(Self::RateLimitReached),
            "NODE_ID_MISMATCH" => Some(Self::NodeIdMismatch),
            "SESSION_NOT_FOUND" => Some(Self::SessionNotFound),
            "SESSION_ALREADY_CLOSED" => Some(Self::SessionAlreadyClosed),
            "SESSION_CANNOT_CLOSED" => Some(Self::SessionCannotClosed),
            _ => None,
        }
    }
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ConnectRequest {
    #[prost(uint32, tag = "1")]
    pub request_id: u32,
    #[prost(string, tag = "2")]
    pub protocol_version: ::prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub node_id: ::prost::alloc::string::String,
    #[prost(uint32, tag = "4")]
    pub ping_interval: u32,
    #[prost(uint32, tag = "5")]
    pub ping_timeout: u32,
    #[prost(message, optional, tag = "6")]
    pub extension_fields: ::core::option::Option<
        extensions::ConnectRequestExtensionFields,
    >,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ConnectResponse {
    #[prost(uint32, tag = "1")]
    pub request_id: u32,
    #[prost(string, tag = "2")]
    pub protocol_version: ::prost::alloc::string::String,
    #[prost(enumeration = "ResultCode", tag = "3")]
    pub result_code: i32,
    #[prost(string, tag = "4")]
    pub result_string: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "5")]
    pub extension_fields: ::core::option::Option<
        extensions::ConnectResponseExtensionFields,
    >,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Disconnect {
    #[prost(enumeration = "ResultCode", tag = "1")]
    pub result_code: i32,
    #[prost(string, tag = "2")]
    pub result_string: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "3")]
    pub extension_fields: ::core::option::Option<extensions::DisconnectExtensionFields>,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BaseTime {
    #[prost(string, tag = "1")]
    pub session_id: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
    #[prost(uint32, tag = "3")]
    pub priority: u32,
    #[prost(uint64, tag = "4")]
    pub elapsed_time: u64,
    #[prost(sint64, tag = "5")]
    pub base_time: i64,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UpstreamOpen {
    #[prost(bytes = "bytes", tag = "1")]
    pub stream_id: ::prost::bytes::Bytes,
    #[prost(string, tag = "2")]
    pub session_id: ::prost::alloc::string::String,
    #[prost(enumeration = "QoS", tag = "3")]
    pub qos: i32,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UpstreamAbnormalClose {
    #[prost(bytes = "bytes", tag = "1")]
    pub stream_id: ::prost::bytes::Bytes,
    #[prost(string, tag = "2")]
    pub session_id: ::prost::alloc::string::String,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UpstreamResume {
    #[prost(bytes = "bytes", tag = "1")]
    pub stream_id: ::prost::bytes::Bytes,
    #[prost(string, tag = "2")]
    pub session_id: ::prost::alloc::string::String,
    #[prost(enumeration = "QoS", tag = "3")]
    pub qos: i32,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UpstreamNormalClose {
    #[prost(bytes = "bytes", tag = "1")]
    pub stream_id: ::prost::bytes::Bytes,
    #[prost(string, tag = "2")]
    pub session_id: ::prost::alloc::string::String,
    #[prost(uint64, tag = "3")]
    pub total_data_points: u64,
    #[prost(uint32, tag = "4")]
    pub final_sequence_number: u32,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DownstreamOpen {
    #[prost(bytes = "bytes", tag = "1")]
    pub stream_id: ::prost::bytes::Bytes,
    #[prost(message, repeated, tag = "2")]
    pub downstream_filters: ::prost::alloc::vec::Vec<DownstreamFilter>,
    #[prost(enumeration = "QoS", tag = "3")]
    pub qos: i32,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DownstreamAbnormalClose {
    #[prost(bytes = "bytes", tag = "1")]
    pub stream_id: ::prost::bytes::Bytes,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DownstreamResume {
    #[prost(bytes = "bytes", tag = "1")]
    pub stream_id: ::prost::bytes::Bytes,
    #[prost(message, repeated, tag = "2")]
    pub downstream_filters: ::prost::alloc::vec::Vec<DownstreamFilter>,
    #[prost(enumeration = "QoS", tag = "3")]
    pub qos: i32,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DownstreamNormalClose {
    #[prost(bytes = "bytes", tag = "1")]
    pub stream_id: ::prost::bytes::Bytes,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DownstreamOpenRequest {
    #[prost(uint32, tag = "1")]
    pub request_id: u32,
    #[prost(uint32, tag = "2")]
    pub desired_stream_id_alias: u32,
    #[prost(message, repeated, tag = "3")]
    pub downstream_filters: ::prost::alloc::vec::Vec<DownstreamFilter>,
    #[prost(uint32, tag = "4")]
    pub expiry_interval: u32,
    #[prost(map = "uint32, message", tag = "5")]
    pub data_id_aliases: ::std::collections::HashMap<u32, DataId>,
    #[prost(enumeration = "QoS", tag = "6")]
    pub qos: i32,
    #[prost(message, optional, tag = "7")]
    pub extension_fields: ::core::option::Option<
        extensions::DownstreamOpenRequestExtensionFields,
    >,
    #[prost(bool, tag = "8")]
    pub omit_empty_chunk: bool,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DownstreamOpenResponse {
    #[prost(uint32, tag = "1")]
    pub request_id: u32,
    #[prost(bytes = "bytes", tag = "2")]
    pub assigned_stream_id: ::prost::bytes::Bytes,
    #[prost(sint64, tag = "3")]
    pub server_time: i64,
    #[prost(enumeration = "ResultCode", tag = "4")]
    pub result_code: i32,
    #[prost(string, tag = "5")]
    pub result_string: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "6")]
    pub extension_fields: ::core::option::Option<
        extensions::DownstreamOpenResponseExtensionFields,
    >,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DownstreamResumeRequest {
    #[prost(uint32, tag = "1")]
    pub request_id: u32,
    #[prost(bytes = "bytes", tag = "2")]
    pub stream_id: ::prost::bytes::Bytes,
    #[prost(uint32, tag = "3")]
    pub desired_stream_id_alias: u32,
    #[prost(message, optional, tag = "4")]
    pub extension_fields: ::core::option::Option<
        extensions::DownstreamResumeRequestExtensionFields,
    >,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DownstreamResumeResponse {
    #[prost(uint32, tag = "1")]
    pub request_id: u32,
    #[prost(enumeration = "ResultCode", tag = "2")]
    pub result_code: i32,
    #[prost(string, tag = "3")]
    pub result_string: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "4")]
    pub extension_fields: ::core::option::Option<
        extensions::DownstreamResumeResponseExtensionFields,
    >,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DownstreamCloseRequest {
    #[prost(uint32, tag = "1")]
    pub request_id: u32,
    #[prost(bytes = "bytes", tag = "2")]
    pub stream_id: ::prost::bytes::Bytes,
    #[prost(message, optional, tag = "3")]
    pub extension_fields: ::core::option::Option<
        extensions::DownstreamCloseRequestExtensionFields,
    >,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DownstreamCloseResponse {
    #[prost(uint32, tag = "1")]
    pub request_id: u32,
    #[prost(enumeration = "ResultCode", tag = "2")]
    pub result_code: i32,
    #[prost(string, tag = "3")]
    pub result_string: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "4")]
    pub extension_fields: ::core::option::Option<
        extensions::DownstreamCloseResponseExtensionFields,
    >,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DownstreamChunk {
    #[prost(uint32, tag = "1")]
    pub stream_id_alias: u32,
    #[prost(message, optional, tag = "4")]
    pub stream_chunk: ::core::option::Option<StreamChunk>,
    #[prost(message, optional, tag = "5")]
    pub extension_fields: ::core::option::Option<
        extensions::DownstreamChunkExtensionFields,
    >,
    #[prost(oneof = "downstream_chunk::UpstreamOrAlias", tags = "2, 3")]
    pub upstream_or_alias: ::core::option::Option<downstream_chunk::UpstreamOrAlias>,
}
/// Nested message and enum types in `DownstreamChunk`.
pub mod downstream_chunk {
    #[derive(serde::Serialize, serde::Deserialize)]
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum UpstreamOrAlias {
        #[prost(message, tag = "2")]
        UpstreamInfo(super::UpstreamInfo),
        #[prost(uint32, tag = "3")]
        UpstreamAlias(u32),
    }
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DownstreamChunkAck {
    #[prost(uint32, tag = "1")]
    pub stream_id_alias: u32,
    #[prost(uint32, tag = "2")]
    pub ack_id: u32,
    #[prost(message, repeated, tag = "3")]
    pub results: ::prost::alloc::vec::Vec<DownstreamChunkResult>,
    #[prost(map = "uint32, message", tag = "4")]
    pub upstream_aliases: ::std::collections::HashMap<u32, UpstreamInfo>,
    #[prost(map = "uint32, message", tag = "5")]
    pub data_id_aliases: ::std::collections::HashMap<u32, DataId>,
    #[prost(message, optional, tag = "6")]
    pub extension_fields: ::core::option::Option<
        extensions::DownstreamChunkAckExtensionFields,
    >,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DownstreamChunkAckComplete {
    #[prost(uint32, tag = "1")]
    pub stream_id_alias: u32,
    #[prost(uint32, tag = "2")]
    pub ack_id: u32,
    #[prost(enumeration = "ResultCode", tag = "3")]
    pub result_code: i32,
    #[prost(string, tag = "4")]
    pub result_string: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "5")]
    pub extension_fields: ::core::option::Option<
        extensions::DownstreamChunkAckCompleteExtensionFields,
    >,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DownstreamMetadata {
    #[prost(uint32, tag = "1")]
    pub request_id: u32,
    #[prost(uint32, tag = "13")]
    pub stream_id_alias: u32,
    #[prost(string, tag = "11")]
    pub source_node_id: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "12")]
    pub extension_fields: ::core::option::Option<
        extensions::DownstreamMetadataExtensionFields,
    >,
    #[prost(
        oneof = "downstream_metadata::Metadata",
        tags = "2, 3, 4, 5, 6, 7, 8, 9, 10"
    )]
    pub metadata: ::core::option::Option<downstream_metadata::Metadata>,
}
/// Nested message and enum types in `DownstreamMetadata`.
pub mod downstream_metadata {
    #[derive(serde::Serialize, serde::Deserialize)]
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Metadata {
        #[prost(message, tag = "2")]
        BaseTime(super::BaseTime),
        #[prost(message, tag = "3")]
        UpstreamOpen(super::UpstreamOpen),
        #[prost(message, tag = "4")]
        UpstreamAbnormalClose(super::UpstreamAbnormalClose),
        #[prost(message, tag = "5")]
        UpstreamResume(super::UpstreamResume),
        #[prost(message, tag = "6")]
        UpstreamNormalClose(super::UpstreamNormalClose),
        #[prost(message, tag = "7")]
        DownstreamOpen(super::DownstreamOpen),
        #[prost(message, tag = "8")]
        DownstreamAbnormalClose(super::DownstreamAbnormalClose),
        #[prost(message, tag = "9")]
        DownstreamResume(super::DownstreamResume),
        #[prost(message, tag = "10")]
        DownstreamNormalClose(super::DownstreamNormalClose),
    }
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DownstreamMetadataAck {
    #[prost(uint32, tag = "1")]
    pub request_id: u32,
    #[prost(enumeration = "ResultCode", tag = "2")]
    pub result_code: i32,
    #[prost(string, tag = "3")]
    pub result_string: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "4")]
    pub extension_fields: ::core::option::Option<
        extensions::DownstreamMetadataAckExtensionFields,
    >,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UpstreamInfo {
    #[prost(string, tag = "1")]
    pub session_id: ::prost::alloc::string::String,
    #[prost(bytes = "bytes", tag = "2")]
    pub stream_id: ::prost::bytes::Bytes,
    #[prost(string, tag = "3")]
    pub source_node_id: ::prost::alloc::string::String,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DownstreamChunkResult {
    #[prost(bytes = "bytes", tag = "1")]
    pub stream_id_of_upstream: ::prost::bytes::Bytes,
    #[prost(uint32, tag = "2")]
    pub sequence_number_in_upstream: u32,
    #[prost(enumeration = "ResultCode", tag = "3")]
    pub result_code: i32,
    #[prost(string, tag = "4")]
    pub result_string: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "5")]
    pub extension_fields: ::core::option::Option<
        extensions::DownstreamChunkResultExtensionFields,
    >,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UpstreamCall {
    #[prost(string, tag = "1")]
    pub call_id: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub request_call_id: ::prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub destination_node_id: ::prost::alloc::string::String,
    #[prost(string, tag = "4")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag = "5")]
    #[serde(rename = "type")]
    pub type_: ::prost::alloc::string::String,
    #[prost(bytes = "bytes", tag = "6")]
    pub payload: ::prost::bytes::Bytes,
    #[prost(message, optional, tag = "7")]
    pub extension_fields: ::core::option::Option<
        extensions::UpstreamCallExtensionFields,
    >,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UpstreamCallAck {
    #[prost(string, tag = "1")]
    pub call_id: ::prost::alloc::string::String,
    #[prost(enumeration = "ResultCode", tag = "2")]
    pub result_code: i32,
    #[prost(string, tag = "3")]
    pub result_string: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "4")]
    pub extension_fields: ::core::option::Option<
        extensions::UpstreamCallAckExtensionFields,
    >,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DownstreamCall {
    #[prost(string, tag = "1")]
    pub call_id: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub request_call_id: ::prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub source_node_id: ::prost::alloc::string::String,
    #[prost(string, tag = "4")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag = "5")]
    #[serde(rename = "type")]
    pub type_: ::prost::alloc::string::String,
    #[prost(bytes = "bytes", tag = "6")]
    pub payload: ::prost::bytes::Bytes,
    #[prost(message, optional, tag = "7")]
    pub extension_fields: ::core::option::Option<
        extensions::DownstreamCallExtensionFields,
    >,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Ping {
    #[prost(uint32, tag = "1")]
    pub request_id: u32,
    #[prost(message, optional, tag = "2")]
    pub extension_fields: ::core::option::Option<extensions::PingExtensionFields>,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Pong {
    #[prost(uint32, tag = "1")]
    pub request_id: u32,
    #[prost(message, optional, tag = "2")]
    pub extension_fields: ::core::option::Option<extensions::PongExtensionFields>,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UpstreamOpenRequest {
    #[prost(uint32, tag = "1")]
    pub request_id: u32,
    #[prost(string, tag = "2")]
    pub session_id: ::prost::alloc::string::String,
    #[prost(uint32, tag = "3")]
    pub ack_interval: u32,
    #[prost(uint32, tag = "5")]
    pub expiry_interval: u32,
    #[prost(message, repeated, tag = "6")]
    pub data_ids: ::prost::alloc::vec::Vec<DataId>,
    #[prost(enumeration = "QoS", tag = "7")]
    pub qos: i32,
    #[prost(message, optional, tag = "8")]
    pub extension_fields: ::core::option::Option<
        extensions::UpstreamOpenRequestExtensionFields,
    >,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UpstreamOpenResponse {
    #[prost(uint32, tag = "1")]
    pub request_id: u32,
    #[prost(bytes = "bytes", tag = "2")]
    pub assigned_stream_id: ::prost::bytes::Bytes,
    #[prost(uint32, tag = "3")]
    pub assigned_stream_id_alias: u32,
    #[prost(map = "uint32, message", tag = "4")]
    pub data_id_aliases: ::std::collections::HashMap<u32, DataId>,
    #[prost(sint64, tag = "5")]
    pub server_time: i64,
    #[prost(enumeration = "ResultCode", tag = "6")]
    pub result_code: i32,
    #[prost(string, tag = "7")]
    pub result_string: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "8")]
    pub extension_fields: ::core::option::Option<
        extensions::UpstreamOpenResponseExtensionFields,
    >,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UpstreamResumeRequest {
    #[prost(uint32, tag = "1")]
    pub request_id: u32,
    #[prost(bytes = "bytes", tag = "2")]
    pub stream_id: ::prost::bytes::Bytes,
    #[prost(message, optional, tag = "3")]
    pub extension_fields: ::core::option::Option<
        extensions::UpstreamResumeRequestExtensionFields,
    >,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UpstreamResumeResponse {
    #[prost(uint32, tag = "1")]
    pub request_id: u32,
    #[prost(uint32, tag = "2")]
    pub assigned_stream_id_alias: u32,
    #[prost(enumeration = "ResultCode", tag = "3")]
    pub result_code: i32,
    #[prost(string, tag = "4")]
    pub result_string: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "5")]
    pub extension_fields: ::core::option::Option<
        extensions::UpstreamResumeResponseExtensionFields,
    >,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UpstreamCloseRequest {
    #[prost(uint32, tag = "1")]
    pub request_id: u32,
    #[prost(bytes = "bytes", tag = "2")]
    pub stream_id: ::prost::bytes::Bytes,
    #[prost(uint64, tag = "3")]
    pub total_data_points: u64,
    #[prost(uint32, tag = "4")]
    pub final_sequence_number: u32,
    #[prost(message, optional, tag = "5")]
    pub extension_fields: ::core::option::Option<
        extensions::UpstreamCloseRequestExtensionFields,
    >,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UpstreamCloseResponse {
    #[prost(uint32, tag = "1")]
    pub request_id: u32,
    #[prost(enumeration = "ResultCode", tag = "2")]
    pub result_code: i32,
    #[prost(string, tag = "3")]
    pub result_string: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "4")]
    pub extension_fields: ::core::option::Option<
        extensions::UpstreamCloseResponseExtensionFields,
    >,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UpstreamChunk {
    #[prost(uint32, tag = "1")]
    pub stream_id_alias: u32,
    #[prost(message, optional, tag = "2")]
    pub stream_chunk: ::core::option::Option<StreamChunk>,
    #[prost(message, repeated, tag = "3")]
    pub data_ids: ::prost::alloc::vec::Vec<DataId>,
    #[prost(message, optional, tag = "4")]
    pub extension_fields: ::core::option::Option<
        extensions::UpstreamChunkExtensionFields,
    >,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UpstreamChunkAck {
    #[prost(uint32, tag = "1")]
    pub stream_id_alias: u32,
    #[prost(message, repeated, tag = "2")]
    pub results: ::prost::alloc::vec::Vec<UpstreamChunkResult>,
    #[prost(map = "uint32, message", tag = "3")]
    pub data_id_aliases: ::std::collections::HashMap<u32, DataId>,
    #[prost(message, optional, tag = "4")]
    pub extension_fields: ::core::option::Option<
        extensions::UpstreamChunkAckExtensionFields,
    >,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UpstreamMetadata {
    #[prost(uint32, tag = "1")]
    pub request_id: u32,
    #[prost(message, optional, tag = "4")]
    pub extension_fields: ::core::option::Option<
        extensions::UpstreamMetadataExtensionFields,
    >,
    #[prost(oneof = "upstream_metadata::Metadata", tags = "2")]
    pub metadata: ::core::option::Option<upstream_metadata::Metadata>,
}
/// Nested message and enum types in `UpstreamMetadata`.
pub mod upstream_metadata {
    #[derive(serde::Serialize, serde::Deserialize)]
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Metadata {
        #[prost(message, tag = "2")]
        BaseTime(super::BaseTime),
    }
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UpstreamMetadataAck {
    #[prost(uint32, tag = "1")]
    pub request_id: u32,
    #[prost(enumeration = "ResultCode", tag = "2")]
    pub result_code: i32,
    #[prost(string, tag = "3")]
    pub result_string: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "4")]
    pub extension_fields: ::core::option::Option<
        extensions::UpstreamMetadataAckExtensionFields,
    >,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UpstreamChunkResult {
    #[prost(uint32, tag = "1")]
    pub sequence_number: u32,
    #[prost(enumeration = "ResultCode", tag = "2")]
    pub result_code: i32,
    #[prost(string, tag = "3")]
    pub result_string: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "4")]
    pub extension_fields: ::core::option::Option<
        extensions::UpstreamChunkResultExtensionFields,
    >,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Message {
    #[prost(
        oneof = "message::Message",
        tags = "1, 2, 3, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 128, 129, 130, 131, 132, 133, 134, 135, 136, 137, 138, 192, 193, 256, 257, 258"
    )]
    pub message: ::core::option::Option<message::Message>,
}
/// Nested message and enum types in `Message`.
pub mod message {
    #[derive(serde::Serialize, serde::Deserialize)]
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Message {
        /// Connect
        #[prost(message, tag = "1")]
        ConnectRequest(super::ConnectRequest),
        #[prost(message, tag = "2")]
        ConnectResponse(super::ConnectResponse),
        #[prost(message, tag = "3")]
        Disconnect(super::Disconnect),
        /// Upstream
        #[prost(message, tag = "64")]
        UpstreamOpenRequest(super::UpstreamOpenRequest),
        #[prost(message, tag = "65")]
        UpstreamOpenResponse(super::UpstreamOpenResponse),
        #[prost(message, tag = "66")]
        UpstreamResumeRequest(super::UpstreamResumeRequest),
        #[prost(message, tag = "67")]
        UpstreamResumeResponse(super::UpstreamResumeResponse),
        #[prost(message, tag = "68")]
        UpstreamCloseRequest(super::UpstreamCloseRequest),
        #[prost(message, tag = "69")]
        UpstreamCloseResponse(super::UpstreamCloseResponse),
        #[prost(message, tag = "70")]
        UpstreamChunk(super::UpstreamChunk),
        #[prost(message, tag = "71")]
        UpstreamChunkAck(super::UpstreamChunkAck),
        #[prost(message, tag = "72")]
        UpstreamMetadata(super::UpstreamMetadata),
        #[prost(message, tag = "73")]
        UpstreamMetadataAck(super::UpstreamMetadataAck),
        /// Downstream
        #[prost(message, tag = "128")]
        DownstreamOpenRequest(super::DownstreamOpenRequest),
        #[prost(message, tag = "129")]
        DownstreamOpenResponse(super::DownstreamOpenResponse),
        #[prost(message, tag = "130")]
        DownstreamResumeRequest(super::DownstreamResumeRequest),
        #[prost(message, tag = "131")]
        DownstreamResumeResponse(super::DownstreamResumeResponse),
        #[prost(message, tag = "132")]
        DownstreamCloseRequest(super::DownstreamCloseRequest),
        #[prost(message, tag = "133")]
        DownstreamCloseResponse(super::DownstreamCloseResponse),
        #[prost(message, tag = "134")]
        DownstreamChunk(super::DownstreamChunk),
        #[prost(message, tag = "135")]
        DownstreamChunkAck(super::DownstreamChunkAck),
        #[prost(message, tag = "136")]
        DownstreamChunkAckComplete(super::DownstreamChunkAckComplete),
        #[prost(message, tag = "137")]
        DownstreamMetadata(super::DownstreamMetadata),
        #[prost(message, tag = "138")]
        DownstreamMetadataAck(super::DownstreamMetadataAck),
        /// Ping/Pong
        #[prost(message, tag = "192")]
        Ping(super::Ping),
        #[prost(message, tag = "193")]
        Pong(super::Pong),
        /// E2E Call
        #[prost(message, tag = "256")]
        UpstreamCall(super::UpstreamCall),
        #[prost(message, tag = "257")]
        UpstreamCallAck(super::UpstreamCallAck),
        #[prost(message, tag = "258")]
        DownstreamCall(super::DownstreamCall),
    }
}
