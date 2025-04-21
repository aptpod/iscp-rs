use std::{collections::VecDeque, sync::Arc};

use byteorder::{BigEndian, ByteOrder};
use bytes::{Bytes, BytesMut};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use url::Url;

use super::{
    datagram::{FrameBuf, Splitter},
    Certificate, Connector, NegotiationParams, Transport, TransportCloser, TransportError,
    TransportReader, TransportWriter,
};

pub type WebTransportTransport = Transport<WebTransportCloser>;

#[derive(Clone, Debug)]
pub struct WebTransportConnector {
    url: String,
    skip_server_verification: bool,
    client_auth_cert_and_key: Option<(Certificate, Certificate)>,
    max_datagram_size: Option<usize>,
}

impl WebTransportConnector {
    pub fn new<S: ToString>(url: S) -> Self {
        Self {
            url: url.to_string(),
            skip_server_verification: false,
            client_auth_cert_and_key: None,
            max_datagram_size: None,
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

    pub fn max_datagram_size<T: Into<Option<usize>>>(mut self, max_datagram_size: T) -> Self {
        self.max_datagram_size = max_datagram_size.into();
        self
    }
}

impl Connector for WebTransportConnector {
    type Reader = WebTransportReader;
    type Writer = WebTransportWriter;
    type Closer = WebTransportCloser;
    type UnreliableReader = WebTransportUnreliableReader;
    type UnreliableWriter = WebTransportUnreliableWriter;

    async fn connect(
        &self,
        negotiation_params: NegotiationParams,
    ) -> Result<Transport<Self>, TransportError> {
        let mut url: Url = self.url.parse().map_err(TransportError::new)?;
        url.set_query(Some(&negotiation_params.to_uri_query_string()?));

        let client_config = if self.skip_server_verification {
            wtransport::ClientConfig::builder()
                .with_bind_default()
                .with_no_cert_validation()
                .build()
        } else if self.client_auth_cert_and_key.is_some() {
            let mut rustls_config =
                super::tls::rustls_config_client(false, self.client_auth_cert_and_key.clone())?
                    .unwrap();
            rustls_config.alpn_protocols = vec![b"h3".to_vec()];
            wtransport::ClientConfig::builder()
                .with_bind_default()
                .with_custom_tls(rustls_config)
                .build()
        } else {
            wtransport::ClientConfig::default()
        };

        let endpoint = wtransport::Endpoint::client(client_config).map_err(TransportError::new)?;

        let opts = wtransport::endpoint::ConnectOptions::builder(url).build();

        let connection = Arc::new(endpoint.connect(opts).await.map_err(TransportError::new)?);

        log::info!("webtransport connector connected to {}", self.url);

        let Some(max_datagram_size) = connection.max_datagram_size() else {
            return Err(TransportError::from_msg("datagram disabled"));
        };
        if max_datagram_size < 9 {
            return Err(TransportError::from_msg("too small max datagram size"));
        }
        let max_datagram_size = if let Some(mtu) = self.max_datagram_size {
            std::cmp::min(mtu, max_datagram_size)
        } else {
            max_datagram_size
        };
        log::info!("max datagram size: {} bytes", max_datagram_size);

        // Open send stream
        let send_stream = connection
            .open_uni()
            .await
            .map_err(TransportError::new)?
            .await
            .map_err(TransportError::new)?;

        // Accept recv stream
        let (tx_recv_bytes, rx_recv_bytes) = mpsc::channel(1024);
        let conn_read = connection.clone();
        let ct = CancellationToken::new();
        let ct_clone = ct.clone();
        tokio::spawn(async move {
            loop {
                let result = tokio::select! {
                    _ = ct_clone.cancelled() => { break; }
                    result = conn_read.accept_uni() => result,
                };
                let Ok(recv_stream) = result else {
                    break;
                };
                let tx = tx_recv_bytes.clone();
                let ct = ct_clone.clone();
                tokio::spawn(async move {
                    read_recv_stream(recv_stream, tx, ct).await;
                });
            }
            log::debug!("exit stream accept loop");
        });

        let reader = WebTransportReader { rx_recv_bytes, ct };
        let writer = WebTransportWriter {
            send_stream,
            buf: BytesMut::new(),
        };
        let closer = WebTransportCloser {
            endpoint,
            connection: connection.clone(),
        };
        let unreliable_reader = WebTransportUnreliableReader {
            connection: connection.clone(),
            frame_buf: FrameBuf::new(),
        };
        let unreliable_writer = WebTransportUnreliableWriter {
            connection,
            splitter: Splitter::new(max_datagram_size),
            frames: VecDeque::new(),
        };

        Ok(Transport {
            reader,
            writer,
            closer,
            unreliable_reader: Some(unreliable_reader),
            unreliable_writer: Some(unreliable_writer),
        })
    }
}

pub struct WebTransportReader {
    rx_recv_bytes: mpsc::Receiver<Vec<u8>>,
    ct: CancellationToken,
}

async fn read_recv_stream(
    mut recv_stream: wtransport::RecvStream,
    tx: mpsc::Sender<Vec<u8>>,
    ct: CancellationToken,
) {
    loop {
        let mut len = [0; 4];
        if let Err(e) = cancelled_return!(ct, recv_stream.read_exact(&mut len)) {
            log::warn!("cannot read from webtransport recv stream: {}", e);
            return;
        }
        let len = BigEndian::read_u32(&len) as usize;

        let mut buf = vec![0; len];
        if let Err(e) = cancelled_return!(ct, recv_stream.read_exact(&mut buf[..])) {
            log::warn!("cannot read from webtransport recv stream: {}", e);
            return;
        }
        if tx.send(buf).await.is_err() {
            return;
        }
    }
}

impl TransportReader for WebTransportReader {
    async fn read(&mut self, buf: &mut bytes::BytesMut) -> Result<(), TransportError> {
        if let Some(bytes) = self.rx_recv_bytes.recv().await {
            buf.extend_from_slice(&bytes);
            Ok(())
        } else {
            Err(TransportError::from_msg("webtransport stream closed"))
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.ct.cancel();
        Ok(())
    }
}

pub struct WebTransportWriter {
    send_stream: wtransport::SendStream,
    buf: BytesMut,
}

impl TransportWriter for WebTransportWriter {
    async fn write(&mut self, data: &[u8]) -> Result<(), TransportError> {
        let len = data.len();
        self.buf.resize(4, 0);
        let len: u32 = len
            .try_into()
            .map_err(|_| TransportError::from_msg("too large message size"))?;
        BigEndian::write_u32(&mut self.buf, len);
        self.buf.extend_from_slice(data);
        self.send_stream
            .write_all(&self.buf)
            .await
            .map_err(TransportError::new)?;
        Ok(())
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

pub struct WebTransportCloser {
    endpoint: wtransport::Endpoint<wtransport::endpoint::endpoint_side::Client>,
    connection: Arc<wtransport::Connection>,
}

impl TransportCloser for WebTransportCloser {
    async fn close(&mut self) -> Result<(), TransportError> {
        self.connection.close(0u32.into(), b"done");
        self.endpoint.wait_idle().await;
        Ok(())
    }
}

pub struct WebTransportUnreliableReader {
    connection: Arc<wtransport::Connection>,
    frame_buf: FrameBuf,
}

impl TransportReader for WebTransportUnreliableReader {
    async fn read(&mut self, buf: &mut bytes::BytesMut) -> Result<(), TransportError> {
        loop {
            let frame = match self.connection.receive_datagram().await {
                Ok(frame) => frame,
                Err(e) => {
                    log::warn!("cannot read datagram: {}", e);
                    return Err(TransportError::new(e));
                }
            };
            if let Some(data) = self.frame_buf.push(frame.payload()) {
                buf.extend_from_slice(&data[..]);
                return Ok(());
            }
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

pub struct WebTransportUnreliableWriter {
    connection: Arc<wtransport::Connection>,
    splitter: Splitter,
    frames: VecDeque<Bytes>,
}

impl TransportWriter for WebTransportUnreliableWriter {
    async fn write(&mut self, data: &[u8]) -> Result<(), TransportError> {
        if self.splitter.split(data, &mut self.frames).is_err() {
            return Err(TransportError::from_msg(
                "too large message size for datagram",
            ));
        }
        while let Some(frame) = self.frames.pop_front() {
            self.connection
                .send_datagram(frame)
                .map_err(TransportError::new)?;
        }
        Ok(())
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}
