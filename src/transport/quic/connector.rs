use std::net::SocketAddr;
use std::sync::Arc;

use super::Conn;
use crate::{
    error::{Error, Result},
    transport::{BoxedTransport, BoxedUnreliableTransport, NegotiationQuery},
};

use log::info;

pub const PROTOCOL: &[&[u8]] = &[b"iscp"];

/// QUIC用コネクターの設定です。
#[derive(Clone, Debug)]
pub struct ConnectorConfig {
    /// MTUの設定です。
    pub mtu: usize,
    /// サーバー証明書の検証に使用する設定です。
    pub host: String,
    /// 送信されるデータグラムのバッファ最大バイト数
    /// [Read more](https://docs.rs/quinn-proto/latest/quinn_proto/struct.TransportConfig.html#method.datagram_send_buffer_size)
    pub datagram_send_buffer_size: Option<usize>,
    /// 輻輳制御の設定です。
    pub congestion: CongestionConfig,
    /// 接続をタイムアウトするまでの非アクティブ時間の設定です。
    /// [`crate::ConnConfig::ping_interval`]より長い時間を設定してください。
    pub idle_timeout: std::time::Duration,
}

impl Default for ConnectorConfig {
    fn default() -> Self {
        Self {
            mtu: 1500,
            host: "".to_string(),
            datagram_send_buffer_size: None,
            congestion: CongestionConfig::default(),
            idle_timeout: std::time::Duration::from_secs(11), // ping_interval default + 1 [sec]
        }
    }
}

/// 輻輳制御の設定です。
/// [Read more](https://docs.rs/quinn/0.9.3/quinn/congestion/index.html)
#[derive(Clone, Debug)]
pub enum CongestionConfig {
    Default,
    Bbr(quinn::congestion::BbrConfig),
    Cubic(quinn::congestion::CubicConfig),
    NewReno(quinn::congestion::NewRenoConfig),
}

impl Default for CongestionConfig {
    fn default() -> Self {
        Self::Default
    }
}

/// QUIC用のコネクターです。
#[derive(Clone, Debug)]
pub struct Connector {
    cfg: ConnectorConfig,
    cert: Certificate,
    skip_server_verification: bool,
    addr: SocketAddr,
    bind_addr: SocketAddr,
    negotiation: NegotiationQuery,
}

#[allow(dead_code)]
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Certificate {
    Pem(Vec<u8>),
    Der(Vec<u8>),
    None,
}

struct SkipServerVerification;

impl SkipServerVerification {
    fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

impl rustls::client::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::Certificate,
        _intermediates: &[rustls::Certificate],
        _server_name: &rustls::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: std::time::SystemTime,
    ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::ServerCertVerified::assertion())
    }
}

impl TryFrom<Certificate> for rustls::Certificate {
    type Error = crate::error::Error;

    fn try_from(c: Certificate) -> Result<Self> {
        let res = match c {
            Certificate::Pem(pem) => {
                let mut slice = pem.as_slice();
                let certs = rustls_pemfile::certs(&mut slice)
                    .map_err(|e| Error::CertificateLoad(e.to_string()))?;
                rustls::Certificate(certs[0].clone())
            }
            Certificate::Der(der) => rustls::Certificate(der),
            _ => return Err(Error::invalid_value("no certificate")),
        };
        Ok(res)
    }
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
            cert: Certificate::None,
            skip_server_verification: false,
            addr: "127.0.0.1:8080".parse().unwrap(),
            bind_addr: "0.0.0.0:0".parse().unwrap(),
            negotiation: NegotiationQuery::default(),
        }
    }
}

impl Connector {
    pub fn new(addr: SocketAddr, cfg: ConnectorConfig) -> Self {
        Self {
            cfg,
            addr,
            ..Default::default()
        }
    }

    fn to_endpoint(&self) -> Result<quinn::Endpoint> {
        let client_cfg = self.configure_client()?;
        let mut endpoint = quinn::Endpoint::client(self.bind_addr).map_err(Error::unexpected)?;
        endpoint.set_default_client_config(client_cfg);

        Ok(endpoint)
    }

    fn configure_client(&self) -> Result<quinn::ClientConfig> {
        let mut client_crypto = if self.skip_server_verification {
            // Skip server verification
            rustls::ClientConfig::builder()
                .with_safe_defaults()
                .with_custom_certificate_verifier(SkipServerVerification::new())
                .with_no_client_auth()
        } else if matches!(self.cert, Certificate::None) {
            // Use system native root certs
            let mut roots = rustls::RootCertStore::empty();
            let certs = rustls_native_certs::load_native_certs()
                .map_err(|e| Error::CertificateLoad(e.to_string()))?;
            for cert in certs.into_iter() {
                roots.add(&rustls::Certificate(cert.0))?;
            }
            rustls::ClientConfig::builder()
                .with_safe_defaults()
                .with_root_certificates(roots)
                .with_no_client_auth()
        } else {
            // Use given certs
            let mut roots = rustls::RootCertStore::empty();
            let cert: rustls::Certificate = self.cert.clone().try_into()?;
            roots.add(&cert)?;
            rustls::ClientConfig::builder()
                .with_safe_defaults()
                .with_root_certificates(roots)
                .with_no_client_auth()
        };

        client_crypto.alpn_protocols = PROTOCOL.iter().map(|&x| x.into()).collect();

        let mut tr_config = quinn::TransportConfig::default();

        if let Some(value) = self.cfg.datagram_send_buffer_size {
            tr_config.datagram_send_buffer_size(value);
        }
        tr_config.max_idle_timeout(Some(
            quinn::IdleTimeout::try_from(self.cfg.idle_timeout).map_err(Error::invalid_value)?,
        ));
        tr_config.keep_alive_interval(None);

        match &self.cfg.congestion {
            CongestionConfig::Default => (),
            CongestionConfig::Bbr(bbr) => {
                tr_config.congestion_controller_factory(Arc::new(bbr.clone()));
            }
            CongestionConfig::Cubic(cubic) => {
                tr_config.congestion_controller_factory(Arc::new(cubic.clone()));
            }
            CongestionConfig::NewReno(new_reno) => {
                tr_config.congestion_controller_factory(Arc::new(new_reno.clone()));
            }
        }

        let mut config = quinn::ClientConfig::new(Arc::new(client_crypto));

        config.transport_config(Arc::new(tr_config));

        Ok(config)
    }

    async fn exec_negotiation(&self, conn: &quinn::Connection) -> Result<()> {
        let mut stream = conn.open_uni().await?;
        let negotiation = self.negotiation.bytes()?;
        stream.write_all(&negotiation).await?;
        stream.finish().await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl crate::transport::Connector for Connector {
    async fn connect(&self) -> Result<(BoxedTransport, Option<BoxedUnreliableTransport>)> {
        log::trace!("quic connector config: {:?}", self.cfg);

        let e = self.to_endpoint()?;

        let connection = e.connect(self.addr, &self.cfg.host)?.await?;
        info!("connected: addr = {}", connection.remote_address());
        self.exec_negotiation(&connection).await?;
        info!("negotiation complete");

        let conn = Conn::new(connection, e, self.cfg.mtu, self.cfg.idle_timeout).await?;

        let utr = Box::new(conn.clone());
        let utr = utr as BoxedUnreliableTransport;

        Ok((Box::new(conn), Some(utr)))
    }
}
