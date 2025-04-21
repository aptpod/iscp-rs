//! Transport methods.

mod compression;
mod negotiation;
pub mod quic;
pub mod websocket;

#[cfg(feature = "unstable-webtransport")]
#[cfg_attr(docsrs, doc(cfg(feature = "unstable-webtransport")))]
pub mod webtransport;

mod datagram;
mod tls;

pub use compression::*;
pub use negotiation::*;
pub use tls::Certificate;

pub struct TransportError {
    inner: Box<dyn std::error::Error + Send + Sync>,
}

impl std::fmt::Debug for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("TransportError").field(&self.inner).finish()
    }
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "transport error: {}", self.inner)
    }
}

impl std::error::Error for TransportError {}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
struct TransportErrorMessage(String);

impl TransportError {
    pub fn new<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self {
            inner: Box::new(err),
        }
    }

    pub fn from_msg<T: std::fmt::Display>(msg: T) -> Self {
        Self {
            inner: Box::new(TransportErrorMessage(msg.to_string())),
        }
    }
}

pub trait TransportWriter: Send {
    fn write(
        &mut self,
        data: &[u8],
    ) -> impl std::future::Future<Output = Result<(), TransportError>> + Send;

    fn close(&mut self) -> impl std::future::Future<Output = Result<(), TransportError>> + Send;
}

pub trait TransportReader: Send {
    fn read(
        &mut self,
        buf: &mut bytes::BytesMut,
    ) -> impl std::future::Future<Output = Result<(), TransportError>> + Send;

    fn close(&mut self) -> impl std::future::Future<Output = Result<(), TransportError>> + Send;
}

pub trait TransportCloser: Send {
    fn close(&mut self) -> impl std::future::Future<Output = Result<(), TransportError>> + Send;
}

pub struct Transport<C: Connector> {
    pub reader: C::Reader,
    pub writer: C::Writer,
    pub closer: C::Closer,
    pub unreliable_reader: Option<C::UnreliableReader>,
    pub unreliable_writer: Option<C::UnreliableWriter>,
}

pub trait Connector: Send + Sized + 'static {
    type Reader: TransportReader + 'static;
    type Writer: TransportWriter + 'static;
    type Closer: TransportCloser + 'static;
    type UnreliableReader: TransportReader + 'static;
    type UnreliableWriter: TransportWriter + 'static;

    fn connect(
        &self,
        negotiation_params: NegotiationParams,
    ) -> impl std::future::Future<Output = Result<Transport<Self>, TransportError>> + Send;

    #[doc(hidden)]
    fn _compat_with_context_takeover() -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug)]
pub enum UnreliableNotSupported {}

impl TransportReader for UnreliableNotSupported {
    async fn read(&mut self, _buf: &mut bytes::BytesMut) -> Result<(), TransportError> {
        unreachable!()
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        unreachable!()
    }
}

impl TransportWriter for UnreliableNotSupported {
    async fn write(&mut self, _data: &[u8]) -> Result<(), TransportError> {
        unreachable!()
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        unreachable!()
    }
}
