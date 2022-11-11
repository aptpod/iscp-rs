use std::convert::From;

/// ノードとブローカーの間で疎通確認のために交換されるメッセージです。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct Ping {
    /// リクエストID
    pub request_id: super::RequestId,
}

/// Pingに対する応答です。
#[derive(Clone, PartialEq, Default, Debug)]
pub struct Pong {
    /// リクエストID
    pub request_id: super::RequestId,
}

impl From<Ping> for Pong {
    fn from(p: Ping) -> Self {
        Self {
            request_id: p.request_id,
        }
    }
}
