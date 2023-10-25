use chrono::Utc;
use uuid::Uuid;

#[derive(Clone, PartialEq, Debug)]
pub enum Metadata {
    BaseTime(BaseTime),
    UpstreamOpen(UpstreamOpen),
    UpstreamAbnormalClose(UpstreamAbnormalClose),
    UpstreamResume(UpstreamResume),
    UpstreamNormalClose(UpstreamNormalClose),
    DownstreamOpen(DownstreamOpen),
    DownstreamAbnormalClose(DownstreamAbnormalClose),
    DownstreamResume(DownstreamResume),
    DownstreamNormalClose(DownstreamNormalClose),
}

#[derive(Clone, PartialEq, Debug)]
pub enum SendableMetadata {
    BaseTime(BaseTime),
}

#[derive(Clone, PartialEq, Debug)]
pub enum ReceivableMetadata {
    BaseTime(BaseTime),
    UpstreamOpen(UpstreamOpen),
    UpstreamAbnormalClose(UpstreamAbnormalClose),
    UpstreamResume(UpstreamResume),
    UpstreamNormalClose(UpstreamNormalClose),
    DownstreamOpen(DownstreamOpen),
    DownstreamAbnormalClose(DownstreamAbnormalClose),
    DownstreamResume(DownstreamResume),
    DownstreamNormalClose(DownstreamNormalClose),
}

impl Default for Metadata {
    fn default() -> Self {
        Self::BaseTime(BaseTime::default())
    }
}

/// 基準時刻です。
///
/// あるセッションの基準となる時刻です。
#[derive(Clone, PartialEq, Debug)]
pub struct BaseTime {
    /// セッションID
    pub session_id: String,
    /// 基準時刻の名称
    pub name: String,
    /// 優先度
    pub priority: u32,
    /// 経過時間
    pub elapsed_time: chrono::Duration,
    /// 基準時刻
    pub base_time: chrono::DateTime<Utc>,
}

impl Default for BaseTime {
    fn default() -> Self {
        Self {
            session_id: String::new(),
            name: String::new(),
            priority: 0,
            elapsed_time: chrono::Duration::zero(),
            base_time: chrono::DateTime::<Utc>::MIN_UTC,
        }
    }
}

/// あるアップストリームが開いたことを知らせるメタデータです。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct UpstreamOpen {
    /// ストリームID
    pub stream_id: Uuid,
    /// セッションID
    pub session_id: String,
    /// QoS
    pub qos: super::QoS,
}

/// あるアップストリームが異常切断したことを知らせるメタデータです。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct UpstreamAbnormalClose {
    /// ストリームID
    pub stream_id: Uuid,
    /// セッションID
    pub session_id: String,
}

/// あるアップストリームが再開したことを知らせるメタデータです。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct UpstreamResume {
    /// ストリームID
    pub stream_id: Uuid,
    /// セッションID
    pub session_id: String,
    /// QoS
    pub qos: super::QoS,
}

/// あるアップストリームが正常切断したことを知らせるメタデータです。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct UpstreamNormalClose {
    /// ストリームID
    pub stream_id: Uuid,
    /// セッションID
    pub session_id: Uuid,
    /// 総データポイント数
    pub total_data_points: u64,
    /// 最終シーケンス番号
    pub final_sequence_number: u32,
}

/// あるダウンストリームが開いたことを知らせるメタデータです。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct DownstreamOpen {
    /// ストリームID
    pub stream_id: Uuid,
    /// ダウンストリームフィルタ
    pub downstream_filters: Vec<super::DownstreamFilter>,
    /// QoS
    pub qos: super::QoS,
}

/// あるダウンストリームが異常切断したことを知らせるメタデータです。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct DownstreamAbnormalClose {
    /// ストリームID
    pub stream_id: Uuid,
}

/// あるダウンストリームが再開したことを知らせるメタデータです。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct DownstreamResume {
    /// ストリームID
    pub stream_id: Uuid,
    /// ダウンストリームフィルタ
    pub downstream_filters: Vec<super::DownstreamFilter>,
    /// QoS
    pub qos: super::QoS,
}

/// あるダウンストリームが正常切断したことを知らせるメタデータです。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct DownstreamNormalClose {
    /// ストリームID
    pub stream_id: Uuid,
}

/// アップストリームメタデータです。
#[derive(Clone, PartialEq, Debug)]
pub struct UpstreamMetadata {
    /// リクエストID
    pub request_id: super::RequestId,
    /// メタデータ
    pub metadata: SendableMetadata,
    /// 拡張フィールドに含まれている永続化フラグ
    pub persist: Option<bool>,
}

impl Default for UpstreamMetadata {
    fn default() -> Self {
        Self {
            request_id: 0.into(),
            metadata: SendableMetadata::BaseTime(BaseTime::default()),
            persist: None,
        }
    }
}

/// アップストリームメタデータに対する応答です。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct UpstreamMetadataAck {
    /// リクエストID
    pub request_id: super::RequestId,
    /// 結果コード
    pub result_code: super::ResultCode,
    /// 結果文字列
    pub result_string: String,
}

/// ダウンストリームメタデータです。
///
/// メタデータを格納してブローカーからノードへ転送するためのメッセージです。
#[derive(Clone, PartialEq, Debug)]
pub struct DownstreamMetadata {
    /// リクエストID
    pub request_id: super::RequestId,
    /// ストリームIDエイリアス
    pub stream_id_alias: u32,
    /// 生成元ノードID
    pub source_node_id: String,
    /// メタデータ
    pub metadata: ReceivableMetadata,
}

impl Default for DownstreamMetadata {
    fn default() -> Self {
        Self {
            request_id: 0.into(),
            stream_id_alias: 0,
            source_node_id: "".to_string(),
            metadata: ReceivableMetadata::BaseTime(BaseTime::default()),
        }
    }
}

/// ダウンストリームメタデータに対する応答です。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct DownstreamMetadataAck {
    /// リクエストID
    pub request_id: super::RequestId,
    /// 結果コード
    pub result_code: super::ResultCode,
    /// 結果文字列
    pub result_string: String,
}
