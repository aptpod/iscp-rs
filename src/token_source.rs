use tokio::sync::{mpsc, oneshot};

/// An access token for iSCP connection.
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

/// Source of iSCP access token.
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
                let is_err = result.is_err();
                if tx.send(result).is_err() {
                    break;
                }
                if is_err {
                    break;
                }
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

/// Static token source.
#[derive(Clone, Debug, Default)]
pub struct StaticTokenSource(AccessToken);

impl StaticTokenSource {
    /// Create StaticTokenSource from a string.
    pub fn new<T: std::fmt::Display>(token: T) -> Self {
        Self(AccessToken::new(token))
    }
}

impl TokenSource for StaticTokenSource {
    async fn token(&mut self) -> Result<AccessToken, TokenSourceError> {
        Ok(self.0.clone())
    }
}

/// Represent an error occured at token source.
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
