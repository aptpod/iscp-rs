use std::borrow::Cow;

use thiserror::Error;

/// iSCPエラー型
#[non_exhaustive]
#[derive(Error, Debug)]
pub enum Error {
    /// トランスポートレベルのエラー
    ///
    /// 下位層のネットワーク通信で発生したエラーです。
    /// WebSocketの切断、ネットワーク接続の問題、TLSエラーなどが含まれます。
    #[error("{0}")]
    Transport(#[from] crate::transport::TransportError),

    /// トークンソースのエラー
    ///
    /// アクセストークンの取得中に発生したエラーです。
    /// 認証の失敗、トークンの期限切れなどが含まれます。
    #[error("{0}")]
    TokenSource(#[from] crate::token_source::TokenSourceError),

    /// 成功以外の結果コードを受信
    ///
    /// サーバーからエラー結果コードを含むレスポンスを受信しました。
    /// これは通常、サーバー側で処理が拒否されたことを意味します。
    #[error("result code is: {result_code:?}, detail: {detail:?})")]
    FailedMessage {
        /// 受信した結果コード
        result_code: crate::message::ResultCode,
        /// エラーの詳細メッセージ
        detail: String,
    },

    /// リクエストがタイムアウトしました
    ///
    /// 指定された時間内にサーバーから応答がなかったか、
    /// または操作が完了しませんでした。
    #[error("timeout {0}")]
    Timeout(Cow<'static, str>),

    /// 接続が既に閉じられています
    ///
    /// 接続が既に終了している状態で操作を試みた場合に発生します。
    #[error("connection closed")]
    ConnectionClosed,

    /// 閉じられたことによりキャンセルされた操作
    ///
    /// 進行中の操作が、接続が閉じられたためにキャンセルされました。
    #[error("cancelled by close")]
    CancelledByClose,

    /// ストリームが既に閉じられています
    ///
    /// アップストリームやダウンストリームが既に終了している状態で
    /// 操作を試みた場合に発生します。
    #[error("stream closed")]
    StreamClosed,

    /// 予期せぬエラー
    ///
    /// ライブラリ内部で発生した予期しないエラーです。
    /// バグの可能性があります。
    #[error("unexpected: {0}")]
    Unexpected(Cow<'static, str>),

    /// 無効な値
    ///
    /// 引数やパラメータに無効な値が指定された場合に発生します。
    #[error("invalid value `{0}`")]
    InvalidValue(Cow<'static, str>),

    /// 並べ替えエラー
    ///
    /// ダウンストリームでのチャンク並べ替え中に発生したエラーです。
    #[error("cannot wait chunk in reordering, the upstream id is `{0}`")]
    Reordering(uuid::Uuid),
}

impl Error {
    pub(crate) fn can_retry(&self) -> bool {
        matches!(
            self,
            Error::Transport(..) | Error::ConnectionClosed | Error::CancelledByClose
        )
    }

    pub(crate) fn result_code(&self) -> Option<crate::message::ResultCode> {
        match self {
            Error::FailedMessage { result_code, .. } => Some(*result_code),
            _ => None,
        }
    }

    pub(crate) fn timeout<T: Into<Cow<'static, str>>>(msg: T) -> Self {
        Self::Timeout(msg.into())
    }

    pub(crate) fn unexpected<T: Into<Cow<'static, str>>>(msg: T) -> Self {
        Self::Unexpected(msg.into())
    }

    pub(crate) fn invalid_value<T: Into<Cow<'static, str>>>(msg: T) -> Self {
        Self::InvalidValue(msg.into())
    }
}
