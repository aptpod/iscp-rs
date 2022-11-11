//! データ型に関するモジュールです。

use std::collections::HashMap;

use crate::msg;

pub type IdAliasMap = HashMap<msg::DataId, u32>;
pub type DataId = msg::DataId;
pub type UpstreamInfo = msg::UpstreamInfo;

/// データポイントです。
#[derive(Clone, PartialEq, Debug)]
pub struct DataPoint {
    /// データID
    pub id: DataId,
    /// データポイントに付与された経過時間
    pub elapsed_time: chrono::Duration,
    /// ペイロード
    pub payload: Vec<u8>,
}
impl DataPoint {
    pub fn id(&self) -> DataId {
        self.id.clone()
    }

    pub fn data_point_id(&self) -> DataPointId {
        DataPointId {
            id: self.id(),
            elapsed_time: self.elapsed_time,
        }
    }
}

impl Default for DataPoint {
    fn default() -> Self {
        Self {
            id: DataId::new("", ""),
            elapsed_time: chrono::Duration::zero(),
            payload: Vec::new(),
        }
    }
}

/// データポイントIDはデータIDと経過時間を保持する構造体です。
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct DataPointId {
    /// データID
    pub id: DataId,
    /// データポイントに付与された経過時間
    pub elapsed_time: chrono::Duration,
}

/// [`DataPoint`]に対するAckです。
#[derive(Clone, PartialEq, Debug)]
pub struct Ack {
    /// 結果コード
    pub result_code: msg::ResultCode,
    /// 結果文字列
    pub result_string: String,
    /// UpstreamChunkに含まれていたデータポイントID
    pub data_point_ids: Vec<DataPointId>,
}

/// ダウンストリームで取得したデータポイントです。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct DownstreamDataPoint {
    /// データポイント
    pub data_point: DataPoint,
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

/// E2Eのデータです。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct E2EData {
    /// データID
    pub id: DataId,
    /// ペイロード
    pub payload: Vec<u8>,
}

/// E2Eのアップストリームコールです。
#[derive(Clone, PartialEq, Debug)]
pub struct UpstreamCall {
    /// ノードID
    pub destination_node_id: String,
    /// データ
    pub data: E2EData,
}

/// E2Eのダウンストリームコールです。
#[derive(Clone, PartialEq, Debug)]
pub struct DownstreamCall {
    /// コールID
    pub call_id: String,
    /// 送信元ノードID
    pub source_node_id: String,
    /// データ
    pub data: E2EData,
}

/// E2Eのリプライ用のコールです。
pub struct UpstreamReplyCall {
    /// 受信したE2EDownstreamCallのコールID
    pub request_call_id: String,
    /// リプライ先のNodeID
    pub destination_node_id: String,
    /// データ
    pub data: E2EData,
}

/// E2Eのリプライ用のコールです。
pub struct DownstreamReplyCall {
    /// コールID
    pub call_id: String,
    /// リクエストコールID
    pub request_call_id: String,
    /// 送信元ノードID
    pub source_node_id: String,
    /// データ
    pub data: E2EData,
}

impl From<msg::DownstreamCall> for DownstreamCall {
    fn from(c: msg::DownstreamCall) -> Self {
        Self {
            call_id: c.call_id,
            source_node_id: c.source_node_id,
            data: E2EData {
                id: DataId::new(c.name, c.f_type),
                payload: c.payload,
            },
        }
    }
}

impl From<msg::DownstreamCall> for DownstreamReplyCall {
    fn from(c: msg::DownstreamCall) -> Self {
        Self {
            call_id: c.call_id,
            request_call_id: c.request_call_id,
            source_node_id: c.source_node_id,
            data: E2EData {
                id: DataId::new(c.name, c.f_type),
                payload: c.payload,
            },
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn data_point_id_hash() {
        type Test = HashMap<DataPointId, i32>;
        let mut test = Test::default();
        let dp_id = DataPointId {
            id: DataId::new("1", "1"),
            elapsed_time: chrono::Duration::zero(),
        };

        test.insert(dp_id.clone(), 1);
        assert!(test.get(&dp_id).is_some());
    }
}
