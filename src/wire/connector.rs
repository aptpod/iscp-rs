use std::time::Duration;

use crate::{enc, tr, Result};
use async_trait::async_trait;
use log::info;

#[derive(Clone, Debug)]
struct Connector {
    tr_connector: tr::Connector,
    encoding: enc::Encoding,
}

pub fn new_connector(
    tr_connector: tr::Connector,
    encoding: enc::Encoding,
) -> impl super::Connector {
    Connector {
        tr_connector,
        encoding,
    }
}

impl Connector {
    pub async fn connect(&self, timeout: Option<Duration>) -> Result<super::BoxedConnection> {
        let (tr, maybe_utr) = self.tr_connector.connect().await?;
        info!("transport connector successfully connected");

        let enc = self.encoding.generate()?;
        let tr = super::transport::Tr::new(tr, enc);

        let enc = self.encoding.generate()?;
        let utr = maybe_utr.map(|utr| {
            let res = super::transport::Tr::new(utr, enc);
            Box::new(res) as super::BoxedUnreliableTransport
        });

        let conn = super::connection::connect(tr, utr, timeout).await?;

        info!("wire connector successfully connected");
        Ok(Box::new(conn))
    }
}

#[async_trait]
impl super::Connector for Connector {
    async fn connect(&self, timeout: Option<Duration>) -> Result<super::BoxedConnection> {
        let conn = self.connect(timeout).await?;
        Ok(Box::new(conn))
    }
}
