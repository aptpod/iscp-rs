pub mod connector;
pub use connector::*;
mod error;
mod frame;
use frame::*;

use crate::{
    error::{Error, Result},
    sync::{Cancel, WaitGroup, Waiter},
};
use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use log::*;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::AsyncReadExt,
    sync::{broadcast, mpsc, oneshot},
};

#[derive(Clone, Debug)]
enum Command<T> {
    Cmd(T),
    Term,
}

#[derive(Clone, Debug)]
pub struct Conn {
    cancel: Cancel,
    buf: Arc<Mutex<DatagramsBuffer<'static>>>,
    mtu: usize,
    close_notify: broadcast::Sender<()>,
    endpoint: quinn::Endpoint,
    conn: quinn::Connection,
    read_cmd_sender: mpsc::Sender<oneshot::Sender<Vec<u8>>>,
    write_cmd_sender: mpsc::Sender<Command<Bytes>>,
    datagrams_frame_id_generator: Arc<DatagramsFrameIdGenerator>,
    datagrams_read_cmd_sender: mpsc::Sender<oneshot::Sender<Vec<u8>>>,
    datagrams_write_cmd_sender: mpsc::Sender<Command<Bytes>>,
    timeout: Duration,
}

impl Conn {
    async fn new(
        connection: quinn::Connection,
        endpoint: quinn::Endpoint,
        mut mtu: usize,
        timeout: std::time::Duration,
    ) -> Result<Self> {
        let (read_cmd_sender, read_cmd_receiver) = mpsc::channel(1);
        let (write_cmd_sender, write_cmd_receiver) = mpsc::channel(1);

        let (datagrams_read_cmd_sender, datagrams_read_cmd_receiver) = mpsc::channel(1);
        let (datagrams_write_cmd_sender, datagrams_write_cmd_receiver) = mpsc::channel(1);

        let (frame_sender, frame_receiver) = mpsc::channel(1);
        let (datagrams_frame_sender, datagrams_frame_receiver) = mpsc::channel(1024);
        let (close_notify, _) = broadcast::channel(1);

        if let Some(max) = connection.max_datagram_size() {
            mtu = mtu.min(max);
            info!("reset MTU: {}", mtu)
        }

        let conn = Conn {
            mtu,
            buf: Arc::default(),
            endpoint,
            close_notify,
            cancel: Cancel::new(),
            timeout,
            read_cmd_sender,
            write_cmd_sender,
            datagrams_read_cmd_sender,
            datagrams_write_cmd_sender,
            datagrams_frame_id_generator: Arc::default(),
            conn: connection,
        };

        let (waiter, wg) = WaitGroup::new();

        let conn_c = conn.clone();
        let _wg = wg.clone();
        tokio::spawn(async move {
            log_err!(
                warn,
                conn_c.read_loop(_wg, frame_sender).await,
                "in read_loop"
            );
            trace!("exit read_loop");
        });

        let conn_c = conn.clone();
        let _wg = wg.clone();
        tokio::spawn(async move {
            conn_c
                .datagrams_read_loop(_wg, datagrams_frame_sender)
                .await;
            trace!("exit datagrams_read_loop");
        });

        let conn_c = conn.clone();
        let _wg = wg.clone();
        tokio::spawn(async move {
            conn_c
                .read_cmd_loop(_wg, frame_receiver, read_cmd_receiver)
                .await;
            trace!("exit read_cmd_loop");
        });

        let conn_c = conn.clone();
        let _wg = wg.clone();
        let stream = conn.conn.open_uni().await?;
        tokio::spawn(async move {
            log_err!(
                warn,
                conn_c.write_cmd_loop(_wg, stream, write_cmd_receiver).await,
                "in write_cmd_loop"
            );
            trace!("exit write_cmd_loop");
        });

        let conn_c = conn.clone();
        let _wg = wg.clone();
        tokio::spawn(async move {
            conn_c
                .datagrams_read_cmd_loop(_wg, datagrams_frame_receiver, datagrams_read_cmd_receiver)
                .await;
            trace!("exit datagrams_read_cmd_loop");
        });

        let conn_c = conn.clone();
        let _wg = wg.clone();
        tokio::spawn(async move {
            log_err!(
                warn,
                conn_c
                    .datagrams_write_cmd_loop(_wg, datagrams_write_cmd_receiver)
                    .await,
                "in datagrams_write_cmd_loop"
            );
            trace!("exit datagrams_write_cmd_loop");
        });

        let conn_c = conn.clone();
        let _wg = wg;
        tokio::spawn(async move {
            conn_c.clear_timeout_buf(_wg).await;
            trace!("exit clear_timeout_buf")
        });

        let conn_c = conn.clone();
        tokio::spawn(async move {
            conn_c.close_waiter(waiter).await;
            trace!("exit close waiter");
        });

        Ok(conn)
    }
}

impl Conn {
    pub async fn read(&self) -> Result<Vec<u8>> {
        let (s, r) = oneshot::channel();
        self.read_cmd_sender.send(s).await?;
        let res = tokio::time::timeout(self.timeout, r).await??;

        Ok(res)
    }
    pub async fn write(&self, bin: &[u8]) -> Result<()> {
        self.write_cmd_sender
            .send(Command::Cmd(Frame::new(bin).to_bytes()))
            .await?;
        Ok(())
    }

    pub async fn close(&self) -> Result<()> {
        let mut close_notified = self.close_notify.subscribe();

        self.write_cmd_sender.send(Command::Term).await?;
        self.datagrams_write_cmd_sender.send(Command::Term).await?;

        log_err!(trace, self.cancel.notify());

        tokio::time::timeout(self.timeout, close_notified.recv()).await??;
        self.conn.close(0u32.into(), b"done");
        self.endpoint.wait_idle().await;
        Ok(())
    }

    pub async fn unreliable_write(&self, bin: &[u8]) -> Result<()> {
        let id = self.datagrams_frame_id_generator.next();
        let frames = DatagramsFrame::new(bin, id, self.mtu)?;
        trace!("write datagram frame: id = {}, num = {}", id, frames.len());

        for frame in frames.into_iter() {
            self.datagrams_write_cmd_sender
                .send(Command::Cmd(frame.into_bytes()))
                .await
                .map_err(Error::unexpected)?;
        }
        Ok(())
    }

    pub async fn unreliable_read(&self) -> Result<Vec<u8>> {
        let (s, r) = oneshot::channel();
        self.datagrams_read_cmd_sender.send(s).await?;
        let res = r.await?;

        Ok(res)
    }
}

// background tasks
impl Conn {
    async fn close_waiter(&self, mut waiter: Waiter) {
        waiter.wait().await;
        log_err!(trace, self.close_notify.send(()));
    }

    async fn read_loop(&self, _wg: WaitGroup, sender: mpsc::Sender<Vec<u8>>) -> Result<()> {
        loop {
            let stream = tokio::select! {
                _ = self.cancel.notified() => break,
                Ok(recv) = self.conn.accept_uni() => recv,
            };
            trace!("coming new uni stream");

            let frame_sender = sender.clone();
            let wg = _wg.clone();
            let cancel = self.cancel.clone();
            tokio::spawn(async move {
                log_err!(
                    warn,
                    Self::extract_frame_from_stream(cancel, wg, stream, frame_sender,).await,
                    "in extract_frame_from_stream"
                );
                trace!("exit extract_frame_from_stream");
            });
        }
        Ok(())
    }

    async fn extract_frame_from_stream(
        cancel: Cancel,
        _wg: WaitGroup,
        mut stream: quinn::RecvStream,
        sender: mpsc::Sender<Vec<u8>>,
    ) -> Result<()> {
        loop {
            let len = tokio::select! {
                _ = cancel.notified() => break,
                res = stream.read_u32() => res.map_err(|e| Error::Unexpected(format!("{:?}", e)))? as usize,
            };

            trace!("got header: payload len = {}", len);
            let mut buf = BytesMut::with_capacity(len);
            buf.resize(len, 0);

            tokio::select! {
                _ = cancel.notified() => break,
                res = stream.read_exact(&mut buf) => res.map_err(|e| Error::Unexpected(format!("{:?}", e)))?,
            };

            log_err!(
                warn,
                sender.send(buf.to_vec()).await,
                "extract_frame_from_stream"
            );
        }
        Ok(())
    }

    async fn read_cmd_loop(
        &self,
        _wg: WaitGroup,
        mut recv: mpsc::Receiver<Vec<u8>>,
        mut cmd_recv: mpsc::Receiver<oneshot::Sender<Vec<u8>>>,
    ) {
        loop {
            let maybe_sender = tokio::select! {
                s = cmd_recv.recv() => s,
                _ = self.cancel.notified() => break,
            };
            let sender = match maybe_sender {
                Some(s) => s,
                None => break,
            };
            let res = tokio::select! {
                res = recv.recv() => res,
                _ = self.cancel.notified() => break,
            };

            let bin = match res {
                Some(bin) => bin,
                None => break,
            };
            if let Err(e) = sender.send(bin) {
                warn!("drop message: {:?}", e);
            }
        }
    }

    async fn write_cmd_loop(
        &self,
        _wg: WaitGroup,
        mut stream: quinn::SendStream,
        mut cmd_recv: mpsc::Receiver<Command<Bytes>>,
    ) -> Result<()> {
        while let Some(cmd) = cmd_recv.recv().await {
            let bin = match cmd {
                Command::Cmd(bin) => bin,
                Command::Term => break,
            };
            log_err!(error, stream.write(&bin).await);
        }

        Ok(stream.finish().await?)
    }

    async fn datagrams_read_loop(&self, _wg: WaitGroup, sender: mpsc::Sender<Vec<u8>>) {
        loop {
            let segment = tokio::select! {
                _ = self.cancel.notified() => break,
                Ok(segment) = self.conn.read_datagram() => segment,
            };

            let maybe_msg = {
                let mut buf = self.buf.lock().unwrap();
                match DatagramsFrame::parse(segment) {
                    Ok(f) => buf.push_and_parse(f),
                    Err(e) => {
                        warn!("datagrams frame parse error: {}", e);
                        continue;
                    }
                }
            };
            let msg = match maybe_msg {
                Some(msg) => msg,
                None => continue,
            };
            log_err!(warn, sender.try_send(msg), "datagrams_read_loop");
        }
    }

    async fn datagrams_write_cmd_loop(
        &self,
        _wg: WaitGroup,
        mut cmd_recv: mpsc::Receiver<Command<Bytes>>,
    ) -> Result<()> {
        while let Some(cmd) = cmd_recv.recv().await {
            let bin = match cmd {
                Command::Cmd(bin) => bin,
                Command::Term => break,
            };
            log_err!(warn, self.conn.send_datagram(bin), "send_datagram");
        }
        Ok(())
    }

    async fn datagrams_read_cmd_loop(
        &self,
        _wg: WaitGroup,
        mut recv: mpsc::Receiver<Vec<u8>>,
        mut cmd_recv: mpsc::Receiver<oneshot::Sender<Vec<u8>>>,
    ) {
        loop {
            let maybe_sender = tokio::select! {
                s = cmd_recv.recv() => s,
                _ = self.cancel.notified() => break,
            };
            let sender = match maybe_sender {
                Some(s) => s,
                None => break,
            };
            let res = tokio::select! {
                res = recv.recv() => res,
                _ = self.cancel.notified() => break,
            };

            let bin = match res {
                Some(bin) => bin,
                None => break,
            };
            if let Err(e) = sender.send(bin) {
                warn!("drop message: {:?}", e);
            }
        }
    }

    async fn clear_timeout_buf(&self, _wg: WaitGroup) {
        let mut ticker = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = self.cancel.notified() => break,
                _ = ticker.tick() => {
                    self.buf.lock().unwrap().clear_timeout_buf();
                },
            };
        }
    }
}

#[async_trait]
impl super::Transport for Conn {
    async fn read(&self) -> Result<Vec<u8>> {
        self.read().await
    }
    async fn write(&self, bin: &[u8]) -> Result<()> {
        self.write(bin).await
    }
    async fn close(&self) -> Result<()> {
        self.close().await
    }
}

#[async_trait]
impl super::UnreliableTransport for Conn {
    async fn unreliable_read(&self) -> Result<Vec<u8>> {
        self.unreliable_read().await
    }
    async fn unreliable_write(&self, buf: &[u8]) -> Result<()> {
        self.unreliable_write(buf).await
    }
}
