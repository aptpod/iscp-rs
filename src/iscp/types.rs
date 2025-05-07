use std::sync::Arc;

use bytes::Bytes;
use uuid::Uuid;

/// データID
pub type DataId = crate::message::DataId;

/// データポイント
pub type DataPoint = crate::message::DataPoint;

/// データフィルタ
pub type DataFilter = crate::message::DataFilter;

/// ダウンストリームフィルタ
pub type DownstreamFilter = crate::message::DownstreamFilter;

/// ストリームのQoS
pub type QoS = crate::message::QoS;

/// データポイントグループ
#[derive(Clone, PartialEq, Debug)]
pub struct DataPointGroup {
    /// データ型
    pub data_id: DataId,
    /// データポイント
    pub data_points: Vec<DataPoint>,
}

/// アップストリームチャンク
#[derive(Clone, PartialEq, Debug)]
pub struct UpstreamChunk {
    /// アップストリーム内のシーケンス番号
    pub sequence_number: u32,
    /// アップストリームするデータポイントグループ
    pub data_point_groups: Vec<DataPointGroup>,
}

/// アップストリームチャンクのリザルト
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct UpstreamChunkResult {
    /// アップストリーム内のシーケンス番号
    pub sequence_number: u32,
    /// リザルトコード
    pub result_code: crate::message::ResultCode,
    /// リザルト文字列
    pub result_string: String,
}

/// ダウンストリームチャンク
#[derive(Clone, PartialEq, Debug)]
pub struct DownstreamChunk {
    /// ダウンストリーム内のシーケンス番号
    pub sequence_number: u32,
    /// ダウンストリームしたデータポイントグループ
    pub data_point_groups: Vec<DataPointGroup>,
    /// アップストリームの情報
    pub upstream: Arc<UpstreamInfo>,
}

/// ダウンストリームしたメタデータ
#[derive(Clone, PartialEq, Debug)]
pub struct DownstreamMetadata {
    /// 送信元のノードID
    pub source_node_id: String,
    /// メタデータ
    pub metadata: super::metadata::ReceivableMetadata,
}

/// アップストリーム情報
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct UpstreamInfo {
    /// セッションID
    pub session_id: String,
    /// ストリームID
    pub stream_id: Uuid,
    /// 送信元のノードID
    pub source_node_id: String,
}

/// E2Eのアップストリームコール
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct UpstreamCall {
    pub destination_node_id: String,
    pub name: String,
    pub type_: String,
    pub payload: Bytes,
}

/// E2Eのアップストリームリプライコール
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct UpstreamReplyCall {
    pub request_call_id: String,
    pub destination_node_id: String,
    pub name: String,
    pub type_: String,
    pub payload: Bytes,
}

/// E2Eのダウンストリームリプライコール
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct DownstreamReplyCall {
    pub request_call_id: String,
    pub source_node_id: String,
    pub name: String,
    pub type_: String,
    pub payload: Bytes,
}

/// E2Eのダウンストリームコール
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct DownstreamCall {
    pub call_id: String,
    pub source_node_id: String,
    pub name: String,
    pub type_: String,
    pub payload: Bytes,
}
