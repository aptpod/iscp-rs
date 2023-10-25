use chrono::{TimeZone, Utc};
use uuid::Uuid;

use super::DataIdAliasMap;

/// アップストリーム開始要求です。
#[derive(PartialEq, Clone, Debug)]
pub struct UpstreamOpenRequest {
    /// リクエストID
    pub request_id: super::RequestId,
    /// セッションID
    pub session_id: String,
    /// Ackの返却間隔
    pub ack_interval: chrono::Duration,
    /// 有効期限
    pub expiry_interval: chrono::Duration,
    /// データIDリスト
    pub data_ids: Vec<super::DataId>,
    /// QoS
    pub qos: super::qos::QoS,
    /// 拡張フィールドに含まれている永続化フラグ
    pub persist: Option<bool>,
}

impl Default for UpstreamOpenRequest {
    fn default() -> Self {
        Self {
            request_id: super::RequestId::default(),
            session_id: String::new(),
            ack_interval: chrono::Duration::zero(),
            expiry_interval: chrono::Duration::zero(),
            data_ids: Vec::new(),
            qos: super::QoS::Unreliable,
            persist: None,
        }
    }
}

/// アップストリーム開始要求の応答です。
#[derive(PartialEq, Clone, Debug)]
pub struct UpstreamOpenResponse {
    /// リクエストID
    pub request_id: super::RequestId,
    /// 割り当てられたストリームID
    pub assigned_stream_id: Uuid,
    /// 割り当てられたストリームIDエイリアス
    pub assigned_stream_id_alias: u32,
    /// サーバー時刻
    pub server_time: chrono::DateTime<Utc>,
    /// 結果コード
    pub result_code: super::ResultCode,
    /// 結果文字列
    pub result_string: String,
    /// DataIDエイリアス
    pub data_id_aliases: DataIdAliasMap,
}

impl Default for UpstreamOpenResponse {
    fn default() -> Self {
        Self {
            request_id: super::RequestId::default(),
            assigned_stream_id: Uuid::default(),
            assigned_stream_id_alias: 0,
            server_time: Utc.with_ymd_and_hms(1970, 1, 1, 0, 0, 0).unwrap(),
            result_code: super::ResultCode::Succeeded,
            result_string: String::new(),
            data_id_aliases: DataIdAliasMap::default(),
        }
    }
}

/// アップストリーム再開要求です。
#[derive(PartialEq, Clone, Default, Debug)]
pub struct UpstreamResumeRequest {
    /// リクエストID
    pub request_id: super::RequestId,
    /// ストリームID
    pub stream_id: Uuid,
}

/// アップストリーム再開要求の応答です。
#[derive(PartialEq, Clone, Default, Debug)]
pub struct UpstreamResumeResponse {
    /// リクエストID
    pub request_id: super::RequestId,
    /// 割り当てられたストリームIDエイリアス
    pub assigned_stream_id_alias: u32,
    /// 結果コード
    pub result_code: super::ResultCode,
    /// 結果文字列
    pub result_string: String,
}

/// アップストリーム切断要求です。
#[derive(PartialEq, Clone, Default, Debug)]
pub struct UpstreamCloseRequest {
    /// リクエストID
    pub request_id: super::RequestId,
    /// ストリームID
    pub stream_id: Uuid,
    /// 総データポイント数
    pub total_data_points: u64,
    /// 最終シーケンス番号
    pub final_sequence_number: u32,
    /// 拡張フィールド
    pub extension_fields: Option<UpstreamCloseRequestExtensionFields>,
}

/// アップストリーム切断要求に含まれる拡張フィールドです。
#[derive(PartialEq, Clone, Default, Debug)]
pub struct UpstreamCloseRequestExtensionFields {
    ///  セッションをクローズするかどうか
    pub close_session: bool,
}

/// アップストリーム切断要求の応答です。
#[derive(PartialEq, Clone, Default, Debug)]
pub struct UpstreamCloseResponse {
    /// リクエストID
    pub request_id: super::RequestId,
    /// 結果コード
    pub result_code: super::ResultCode,
    /// 結果文字列
    pub result_string: String,
}
