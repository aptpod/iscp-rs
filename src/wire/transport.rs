use crate::{enc, msg, tr, Result};

use async_trait::async_trait;

pub struct Tr<T, E> {
    tr: T,
    enc: E,
}

impl<T, E> Tr<T, E> {
    pub fn new(tr: T, enc: E) -> Self {
        Self { tr, enc }
    }
}

#[async_trait]
impl<T, E> super::Transport for Tr<T, E>
where
    T: tr::Transport,
    E: enc::Encoder,
{
    async fn read_message(&self) -> Result<msg::Message> {
        let bin = self.tr.read().await?;
        let msg = self.enc.decode(&bin)?;
        Ok(msg)
    }
    async fn write_message(&self, msg: msg::Message) -> Result<()> {
        let bin = self.enc.encode(msg)?;
        self.tr.write(&bin).await?;
        Ok(())
    }
    async fn close(&self) -> Result<()> {
        self.tr.close().await?;
        Ok(())
    }
}

#[async_trait]
impl<T, E> super::UnreliableTransport for Tr<T, E>
where
    T: tr::UnreliableTransport,
    E: enc::Encoder,
{
    async fn unreliable_read_message(&self) -> Result<msg::Message> {
        let bin = self.tr.unreliable_read().await?;
        let msg = self.enc.decode(&bin)?;
        Ok(msg)
    }
    async fn unreliable_write_message(&self, msg: msg::Message) -> Result<()> {
        let bin = self.enc.encode(msg)?;
        self.tr.unreliable_write(&bin).await?;
        Ok(())
    }
}
