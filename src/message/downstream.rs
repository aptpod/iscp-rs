use super::DataIdAliasMap;
use std::collections::HashMap;
use uuid::Uuid;

/// ダウンストリーム開始要求です。
///
/// ダウンストリーム開始要求を受信したブローカーは、ブローカーからノード方向のデータ送信ストリームを開始します。
#[derive(Clone, PartialEq, Debug)]
pub struct DownstreamOpenRequest {
    // message fields
    /// リクエストID
    pub request_id: super::RequestId,
    /// 割り当てを希望するストリームIDエイリアス
    pub desired_stream_id_alias: u32,
    /// ダウンストリームフィルタ
    pub downstream_filters: Vec<super::DownstreamFilter>,
    /// 有効期限
    pub expiry_interval: chrono::Duration,
    /// データIDエイリアス
    pub data_id_aliases: DataIdAliasMap,
    /// QoS
    pub qos: super::QoS,
}

impl Default for DownstreamOpenRequest {
    fn default() -> Self {
        Self {
            request_id: super::RequestId::default(),
            desired_stream_id_alias: 0,
            downstream_filters: Vec::new(),
            expiry_interval: chrono::Duration::zero(),
            data_id_aliases: HashMap::new(),
            qos: super::QoS::Unreliable,
        }
    }
}

/// ダウンストリーム開始要求に対する応答です。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct DownstreamOpenResponse {
    /// リクエストID
    pub request_id: super::RequestId,
    /// 割り当てられたストリームID
    pub assigned_stream_id: Uuid,
    /// サーバー時刻
    pub server_time: chrono::DateTime<chrono::Utc>,
    /// 結果コード
    pub result_code: super::ResultCode,
    /// 結果文字列
    pub result_string: String,
}

/// ダウンストリーム再開要求です。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct DownstreamResumeRequest {
    /// リクエストID
    pub request_id: super::RequestId,
    /// ストリームID
    pub stream_id: Uuid,
    /// 割り当てを希望するストリームIDエイリアス
    pub desired_stream_id_alias: u32,
}

/// ダウンストリーム再開要求に対する応答です。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct DownstreamResumeResponse {
    /// リクエストID
    pub request_id: super::RequestId,
    /// 結果コード
    pub result_code: super::ResultCode,
    /// 結果文字列
    pub result_string: String,
}

/// ダウンストリーム切断要求です。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct DownstreamCloseRequest {
    /// リクエストID
    pub request_id: super::RequestId,
    /// ストリームID
    pub stream_id: Uuid,
}

/// ダウンストリーム切断要求に対する応答です。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct DownstreamCloseResponse {
    /// リクエストID
    pub request_id: super::RequestId,
    /// 結果コード
    pub result_code: super::ResultCode,
    /// 結果文字列
    pub result_string: String,
}
