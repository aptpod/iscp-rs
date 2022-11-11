use std::collections::HashMap;

use uuid::Uuid;

use super::{DataId, DataIdAliasMap};

pub use crate::encoding::internal::autogen::data_point_group::DataIdOrAlias;

impl From<DataId> for DataIdOrAlias {
    fn from(id: DataId) -> Self {
        Self::DataId(id)
    }
}

impl From<u32> for DataIdOrAlias {
    fn from(alias: u32) -> Self {
        Self::DataIdAlias(alias)
    }
}

/// アップストリーム情報、またはアップストリームエイリアスです。
#[derive(Clone, PartialEq, Debug)]
pub enum UpstreamOrAlias {
    UpstreamInfo(UpstreamInfo),
    Alias(u32),
}

impl Default for UpstreamOrAlias {
    fn default() -> Self {
        Self::Alias(0)
    }
}

impl From<UpstreamInfo> for UpstreamOrAlias {
    fn from(info: UpstreamInfo) -> Self {
        Self::UpstreamInfo(info)
    }
}

impl From<u32> for UpstreamOrAlias {
    fn from(alias: u32) -> Self {
        Self::Alias(alias)
    }
}

/// ストリームチャンク（上り用）で送信されたデータポイントの処理結果です。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct UpstreamChunkResult {
    /// シーケンス番号
    pub sequence_number: u32,
    /// 結果コード
    pub result_code: super::ResultCode,
    /// 結果文字列
    pub result_string: String,
}

/// ストリームチャンク（下り用）で送信されたデータポイントの処理結果です。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct DownstreamChunkResult {
    /// アップストリームのストリームID
    pub stream_id_of_upstream: Uuid,
    /// アップストリームにおけるシーケンス番号
    pub sequence_number_in_upstream: u32,
    /// 結果コード
    pub result_code: super::ResultCode,
    /// 結果文字列
    pub result_string: String,
}

/// ストリームを時間で区切ったデータポイントのまとまりです。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct StreamChunk {
    pub sequence_number: u32,
    pub data_point_groups: Vec<DataPointGroup>,
}

pub use crate::encoding::internal::autogen::DataPoint;

/// ストリームチャンクの中のデータポイントをデータIDごとにまとめた集合です。
#[derive(Clone, PartialEq, Debug)]
pub struct DataPointGroup {
    /// データポイント
    pub data_points: Vec<DataPoint>,
    /// データID または データIDエイリアス
    pub data_id_or_alias: DataIdOrAlias,
}

/// アップストリーム情報です。
#[derive(Clone, Eq, PartialEq, Hash, Default, Debug)]
pub struct UpstreamInfo {
    pub source_node_id: String,
    pub session_id: String,
    pub stream_id: Uuid,
}

/// ストリームチャンク（下り用）です。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct DownstreamChunk {
    /// ストリームIDエイリアス
    pub stream_id_alias: u32,
    /// ストリームチャンク
    pub stream_chunk: StreamChunk,
    /// アップストリーム情報、またはアップストリームエイリアス
    pub upstream_or_alias: UpstreamOrAlias,
}

/// ストリームチャンク（下り用）に対する確認応答です。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct DownstreamChunkAck {
    /// ACK ID
    pub ack_id: u32,
    /// ストリームIDエイリアス
    pub stream_id_alias: u32,
    /// アップストリームエイリアス
    pub upstream_aliases: HashMap<u32, UpstreamInfo>,
    /// データIDエイリアス
    pub data_id_aliases: DataIdAliasMap,
    /// 処理結果
    pub results: Vec<DownstreamChunkResult>,
}

/// ストリームチャンク（下り用）に対する確認応答に対する応答です。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct DownstreamChunkAckComplete {
    /// ストリームIDエイリアス
    pub stream_id_alias: u32,
    /// ACK ID
    pub ack_id: u32,
    /// 結果コード
    pub result_code: super::ResultCode,
    /// 結果文字列
    pub result_string: String,
}

/// ストリームチャンク（上り用）です。
#[derive(Clone, PartialEq, Debug, Default)]
pub struct UpstreamChunk {
    /// ストリームIDエイリアス
    pub stream_id_alias: u32,
    /// ストリームチャンク
    pub stream_chunk: StreamChunk,
    /// データID
    pub data_ids: Vec<DataId>,
}

/// ストリームチャンク（上り用）に対する確認応答です。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct UpstreamChunkAck {
    /// ストリームIDエイリアス
    pub stream_id_alias: u32,
    /// 処理結果
    pub results: Vec<UpstreamChunkResult>,
    /// データIDエイリアス
    pub data_id_aliases: DataIdAliasMap,
}
