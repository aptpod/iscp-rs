mod connector;

use std::borrow::Cow;

use crate::{
    error::{Error, Result},
    sync::{Cancel, WaitGroup, Waiter},
};
pub use connector::*;

use async_trait::async_trait;
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use tokio::{
    net::TcpStream,
    sync::{broadcast, mpsc, oneshot},
};
use tokio_tungstenite::{
    tungstenite::{handshake::client::Request, protocol, Message},
    MaybeTlsStream, WebSocketStream,
};
use tungstenite::protocol::WebSocketConfig;

use super::Transport;
use log::*;

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Clone, Debug)]
enum Command {
    Message(Message),
    Term,
}

impl From<Message> for Command {
    fn from(m: Message) -> Self {
        Self::Message(m)
    }
}

#[derive(Clone, Debug)]
struct Conn {
    tx_sender: mpsc::Sender<Command>,
    recv_cmd_sender: mpsc::Sender<oneshot::Sender<Vec<u8>>>,
    close_notify: broadcast::Sender<()>,
    cancel: Cancel,
    timeout: std::time::Duration,
}

impl Conn {
    pub async fn connect(
        request: Request,
        timeout: std::time::Duration,
        wsconf: WebSocketConfig,
        send_queue_size: Option<usize>,
    ) -> Result<Self> {
        let (ws_stream, _) = self::connect_async_with_config(request, Some(wsconf))
            .await
            .map_err(Error::connect)?;

        let (sink, stream) = ws_stream.split();
        let (rx_sender, rx_receiver) = mpsc::channel(1);
        let (tx_sender, tx_receiver) = mpsc::channel(send_queue_size.unwrap_or(1));
        let (cmd_sender, cmd_receiver) = mpsc::channel(1);

        let (waiter, wg) = WaitGroup::new();

        let cancel = Cancel::new();
        let (close_notify, _) = broadcast::channel(1);

        let conn = Self {
            tx_sender,
            recv_cmd_sender: cmd_sender,
            close_notify,
            cancel,
            timeout,
        };

        let conn_c = conn.clone();
        let wg_c = wg.clone();
        tokio::spawn(
            async move { log_err!(warn, conn_c.read_loop(wg_c, stream, rx_sender).await) },
        );

        let conn_c = conn.clone();
        let wg_c = wg.clone();
        tokio::spawn(
            async move { log_err!(warn, conn_c.write_loop(wg_c, sink, tx_receiver).await) },
        );

        let conn_c = conn.clone();
        let wg_c = wg;
        tokio::spawn(async move {
            log_err!(warn, conn_c.cmd_loop(wg_c, cmd_receiver, rx_receiver).await)
        });

        let conn_c = conn.clone();
        tokio::spawn(async move { conn_c.close_waiter(waiter).await });

        Ok(conn)
    }
}

// connect with TCP_NODELAY option
async fn connect_async_with_config<R>(
    request: R,
    config: Option<WebSocketConfig>,
) -> Result<
    (
        WebSocketStream<MaybeTlsStream<TcpStream>>,
        tungstenite::handshake::client::Response,
    ),
    Error,
>
where
    R: tungstenite::client::IntoClientRequest + Unpin,
{
    let request = request.into_client_request()?;

    let domain = if let Some(d) = request.uri().host() {
        d.to_string()
    } else {
        return Err(Error::Connect("no host name".into()));
    };
    let port = request
        .uri()
        .port_u16()
        .or_else(|| match request.uri().scheme_str() {
            Some("wss") => Some(443),
            Some("ws") => Some(80),
            _ => None,
        })
        .ok_or_else(|| Error::InvalidValue("unsupported url scheme".into()))?;

    let addr = format!("{}:{}", domain, port);
    let socket = TcpStream::connect(addr)
        .await
        .map_err(|e| Error::Connect(e.to_string()))?;
    // Set TCP_NODELAY option
    socket
        .set_nodelay(true)
        .map_err(|e| Error::Connect(format!("cannot set TCP_NODELAY: {}", e)))?;

    tokio_tungstenite::client_async_tls_with_config(request, socket, config, None)
        .await
        .map_err(|e| Error::Connect(e.to_string()))
}

#[async_trait]
impl Transport for Conn {
    async fn read(&self) -> Result<Vec<u8>> {
        let (s, r) = oneshot::channel();
        self.recv_cmd_sender.send(s).await?;

        r.await
            .map_err(|_| Error::ConnectionClosed("during read".into()))
    }
    async fn write(&self, buf: &[u8]) -> Result<()> {
        let msg = Message::binary(buf.to_vec());

        self.tx_sender
            .send(Command::from(msg))
            .await
            .map_err(|_| Error::ConnectionClosed("during write".into()))?;

        Ok(())
    }
    async fn close(&self) -> Result<()> {
        self.tx_sender.send(Command::Term).await?;
        log_err!(trace, self.cancel.notify());

        self.close_notify
            .subscribe()
            .recv()
            .await
            .map_err(Error::unexpected)?;
        Ok(())
    }
}

impl Conn {
    async fn read_loop(
        &self,
        _wg: WaitGroup,
        mut stream: SplitStream<WsStream>,
        sender: mpsc::Sender<Vec<u8>>,
    ) -> Result<()> {
        while let Some(res) = stream.next().await {
            let msg = res?;
            match msg {
                Message::Close(_) => {
                    debug!("receive close frame");
                    break;
                }
                Message::Ping(p) => {
                    self.tx_sender.send(Message::Pong(p).into()).await?;
                    continue;
                }
                Message::Binary(bin) => {
                    sender.send(bin).await?;
                }
                _ => continue,
            };
        }

        trace!("exit read loop");
        Ok(())
    }

    async fn write_loop(
        &self,
        _wg: WaitGroup,
        mut sink: SplitSink<WsStream, Message>,
        mut recv: mpsc::Receiver<Command>,
    ) -> Result<()> {
        while let Some(cmd) = recv.recv().await {
            let msg = match cmd {
                Command::Message(m) => m,
                Command::Term => break,
            };

            tokio::select! {
                result = sink.send(msg) => {
                    result?;
                }
                _ = tokio::time::sleep(self.timeout) => {
                    return Err(Error::TimeOut);
                }
            }
        }
        trace!("exit write loop");
        let normal_close_frame = protocol::CloseFrame {
            code: protocol::frame::coding::CloseCode::Normal,
            reason: Cow::Owned("OK".to_string()),
        };
        log_err!(
            trace,
            sink.send(Message::Close(Some(normal_close_frame))).await
        );
        sink.close().await?;
        Ok(())
    }

    async fn cmd_loop(
        &self,
        _wg: WaitGroup,
        mut cmd_recv: mpsc::Receiver<oneshot::Sender<Vec<u8>>>,
        mut rx_recv: mpsc::Receiver<Vec<u8>>,
    ) -> Result<()> {
        loop {
            let maybe_sender = tokio::select! {
                _ = self.cancel.notified() => break,
                recv = cmd_recv.recv() => recv,
            };
            let sender = match maybe_sender {
                Some(s) => s,
                None => break,
            };

            let res = tokio::select! {
                res = rx_recv.recv() => res,
                _ = self.cancel.notified() => break,
            };
            match res {
                None => break,
                Some(msg) => {
                    if let Err(e) = sender.send(msg) {
                        warn!("drop message: {:?}", e);
                    }
                }
            };
        }
        trace!("exit cmd loop");
        Ok(())
    }

    async fn close_waiter(&self, mut waiter: Waiter) {
        waiter.wait().await;
        let _res = self.close_notify.send(());
        trace!("exit notify waiter");
    }
}
