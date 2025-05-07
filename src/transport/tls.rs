use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};

use super::TransportError;

#[derive(Debug)]
struct SkipServerVerification(Arc<rustls::crypto::CryptoProvider>);

impl SkipServerVerification {
    fn new() -> Arc<Self> {
        Arc::new(Self(Arc::new(rustls::crypto::ring::default_provider())))
    }
}

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// 証明書データ
#[derive(Clone, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum Certificate {
    Pem(Vec<u8>),
    Der(Vec<u8>),
}

impl Certificate {
    fn into_rustls_cert(self) -> Result<Vec<CertificateDer<'static>>, TransportError> {
        let res = match self {
            Certificate::Pem(pem) => {
                let mut slice = pem.as_slice();
                let certs: Vec<_> = rustls_pemfile::certs(&mut slice)
                    .filter_map(|result| match result {
                        Ok(cert) => Some(cert),
                        Err(e) => {
                            log::error!("{}", e);
                            None
                        }
                    })
                    .collect();
                if certs.is_empty() {
                    return Err(TransportError::from_msg("invalid pem data"));
                }
                certs
            }
            Certificate::Der(der) => vec![CertificateDer::from_slice(&der).into_owned()],
        };
        Ok(res)
    }

    fn into_rustls_key(self) -> Result<PrivateKeyDer<'static>, TransportError> {
        let res = match self {
            Certificate::Pem(pem) => {
                let mut slice = pem.as_slice();
                let items: Vec<_> = rustls_pemfile::read_all(&mut slice)
                    .filter_map(|result| match result {
                        Ok(a) => Some(a),
                        Err(e) => {
                            log::warn!("error in reading pem: {}", e);
                            None
                        }
                    })
                    .collect();
                if items.len() > 1 {
                    log::warn!("multiple keys found, use the first item");
                }
                match items.first() {
                    Some(rustls_pemfile::Item::Pkcs1Key(der)) => {
                        PrivateKeyDer::Pkcs1(der.clone_key())
                    }
                    Some(rustls_pemfile::Item::Pkcs8Key(der)) => {
                        PrivateKeyDer::Pkcs8(der.clone_key())
                    }
                    Some(rustls_pemfile::Item::Sec1Key(der)) => {
                        PrivateKeyDer::Sec1(der.clone_key())
                    }
                    _ => {
                        return Err(TransportError::from_msg("no key found"));
                    }
                }
            }
            Certificate::Der(der) => {
                PrivateKeyDer::try_from(der).map_err(TransportError::from_msg)?
            }
        };
        Ok(res)
    }
}

pub fn rustls_config_client(
    skip_server_verification: bool,
    client_auth_cert_and_key: Option<(Certificate, Certificate)>,
) -> Result<Option<rustls::ClientConfig>, TransportError> {
    let client_auth_cert_and_key = if let Some((cert, key)) = client_auth_cert_and_key {
        Some((cert.into_rustls_cert()?, key.into_rustls_key()?))
    } else {
        None
    };

    let client_config = if skip_server_verification {
        // Skip server verification
        let builder = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(SkipServerVerification::new());
        if let Some((client_auth_cert, client_auth_key)) = client_auth_cert_and_key {
            builder
                .with_client_auth_cert(client_auth_cert, client_auth_key)
                .map_err(TransportError::new)?
        } else {
            builder.with_no_client_auth()
        }
    } else if let Some((client_auth_cert, client_auth_key)) = client_auth_cert_and_key {
        // Use system native root certs and use client auth
        let mut roots = rustls::RootCertStore::empty();
        let certs = load_native_certs().map_err(TransportError::new)?;
        for cert in certs.into_iter() {
            roots.add(cert).map_err(TransportError::new)?;
        }
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_client_auth_cert(client_auth_cert, client_auth_key)
            .map_err(TransportError::new)?
    } else {
        // Use default tls config
        return Ok(None);
    };

    Ok(Some(client_config))
}

pub fn load_native_certs() -> Result<Vec<CertificateDer<'static>>, TransportError> {
    let rustls_native_certs::CertificateResult { certs, errors, .. } =
        rustls_native_certs::load_native_certs();

    for e in errors {
        log::warn!("loat native cert error: {}", e);
    }

    if certs.is_empty() {
        return Err(TransportError::from_msg(
            "cannot get any native certificate",
        ));
    }
    Ok(certs)
}
