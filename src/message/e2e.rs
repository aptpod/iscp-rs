use bytes::Bytes;

/// アップストリームコールです。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct UpstreamCall {
    /// コールID
    pub call_id: String,
    /// リクエストコールID
    pub request_call_id: String,
    /// 宛先ノードID
    pub destination_node_id: String,
    /// 名称
    pub name: String,
    /// 型
    pub type_: String,
    /// ペイロード
    pub payload: Bytes,
}

/// アップストリームコールに対する応答です。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct UpstreamCallAck {
    /// コールID
    pub call_id: String,
    /// 結果コード
    pub result_code: super::ResultCode,
    /// 結果文字列
    pub result_string: String,
}

/// ダウンストリームコールです。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct DownstreamCall {
    /// コールID
    pub call_id: String,
    /// リクエストコールID
    pub request_call_id: String,
    /// 送信元ノードID
    pub source_node_id: String,
    /// 名称
    pub name: String,
    /// 型
    pub type_: String,
    /// ペイロード
    pub payload: Bytes,
}

impl DownstreamCall {
    pub fn is_reply(&self) -> bool {
        !self.request_call_id.as_str().is_empty()
    }
}
