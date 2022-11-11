use std::net::SocketAddr;
use std::sync::Arc;

use super::Conn;
use crate::{
    error::{Error, Result},
    transport::{BoxedUnreliableTransport, NegotiationQuery, Transport},
};

use log::info;
use rustls::client::HandshakeSignatureValid;
use rustls::internal::msgs::handshake::DigitallySignedStruct;

pub const PROTOCOL: &[&[u8]] = &[b"iscp"];

/// QUIC用コネクターの設定です。
#[derive(Clone, Debug)]
pub struct ConnectorConfig {
    /// MTUの設定です。
    pub mtu: usize,

    /// サーバー証明書の検証に使用する設定です。
    pub host: String,
}

impl Default for ConnectorConfig {
    fn default() -> Self {
        Self {
            mtu: 1500,
            host: "".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Connector {
    cfg: ConnectorConfig,
    cert: Certificate,
    skip_server_verification: bool,
    addr: SocketAddr,
    bind_addr: SocketAddr,
    negotiation: NegotiationQuery,
    timeout: std::time::Duration,
    datagram_send_buffer_size: Option<usize>,
}

#[allow(dead_code)]
#[derive(Clone, PartialEq, Debug)]
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

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::Certificate,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::Certificate,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
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

impl From<Connector> for super::super::Connector {
    fn from(c: Connector) -> Self {
        Self::Quic(c)
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
            timeout: std::time::Duration::from_secs(2),
            datagram_send_buffer_size: None,
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
        let mut config = quinn::ClientConfig::new(Arc::new(client_crypto));

        let tr_config = Arc::get_mut(&mut config.transport).unwrap();

        if let Some(value) = self.datagram_send_buffer_size {
            tr_config.datagram_send_buffer_size(value);
        }
        tr_config.max_idle_timeout(Some(
            quinn::IdleTimeout::try_from(self.timeout).map_err(Error::invalid_value)?,
        ));
        tr_config.keep_alive_interval(None);

        Ok(config)
    }

    pub async fn connect(&self) -> Result<(impl Transport, Option<BoxedUnreliableTransport>)> {
        let e = self.to_endpoint()?;

        let new_conn = e.connect(self.addr, &self.cfg.host)?.await?;
        info!("connected: addr = {}", new_conn.connection.remote_address());
        self.exec_negotiation(&new_conn.connection).await?;
        info!("negotiation complete");

        let conn = Conn::new(new_conn, e, self.cfg.mtu, self.timeout).await?;

        let utr = Box::new(conn.clone());
        let utr = utr as BoxedUnreliableTransport;

        Ok((conn, Some(utr)))
    }

    async fn exec_negotiation(&self, conn: &quinn::Connection) -> Result<()> {
        let mut stream = conn.open_uni().await?;
        let negotiation = self.negotiation.bytes()?;
        stream.write_all(&negotiation).await?;
        stream.finish().await?;
        Ok(())
    }
}
