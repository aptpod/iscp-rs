/// アクセストークンです。
#[derive(Clone, PartialEq, Eq, Default)]
pub struct AccessToken(String);
impl std::fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccessToken")
            .field("token", &"****")
            .finish()
    }
}

impl AccessToken {
    pub fn new<T: ToString>(token: T) -> Self {
        Self(token.to_string())
    }
}

impl From<String> for AccessToken {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<AccessToken> for String {
    fn from(t: AccessToken) -> Self {
        t.0
    }
}

/// 接続要求です。
#[derive(Clone, PartialEq, Debug)]
pub struct ConnectRequest {
    /// リクエストID
    pub request_id: super::RequestId,
    /// プロトコルバージョン
    pub protocol_version: String,
    /// ノードID
    pub node_id: String,
    /// Ping間隔
    pub ping_interval: chrono::Duration, // sec
    /// Pingタイムアウト
    pub ping_timeout: chrono::Duration, // sec
    /// プロジェクトUUID
    pub project_uuid: Option<String>,
    /// アクセストークン
    pub access_token: Option<AccessToken>,
}

impl Default for ConnectRequest {
    fn default() -> Self {
        Self {
            request_id: 0.into(),
            protocol_version: crate::ISCP_VERSION.to_string(),
            node_id: "".to_string(),
            ping_interval: chrono::Duration::seconds(10),
            ping_timeout: chrono::Duration::seconds(1),
            project_uuid: None,
            access_token: None,
        }
    }
}

/// 接続要求に対する応答です。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct ConnectResponse {
    /// リクエストID
    pub request_id: super::RequestId,
    /// プロトコルバージョン
    pub protocol_version: String,
    /// 結果コード
    pub result_code: super::ResultCode,
    /// 結果文字列
    pub result_string: String,
}

/// 接続の切断です。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct Disconnect {
    /// 結果コード
    pub result_code: super::ResultCode,
    /// 結果文字列
    pub result_string: String,
}
