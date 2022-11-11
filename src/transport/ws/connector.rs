use super::Conn;
use crate::error::Error;
use crate::transport::NegotiationQuery;
use crate::{error::Result, transport::Transport};
use tokio_tungstenite::tungstenite::handshake::client::Request;
use tungstenite::protocol::WebSocketConfig;

// TODO: change to token source
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Token {
    XIntdash(String),
    Bearer(String),
}

/// WebSocket用コネクターの設定です。
#[derive(Clone, Debug)]
pub struct ConnectorConfig {
    /// パスを指定します。
    pub path: String,

    /// TLSアクセスするかどうかを設定します。
    pub enable_tls: bool,

    /// 接続時に認証ヘッダーへ設定するトークンを設定します。
    pub token: Option<Token>,
}

impl Default for ConnectorConfig {
    fn default() -> Self {
        Self {
            path: "".to_owned(),
            enable_tls: false,
            token: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Connector {
    cfg: ConnectorConfig,
    host: String,
    port: Option<u16>,
    send_queue_size: Option<usize>,
    negotiation: NegotiationQuery,
    timeout: std::time::Duration,
    wsconf: WebSocketConfig,
}

impl From<Connector> for super::super::Connector {
    fn from(c: Connector) -> Self {
        Self::Ws(c)
    }
}

impl Default for Connector {
    fn default() -> Self {
        Self {
            cfg: ConnectorConfig::default(),
            host: "localhost".to_owned(),
            port: None,
            send_queue_size: None,
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

    fn to_request(&self) -> Result<Request> {
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

        let req = match &self.cfg.token {
            Some(token) => match token {
                Token::XIntdash(t) => req.header("X-Intdash-Token", t),
                Token::Bearer(t) => req.header("Authorization", format!("Bearer {}", t)),
            },
            _ => req,
        };
        let req = req.body(()).map_err(Error::connect)?;
        Ok(req)
    }

    pub async fn connect(&self) -> Result<impl Transport> {
        let conn = Conn::connect(
            self.to_request()?,
            self.timeout,
            self.wsconf,
            self.send_queue_size,
        )
        .await?;
        Ok(conn)
    }
}

#[cfg(test)]
mod tests {

    use http::HeaderMap;

    use super::*;
    #[test]
    fn to_request() {
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
        .unwrap();
        assert_eq!(
            req.uri().to_string(),
            "wss://example.com:8080/ws?enc=proto".to_string()
        )
    }
    #[test]
    fn to_request_headers() {
        struct Case<'a> {
            token: Token,
            want_key: &'a str,
            want_value: &'a str,
        }
        let cases: Vec<Case> = vec![
            Case {
                token: Token::Bearer("bearer".to_string()),
                want_key: "Authorization",
                want_value: "Bearer bearer",
            },
            Case {
                token: Token::XIntdash("intdash".to_string()),
                want_key: "X-Intdash-Token",
                want_value: "intdash",
            },
        ];

        cases.into_iter().for_each(|c| {
            let mut want = HeaderMap::new();
            want.append(c.want_key, c.want_value.parse().unwrap());
            let got = Connector::new(
                "localhost".to_string(),
                None,
                ConnectorConfig {
                    token: Some(c.token),
                    ..Default::default()
                },
            )
            .to_request()
            .unwrap();
            assert_eq!(want, *got.headers())
        });
    }
}
