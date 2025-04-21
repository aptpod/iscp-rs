use std::sync::Arc;

use bytes::Bytes;
use uuid::Uuid;

/// Data id.
pub type DataId = crate::message::DataId;

/// Represents a data point.
pub type DataPoint = crate::message::DataPoint;

/// Data filter.
pub type DataFilter = crate::message::DataFilter;

/// Filter for a downstream.
pub type DownstreamFilter = crate::message::DownstreamFilter;

/// Quality of Service of a stream.
pub type QoS = crate::message::QoS;

/// Grouped data points by that data id.
#[derive(Clone, PartialEq, Debug)]
pub struct DataPointGroup {
    /// Data id.
    pub data_id: DataId,
    /// Data points.
    pub data_points: Vec<DataPoint>,
}

/// Upstream chunk.
#[derive(Clone, PartialEq, Debug)]
pub struct UpstreamChunk {
    /// Sequence number in the upstream.
    pub sequence_number: u32,
    /// Data point groups to send by the upstream.
    pub data_point_groups: Vec<DataPointGroup>,
}

/// The server result of the upstream chunk.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct UpstreamChunkResult {
    /// Sequence number in the upstream.
    pub sequence_number: u32,
    /// Result code.
    pub result_code: crate::message::ResultCode,
    /// Result string.
    pub result_string: String,
}

/// A chunk received at a downstream.
#[derive(Clone, PartialEq, Debug)]
pub struct DownstreamChunk {
    /// Sequence number of this chunk.
    pub sequence_number: u32,
    /// Data points.
    pub data_point_groups: Vec<DataPointGroup>,
    /// Upstream information.
    pub upstream: Arc<UpstreamInfo>,
}

/// Metadata received by downstream.
#[derive(Clone, PartialEq, Debug)]
pub struct DownstreamMetadata {
    /// Node id of source
    pub source_node_id: String,
    /// Metadata.
    pub metadata: super::metadata::ReceivableMetadata,
}

/// Upstream information
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct UpstreamInfo {
    /// Session id.
    pub session_id: String,
    /// Stream id.
    pub stream_id: Uuid,
    /// Source node id.
    pub source_node_id: String,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct UpstreamCall {
    pub destination_node_id: String,
    pub name: String,
    pub type_: String,
    pub payload: Bytes,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct UpstreamReplyCall {
    pub request_call_id: String,
    pub destination_node_id: String,
    pub name: String,
    pub type_: String,
    pub payload: Bytes,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct DownstreamReplyCall {
    pub request_call_id: String,
    pub source_node_id: String,
    pub name: String,
    pub type_: String,
    pub payload: Bytes,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct DownstreamCall {
    pub call_id: String,
    pub source_node_id: String,
    pub name: String,
    pub type_: String,
    pub payload: Bytes,
}
