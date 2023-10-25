use super::Conn;
use crate::error::Error;
use crate::error::Result;
use crate::token_source::TokenSource;
use crate::transport::{BoxedTransport, BoxedUnreliableTransport, NegotiationQuery};
use std::sync::Arc;
use tokio_tungstenite::tungstenite::handshake::client::Request;
use tungstenite::protocol::WebSocketConfig;

/// WebSocket用コネクターの設定です。
#[derive(Clone)]
pub struct ConnectorConfig {
    /// パスを指定します。
    pub path: String,
    /// TLSアクセスするかどうかを設定します。
    pub enable_tls: bool,
    /// 接続時の認証に用いるトークンソースです。
    pub token_source: Option<Arc<dyn TokenSource>>,
    /// 送信時のキューサイズです。
    pub send_queue_size: Option<usize>,
}

impl Default for ConnectorConfig {
    fn default() -> Self {
        Self {
            path: "".to_owned(),
            enable_tls: true,
            token_source: None,
            send_queue_size: None,
        }
    }
}

impl std::fmt::Debug for ConnectorConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebSocketConfig")
            .field("path", &self.path)
            .field("enable_tls", &self.enable_tls)
            .finish()
    }
}

/// WebSocket用のコネクターです。
#[derive(Clone, Debug)]
pub struct Connector {
    cfg: ConnectorConfig,
    host: String,
    port: Option<u16>,
    negotiation: NegotiationQuery,
    timeout: std::time::Duration,
    wsconf: WebSocketConfig,
}

impl From<Connector> for super::super::BoxedConnector {
    fn from(c: Connector) -> Self {
        Box::new(c)
    }
}

impl Default for Connector {
    fn default() -> Self {
        Self {
            cfg: ConnectorConfig::default(),
            host: "localhost".to_owned(),
            port: None,
            negotiation: NegotiationQuery::default(),
            timeout: std::time::Duration::from_secs(2),
            wsconf: WebSocketConfig::default(),
        }
    }
}

impl Connector {
    pub fn new(host: String, port: Option<u16>, cfg: ConnectorConfig) -> Self {
        Self {
            cfg,
            host,
            port,
            ..Default::default()
        }
    }

    async fn to_request(&self) -> Result<Request> {
        let schema = match self.cfg.enable_tls {
            true => String::from("wss"),
            _ => String::from("ws"),
        };
        let port = match self.port {
            Some(p) => format!("{}", p),
            None => match self.cfg.enable_tls {
                true => String::from("443"),
                _ => String::from("80"),
            },
        };
        let uri = format!(
            "{}://{}:{}{}?{}",
            schema,
            self.host,
            port,
            self.cfg.path,
            self.negotiation.query_string()?
        );
        let req = Request::builder().uri(uri);

        let req = if let Some(token_source) = &self.cfg.token_source {
            let token = token_source.token().await?;
            req.header("Authorization", format!("Bearer {}", token))
        } else {
            req
        };

        let req = req.body(()).map_err(Error::connect)?;
        Ok(req)
    }
}

#[async_trait::async_trait]
impl crate::transport::Connector for Connector {
    async fn connect(&self) -> Result<(BoxedTransport, Option<BoxedUnreliableTransport>)> {
        log::trace!("websocket connector config: {:?}", self.cfg);

        let conn = Conn::connect(
            self.to_request().await?,
            self.timeout,
            self.wsconf,
            self.cfg.send_queue_size,
        )
        .await?;
        Ok((Box::new(conn), None))
    }
}

#[cfg(test)]
mod tests {

    use http::HeaderMap;

    use super::*;
    use crate::StaticTokenSource;

    #[tokio::test]
    async fn to_request() {
        let req = Connector::new(
            "example.com".to_string(),
            Some(8080),
            ConnectorConfig {
                enable_tls: true,
                path: "/ws".to_string(),
                ..Default::default()
            },
        )
        .to_request()
        .await
        .unwrap();
        assert_eq!(
            req.uri().to_string(),
            "wss://example.com:8080/ws?enc=proto".to_string()
        )
    }

    #[tokio::test]
    async fn to_request_headers() {
        struct Case<'a> {
            token_source: StaticTokenSource,
            want_key: &'a str,
            want_value: &'a str,
        }
        let cases: Vec<Case> = vec![Case {
            token_source: StaticTokenSource::new("bearer"),
            want_key: "Authorization",
            want_value: "Bearer bearer",
        }];
        for case in cases.into_iter() {
            let mut want = HeaderMap::new();
            want.append(case.want_key, case.want_value.parse().unwrap());
            let got = Connector::new(
                "localhost".to_string(),
                None,
                ConnectorConfig {
                    token_source: Some(Arc::new(case.token_source.clone())),
                    ..Default::default()
                },
            )
            .to_request()
            .await
            .unwrap();
            assert_eq!(want, *got.headers());
        }
    }
}
