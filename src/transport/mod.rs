//! iSCP で使用するトランスポートを定義するモジュールです。

use async_trait::async_trait;

mod negotiation;
mod quic;
mod ws;

pub(crate) use negotiation::NegotiationQuery;
pub use quic::Connector as QuicConnector;
pub use quic::ConnectorConfig as QuicConfig;
pub use ws::Connector as WebSocketConnector;
pub use ws::ConnectorConfig as WebSocketConfig;

use crate::Result;

#[async_trait]
pub trait Transport: Send + Sync {
    async fn read(&self) -> Result<Vec<u8>>;
    async fn write(&self, buf: &[u8]) -> Result<()>;
    async fn close(&self) -> Result<()>;
}

pub type BoxedTransport = Box<dyn Transport>;

#[async_trait]
impl<T: Transport + ?Sized> Transport for Box<T> {
    async fn read(&self) -> Result<Vec<u8>> {
        (**self).read().await
    }
    async fn write(&self, buf: &[u8]) -> Result<()> {
        (**self).write(buf).await
    }
    async fn close(&self) -> Result<()> {
        (**self).close().await
    }
}

#[async_trait]
pub trait UnreliableTransport: Send + Sync {
    async fn unreliable_read(&self) -> Result<Vec<u8>>;
    async fn unreliable_write(&self, buf: &[u8]) -> Result<()>;
}

pub type BoxedUnreliableTransport = Box<dyn UnreliableTransport>;

#[async_trait]
impl<U: UnreliableTransport + ?Sized> UnreliableTransport for Box<U> {
    async fn unreliable_read(&self) -> Result<Vec<u8>> {
        (**self).unreliable_read().await
    }
    async fn unreliable_write(&self, buf: &[u8]) -> Result<()> {
        (**self).unreliable_write(buf).await
    }
}

#[derive(Clone, Debug)]
pub enum Connector {
    Ws(WebSocketConnector),
    Quic(QuicConnector),
}

impl Connector {
    pub async fn connect(&self) -> Result<(BoxedTransport, Option<BoxedUnreliableTransport>)> {
        match self {
            Self::Ws(conn) => {
                let res = conn.connect().await?;
                Ok((Box::new(res), None))
            }
            Self::Quic(conn) => {
                let (tr, utr) = conn.connect().await?;
                Ok((Box::new(tr), utr))
            }
        }
    }
}
