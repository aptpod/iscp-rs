//! iSCPの認証トークンを定義するモジュールです。

use crate::message::AccessToken;
use crate::Result;
use async_trait::async_trait;

/// 認証トークンを取得するためのインターフェースです。
#[async_trait]
pub trait TokenSource: Sync + Send {
    /// トークンを取得します。
    ///
    ///  iSCPコネクションを開く度に、このメソッドをコールしトークンを取得します。
    async fn token(&self) -> Result<String>;
}

/// 静的に認証トークンを指定するTokenSourceです。
///
/// APIトークンなどを使用するための実装です。クライアントシークレットには使用できません。
#[derive(Clone, Debug, Default)]
pub struct StaticTokenSource {
    token: AccessToken,
}

impl StaticTokenSource {
    /// StaticTokenSource を生成します。
    pub fn new<T: ToString>(token: T) -> Self {
        Self {
            token: AccessToken::new(token),
        }
    }
}

#[async_trait]
impl TokenSource for StaticTokenSource {
    /// トークンを取得します。
    ///
    /// 常に同じトークンを返却します。
    async fn token(&self) -> Result<String> {
        Ok(self.token.clone().into())
    }
}

#[async_trait]
impl<T: TokenSource + Sync + Send + Clone> TokenSource for Box<T> {
    async fn token(&self) -> Result<String> {
        (**self).token().await
    }
}
