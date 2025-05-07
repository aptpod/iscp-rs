//! Quicトランスポートの実装

use std::{
    collections::VecDeque,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use byteorder::{BigEndian, ByteOrder};
use bytes::{Bytes, BytesMut};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{
    datagram::{FrameBuf, Splitter},
    Certificate, Connector, NegotiationParams, Transport, TransportCloser, TransportError,
    TransportReader, TransportWriter,
};

pub type QuicTransport = Transport<QuicConnector>;

#[derive(Clone, Debug)]
pub struct QuicConnector {
    host: String,
    port: u16,
    addr: Option<IpAddr>,
    bind_addr: Option<SocketAddr>,
    max_datagram_size: Option<usize>,
    datagram_send_buffer_size: Option<usize>,
    timeout: Duration,
    congestion_config: CongestionConfig,
    skip_server_verification: bool,
    client_auth_cert_and_key: Option<(Certificate, Certificate)>,
}

impl QuicConnector {
    pub fn new(host: &str, port: u16) -> Self {
        QuicConnector {
            host: host.to_owned(),
            port,
            addr: None,
            bind_addr: None,
            max_datagram_size: None,
            datagram_send_buffer_size: None,
            timeout: Duration::from_secs(10),
            congestion_config: CongestionConfig::Default,
            skip_server_verification: false,
            client_auth_cert_and_key: None,
        }
    }

    pub fn addr<T: Into<Option<IpAddr>>>(mut self, addr: T) -> Self {
        self.addr = addr.into();
        self
    }

    pub fn bind_addr<T: Into<Option<SocketAddr>>>(mut self, bind_addr: T) -> Self {
        self.bind_addr = bind_addr.into();
        self
    }

    pub fn max_datagram_size<T: Into<Option<usize>>>(mut self, max_datagram_size: T) -> Self {
        self.max_datagram_size = max_datagram_size.into();
        self
    }

    pub fn datagram_send_buffer_size<T: Into<Option<usize>>>(
        mut self,
        datagram_send_buffer_size: T,
    ) -> Self {
        self.datagram_send_buffer_size = datagram_send_buffer_size.into();
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn congestion_config(mut self, congestion_config: CongestionConfig) -> Self {
        self.congestion_config = congestion_config;
        self
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

impl Connector for QuicConnector {
    type Reader = QuicReader;
    type Writer = QuicWriter;
    type Closer = QuicCloser;
    type UnreliableReader = QuicUnreliableReader;
    type UnreliableWriter = QuicUnreliableWriter;

    async fn connect(
        &self,
        negotiation_params: NegotiationParams,
    ) -> Result<Transport<Self>, TransportError> {
        // Get address
        let addr = if let Some(addr) = self.addr {
            SocketAddr::new(addr, self.port)
        } else {
            tokio::net::lookup_host(format!("{}:{}", self.host, self.port))
                .await
                .map_err(TransportError::new)?
                .next()
                .ok_or_else(|| TransportError::from_msg("lookup host failed: no address found"))?
        };

        let bind_addr = if let Some(bind_addr) = self.bind_addr {
            bind_addr
        } else {
            SocketAddr::new(
                if addr.is_ipv6() {
                    IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED)
                } else {
                    IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)
                },
                0,
            )
        };

        // Get connection
        let client_config = configure_client(self)?;

        let mut endpoint = quinn::Endpoint::client(bind_addr).map_err(TransportError::new)?;
        endpoint.set_default_client_config(client_config);

        let connection = endpoint
            .connect(addr, &self.host)
            .map_err(TransportError::new)?
            .await
            .map_err(TransportError::new)?;
        log::info!("quic connector connected to {}", addr);

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

        // Negotiation
        {
            log::debug!("quic negotiation");
            let mut stream = connection.open_uni().await.map_err(TransportError::new)?;
            let negotiation_bytes = negotiation_params.to_bytes()?;
            stream
                .write_all(&negotiation_bytes)
                .await
                .map_err(TransportError::new)?;
            stream.finish().map_err(TransportError::new)?;
        }

        // Open send stream
        let send_stream = connection.open_uni().await.map_err(TransportError::new)?;

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

        let reader = QuicReader { rx_recv_bytes, ct };
        let writer = QuicWriter {
            send_stream,
            buf: BytesMut::new(),
        };
        let closer = QuicCloser {
            endpoint,
            connection: connection.clone(),
        };
        let unreliable_reader = QuicUnreliableReader {
            connection: connection.clone(),
            frame_buf: FrameBuf::new(),
        };
        let unreliable_writer = QuicUnreliableWriter {
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

pub struct QuicReader {
    rx_recv_bytes: mpsc::Receiver<Vec<u8>>,
    ct: CancellationToken,
}

async fn read_recv_stream(
    mut recv_stream: quinn::RecvStream,
    tx: mpsc::Sender<Vec<u8>>,
    ct: CancellationToken,
) {
    loop {
        let mut len = [0; 4];
        if let Err(e) = cancelled_return!(ct, recv_stream.read_exact(&mut len)) {
            log::warn!("cannot read from quic recv stream: {}", e);
            return;
        }
        let len = BigEndian::read_u32(&len) as usize;

        let mut buf = vec![0; len];
        if let Err(e) = cancelled_return!(ct, recv_stream.read_exact(&mut buf[..])) {
            log::warn!("cannot read from quic recv stream: {}", e);
            return;
        }
        if tx.send(buf).await.is_err() {
            return;
        }
    }
}

impl TransportReader for QuicReader {
    async fn read(&mut self, buf: &mut bytes::BytesMut) -> Result<(), TransportError> {
        if let Some(bytes) = self.rx_recv_bytes.recv().await {
            buf.extend_from_slice(&bytes);
            Ok(())
        } else {
            Err(TransportError::from_msg("quic stream closed"))
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.ct.cancel();
        Ok(())
    }
}

pub struct QuicWriter {
    send_stream: quinn::SendStream,
    buf: BytesMut,
}

impl TransportWriter for QuicWriter {
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

pub struct QuicCloser {
    endpoint: quinn::Endpoint,
    connection: quinn::Connection,
}

impl TransportCloser for QuicCloser {
    async fn close(&mut self) -> Result<(), TransportError> {
        self.connection.close(0u32.into(), b"done");
        self.endpoint.wait_idle().await;
        Ok(())
    }
}

pub struct QuicUnreliableReader {
    connection: quinn::Connection,
    frame_buf: FrameBuf,
}

impl TransportReader for QuicUnreliableReader {
    async fn read(&mut self, buf: &mut bytes::BytesMut) -> Result<(), TransportError> {
        loop {
            let frame = match self.connection.read_datagram().await {
                Ok(frame) => frame,
                Err(e) => {
                    log::warn!("cannot read datagram: {}", e);
                    return Err(TransportError::new(e));
                }
            };
            if let Some(data) = self.frame_buf.push(frame) {
                buf.extend_from_slice(&data[..]);
                return Ok(());
            }
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

pub struct QuicUnreliableWriter {
    connection: quinn::Connection,
    splitter: Splitter,
    frames: VecDeque<Bytes>,
}

impl TransportWriter for QuicUnreliableWriter {
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

fn configure_client(c: &QuicConnector) -> Result<quinn::ClientConfig, TransportError> {
    pub const PROTOCOL: &[&[u8]] = &[b"iscp"];

    let mut rustls_config = if let Some(rustls_config) = super::tls::rustls_config_client(
        c.skip_server_verification,
        c.client_auth_cert_and_key.clone(),
    )? {
        rustls_config
    } else {
        // Use system native root certs by default
        let mut roots = rustls::RootCertStore::empty();
        let certs = super::tls::load_native_certs().map_err(TransportError::new)?;
        for cert in certs.into_iter() {
            roots.add(cert).map_err(TransportError::new)?;
        }
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth()
    };

    rustls_config.alpn_protocols = PROTOCOL.iter().map(|&x| x.into()).collect();

    let mut tr_config = quinn::TransportConfig::default();

    if let Some(value) = c.datagram_send_buffer_size {
        tr_config.datagram_send_buffer_size(value);
    }
    tr_config.max_idle_timeout(Some(
        quinn::IdleTimeout::try_from(c.timeout).map_err(TransportError::new)?,
    ));
    tr_config.keep_alive_interval(None);
    c.congestion_config.set_factory(&mut tr_config);

    let quic_client_config = quinn::crypto::rustls::QuicClientConfig::try_from(rustls_config)
        .map_err(TransportError::new)?;
    let mut config = quinn::ClientConfig::new(Arc::new(quic_client_config));

    config.transport_config(Arc::new(tr_config));

    Ok(config)
}

#[derive(Clone, Default, Debug)]
pub enum CongestionConfig {
    #[default]
    Default,
    Bbr {
        initial_window: Option<u64>,
    },
    Cubic {
        initial_window: Option<u64>,
    },
    NewReno {
        initial_window: Option<u64>,
        loss_reduction_factor: Option<f32>,
    },
}

impl CongestionConfig {
    fn set_factory(&self, tr_config: &mut quinn::TransportConfig) {
        match self {
            Self::Default => (),
            Self::Bbr { initial_window } => {
                let mut bbr_config = quinn::congestion::BbrConfig::default();
                if let Some(initial_window) = *initial_window {
                    bbr_config.initial_window(initial_window);
                }
                tr_config.congestion_controller_factory(Arc::new(bbr_config));
            }
            Self::Cubic { initial_window } => {
                let mut cubic_config = quinn::congestion::CubicConfig::default();
                if let Some(initial_window) = *initial_window {
                    cubic_config.initial_window(initial_window);
                }
                tr_config.congestion_controller_factory(Arc::new(cubic_config));
            }
            Self::NewReno {
                initial_window,
                loss_reduction_factor,
            } => {
                let mut new_reno_config = quinn::congestion::NewRenoConfig::default();
                if let Some(initial_window) = *initial_window {
                    new_reno_config.initial_window(initial_window);
                }
                if let Some(loss_reduction_factor) = *loss_reduction_factor {
                    new_reno_config.loss_reduction_factor(loss_reduction_factor);
                }
                tr_config.congestion_controller_factory(Arc::new(new_reno_config));
            }
        }
    }
}
