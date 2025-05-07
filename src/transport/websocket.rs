//! WebSocketトランスポートの実装

use bytes::BytesMut;
use reqwest_websocket::{Message, RequestBuilderExt};
use tokio::sync::mpsc;
use url::Url;

use super::{
    Certificate, Connector, NegotiationParams, Transport, TransportCloser, TransportError,
    TransportReader, TransportWriter, UnreliableNotSupported,
};
use futures::{stream::SplitStream, SinkExt, StreamExt};

pub type WebSocketTransport = Transport<WebSocketConnector>;

#[derive(Clone, Debug)]
pub struct WebSocketConnector {
    url: String,
    skip_server_verification: bool,
    client_auth_cert_and_key: Option<(Certificate, Certificate)>,
}

impl WebSocketConnector {
    pub fn new<S: ToString>(url: S) -> Self {
        Self {
            url: url.to_string(),
            skip_server_verification: false,
            client_auth_cert_and_key: None,
        }
    }

    pub fn skip_server_verification(mut self, skip_server_verification: bool) -> Self {
        self.skip_server_verification = skip_server_verification;
        self
    }

    pub fn client_auth_cert_and_key<T: Into<Option<(Certificate, Certificate)>>>(
        mut self,
        client_auth_cert_and_key: T,
    ) -> Self {
        self.client_auth_cert_and_key = client_auth_cert_and_key.into();
        self
    }
}

impl Connector for WebSocketConnector {
    type Reader = WebSocketReader;
    type Writer = WebSocketWriter;
    type Closer = WebSocketCloser;
    type UnreliableReader = UnreliableNotSupported;
    type UnreliableWriter = UnreliableNotSupported;

    async fn connect(
        &self,
        negotiation_params: NegotiationParams,
    ) -> Result<Transport<Self>, TransportError> {
        let url: Url = self.url.parse().map_err(TransportError::new)?;

        // Connect
        let mut url = url.clone();
        url.set_query(Some(&negotiation_params.to_uri_query_string()?));

        log::debug!("websocket request to {}", url);

        let rustls_config = super::tls::rustls_config_client(
            self.skip_server_verification,
            self.client_auth_cert_and_key.clone(),
        )?;
        let builder = reqwest::Client::builder();
        let builder = if let Some(rustls_config) = rustls_config {
            builder.use_preconfigured_tls(rustls_config)
        } else {
            builder
        };

        let client = builder.build().map_err(TransportError::new)?;
        let response = client
            .get(url.clone())
            .upgrade()
            .send()
            .await
            .map_err(TransportError::new)?;
        let websocket = response
            .into_websocket()
            .await
            .map_err(TransportError::new)?;

        log::info!("reqwest websocket connector connected to {}", url);

        let (mut sink, stream) = websocket.split();
        let (tx_write_message, mut rx_write_message) = mpsc::channel(16);

        tokio::spawn(async move {
            while let Some(msg) = rx_write_message.recv().await {
                if let Err(e) = sink.send(msg).await {
                    log::debug!("websocket write error {:?}", e);
                    break;
                }
                if let Err(e) = sink.flush().await {
                    log::debug!("websocket flush error {:?}", e);
                    break;
                }
            }
            log::debug!("websocket write loop closed");
        });

        let writer = WebSocketWriter {
            tx_write_message: tx_write_message.clone(),
        };

        let reader = WebSocketReader {
            stream,
            tx_write_message,
        };

        Ok(Transport {
            reader,
            writer,
            closer: WebSocketCloser {},
            unreliable_reader: None,
            unreliable_writer: None,
        })
    }

    fn _compat_with_context_takeover() -> bool {
        true
    }
}

pub struct WebSocketReader {
    stream: SplitStream<reqwest_websocket::WebSocket>,
    tx_write_message: mpsc::Sender<Message>,
}

pub struct WebSocketWriter {
    tx_write_message: mpsc::Sender<Message>,
}

pub struct WebSocketCloser {}

impl TransportReader for WebSocketReader {
    async fn read(&mut self, buf: &mut BytesMut) -> Result<(), TransportError> {
        while let Some(res) = self.stream.next().await {
            let msg = res.map_err(TransportError::new)?;
            match msg {
                Message::Close { code, reason } => {
                    log::debug!(
                        "receive websocket close frame, code = {}, reason = {}",
                        code,
                        reason
                    );
                    break;
                }
                Message::Ping(p) => {
                    if self
                        .tx_write_message
                        .send_timeout(Message::Pong(p), std::time::Duration::from_secs(10))
                        .await
                        .is_err()
                    {
                        return Err(TransportError::from_msg(
                            "websocket write task closed or timeout, cannot send pong",
                        ));
                    }
                    continue;
                }
                Message::Binary(bin) => {
                    buf.extend_from_slice(bin.as_slice());
                    return Ok(());
                }
                _ => continue,
            };
        }
        Err(TransportError::from_msg("web socket stream closed"))
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

impl TransportWriter for WebSocketWriter {
    async fn write(&mut self, data: &[u8]) -> Result<(), TransportError> {
        let msg = Message::Binary(data.to_vec());
        self.tx_write_message
            .send(msg)
            .await
            .map_err(|_| TransportError::from_msg("websocket write task closed"))
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.tx_write_message
            .send(Message::Close {
                code: reqwest_websocket::CloseCode::Normal,
                reason: "OK".into(),
            })
            .await
            .map_err(|_| TransportError::from_msg("websocket write task closed"))
    }
}

impl TransportCloser for WebSocketCloser {
    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}
