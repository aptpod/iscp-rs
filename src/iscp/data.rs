//! データ型に関するモジュールです。

use bytes::Bytes;
use std::collections::HashMap;

use crate::msg;

pub(crate) type IdAliasMap = HashMap<msg::DataId, u32>;
pub type DataId = msg::DataId;
pub type UpstreamInfo = msg::UpstreamInfo;

/// データポイントです。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DataPoint {
    /// データポイントに付与された経過時間
    pub elapsed_time: chrono::Duration,
    /// ペイロード
    pub payload: Bytes,
}

impl Default for DataPoint {
    fn default() -> Self {
        Self {
            elapsed_time: chrono::Duration::zero(),
            payload: Bytes::new(),
        }
    }
}

/// データポイントグループです。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DataPointGroup {
    /// データID
    pub id: DataId,
    /// データポイント
    pub data_points: Vec<DataPoint>,
}

/// アップストリームで送信するデータポイントです。
#[derive(Clone, PartialEq, Eq, Default, Debug)]
pub struct UpstreamChunk {
    /// シーケンス番号
    pub sequence_number: u32,
    /// データポイント
    pub data_point_groups: Vec<DataPointGroup>,
}

/// UpstreamChunkの処理結果です。
#[derive(Clone, PartialEq, Eq, Default, Debug)]
pub struct UpstreamChunkResult {
    /// シーケンス番号
    pub sequence_number: u32,
    /// 結果コード
    pub result_code: msg::ResultCode,
    /// 結果文字列
    pub result_string: String,
}

/// ダウンストリームで取得したデータポイントです。
#[derive(Clone, PartialEq, Eq, Default, Debug)]
pub struct DownstreamChunk {
    /// シーケンス番号
    pub sequence_number: u32,
    /// データポイント
    pub data_point_groups: Vec<DataPointGroup>,
    /// アップストリーム情報
    pub upstream: UpstreamInfo,
}

/// ダウンストリームで取得したメタデータです。
#[derive(Clone, PartialEq, Debug)]
pub struct DownstreamMetadata {
    /// 送信元ノードID
    pub source_node_id: String,
    /// メタデータ
    pub metadata: msg::ReceivableMetadata,
}

/// E2Eコールのデータです。
#[derive(Clone, PartialEq, Eq, Default, Debug)]
pub struct E2ECallData {
    /// 名称
    pub name: String,
    /// 型
    pub type_: String,
    /// ペイロード
    pub payload: Bytes,
}

/// E2Eのアップストリームコールです。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct UpstreamCall {
    /// 宛先ノードID
    pub destination_node_id: String,
    /// E2Eコールのデータ
    pub e2e_call_data: E2ECallData,
}

/// E2Eのダウンストリームコールです。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DownstreamCall {
    /// コールID
    pub call_id: String,
    /// 送信元ノードID
    pub source_node_id: String,
    /// E2Eコールのデータ
    pub e2e_call_data: E2ECallData,
}

/// アップストリームリプライコールです。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct UpstreamReplyCall {
    /// リクエストコールID
    pub request_call_id: String,
    /// 宛先ノードID
    pub destination_node_id: String,
    /// E2Eコールのデータ
    pub e2e_call_data: E2ECallData,
}

/// E2Eのリプライ用のコールです。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DownstreamReplyCall {
    /// リクエストコールID
    pub request_call_id: String,
    /// 送信元ノードID
    pub source_node_id: String,
    /// E2Eコールのデータ
    pub e2e_call_data: E2ECallData,
}

impl From<msg::DownstreamCall> for DownstreamCall {
    fn from(c: msg::DownstreamCall) -> Self {
        Self {
            call_id: c.call_id,
            source_node_id: c.source_node_id,
            e2e_call_data: E2ECallData {
                name: c.name,
                type_: c.type_,
                payload: c.payload,
            },
        }
    }
}

impl From<msg::DownstreamCall> for DownstreamReplyCall {
    fn from(c: msg::DownstreamCall) -> Self {
        Self {
            request_call_id: c.request_call_id,
            source_node_id: c.source_node_id,
            e2e_call_data: E2ECallData {
                name: c.name,
                type_: c.type_,
                payload: c.payload,
            },
        }
    }
}
