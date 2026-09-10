use tokio::sync::{mpsc, oneshot};

/// iSCP接続のアクセストークン
#[derive(Clone, PartialEq, Eq, Default)]
pub struct AccessToken(pub(crate) String);
impl std::fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccessToken")
            .field("token", &"***")
            .finish()
    }
}

impl AccessToken {
    pub fn new<T: std::fmt::Display>(token: T) -> Self {
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

/// iSCPアクセストークンのソース
pub trait TokenSource: Send + Sync + 'static {
    fn token(
        &mut self,
    ) -> impl std::future::Future<Output = Result<AccessToken, TokenSourceError>> + Send;
}

#[derive(Clone)]
pub(crate) struct SharedTokenSource {
    tx_command: mpsc::Sender<Command>,
}

#[derive(Debug)]
struct Command(oneshot::Sender<Result<AccessToken, TokenSourceError>>);

impl SharedTokenSource {
    pub fn new<T: TokenSource>(mut token_source: T) -> Self {
        let (tx_command, mut rx_command) = mpsc::channel(1);
        tokio::spawn(async move {
            while let Some(command) = rx_command.recv().await {
                let Command(tx) = command;
                let result = token_source.token().await;
                let _ = tx.send(result);
            }
        });

        Self { tx_command }
    }

    pub async fn token(&self) -> Result<AccessToken, TokenSourceError> {
        let (tx, rx) = oneshot::channel();
        if self.tx_command.send(Command(tx)).await.is_err() {
            return Err(TokenSourceError::from_msg("token source closed"));
        }
        rx.await
            .map_err(|_| TokenSourceError::from_msg("token source closed"))?
    }
}

/// 静的トークンソース
#[derive(Clone, Debug, Default)]
pub struct StaticTokenSource(AccessToken);

impl StaticTokenSource {
    /// 文字列から静的トークンソース作成
    pub fn new<T: std::fmt::Display>(token: T) -> Self {
        Self(AccessToken::new(token))
    }
}

impl TokenSource for StaticTokenSource {
    async fn token(&mut self) -> Result<AccessToken, TokenSourceError> {
        Ok(self.0.clone())
    }
}

/// トークンソースのエラー
pub struct TokenSourceError {
    inner: Box<dyn std::error::Error + Send + Sync>,
}

impl std::fmt::Debug for TokenSourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("TokenSourceError")
            .field(&self.inner)
            .finish()
    }
}

impl std::fmt::Display for TokenSourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "token source error: {}", self.inner)
    }
}

impl std::error::Error for TokenSourceError {}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
struct TokenSourceErrorMessage(String);

impl TokenSourceError {
    pub fn new<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self {
            inner: Box::new(err),
        }
    }

    pub fn from_msg<T: std::fmt::Display>(msg: T) -> Self {
        Self {
            inner: Box::new(TokenSourceErrorMessage(msg.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    /// Fails the first `fail_first` calls, then returns a token tagged with the call count.
    struct FlakyTokenSource {
        calls: Arc<AtomicUsize>,
        fail_first: usize,
    }

    impl TokenSource for FlakyTokenSource {
        async fn token(&mut self) -> Result<AccessToken, TokenSourceError> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            if n < self.fail_first {
                Err(TokenSourceError::from_msg("temporary failure"))
            } else {
                Ok(AccessToken::new(format!("token-{n}")))
            }
        }
    }

    #[tokio::test]
    async fn token_error_does_not_kill_shared_task() {
        let calls = Arc::new(AtomicUsize::new(0));
        let shared = SharedTokenSource::new(FlakyTokenSource {
            calls: calls.clone(),
            fail_first: 1,
        });

        let first = shared.token().await;
        assert!(first.is_err(), "first token() should propagate the error");

        let second = shared.token().await;
        let token = second.expect("second token() should succeed after a transient error");
        assert_eq!(token, AccessToken::new("token-1"));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn send_failure_does_not_kill_shared_task() {
        let calls = Arc::new(AtomicUsize::new(0));
        let shared = SharedTokenSource::new(FlakyTokenSource {
            calls: calls.clone(),
            fail_first: 0,
        });

        // Drop the receiver before the task replies, forcing its oneshot send to fail.
        let (tx, rx) = oneshot::channel();
        shared
            .tx_command
            .send(Command(tx))
            .await
            .expect("command channel should be open");
        drop(rx);

        let token = shared
            .token()
            .await
            .expect("token() should succeed even after a oneshot send failure");
        assert!(calls.load(Ordering::SeqCst) >= 1);
        assert!(token.0.starts_with("token-"));
    }
}
