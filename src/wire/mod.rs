//! Wire connection definitions.

mod read_loop;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use bytes::BytesMut;
use crossbeam::atomic::AtomicCell;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::{
    encoding::{Encoder, Encoding},
    error::Error,
    internal::{timeout_with_ct, Waiter},
    message::{
        DownstreamCall, HasRequestId, HasResultCode, Message, RequestMessage, UpstreamCallAck,
    },
    transport::{
        Compression, Compressor, Connector, Transport, TransportCloser, TransportReader,
        TransportWriter,
    },
};

pub(crate) use read_loop::*;

#[derive(Clone)]
pub struct Conn {
    inner: Arc<ConnInner>,
}

impl std::fmt::Debug for Conn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Conn").finish()
    }
}

struct ConnInner {
    tx_write_message: mpsc::Sender<Message>,
    tx_unreliable_write_message: Option<mpsc::Sender<Message>>,
    tx_read_loop_command: mpsc::UnboundedSender<read_loop::ReadLoopCommand>,
    tx_unreliable_read_loop_command: Option<mpsc::UnboundedSender<read_loop::ReadLoopCommand>>,
    rx_downstream_call: broadcast::Receiver<DownstreamCall>,
    response_senders: Mutex<fnv::FnvHashMap<u32, oneshot::Sender<Message>>>,
    request_id_counter: RequestIdCounter,
    ct: CancellationToken,
    waiter: Waiter,
    waiter_rw: Waiter,
    response_message_timeout: Duration,
    downstream_stream_id_alias_counter: AtomicCell<u32>,
    channel_size: usize,
}

impl Conn {
    pub(crate) fn new<C: Connector>(
        transport: Transport<C>,
        encoding: Encoding,
        compression: Compression,
        channel_size: usize,
        timeout: Duration,
        ping_interval: Duration,
        ping_timeout: Duration,
    ) -> Self {
        let Transport {
            mut reader,
            mut writer,
            closer,
            unreliable_reader,
            unreliable_writer,
        } = transport;

        let (tx_write_message, rx_write_message) = mpsc::channel(channel_size);
        let (tx_pong, rx_pong) = mpsc::channel(channel_size);
        let (tx_downstream_call, rx_downstream_call) = broadcast::channel(1);
        let (tx_read_loop_command, rx_read_loop_command) = mpsc::unbounded_channel();
        let (waiter, wg) = Waiter::new();
        let (waiter_rw, wg_rw) = Waiter::new();
        let Encoding {
            encoder, decoder, ..
        } = encoding;
        let (compressor, extractor) = compression.converters();
        let (compressor_for_unreliable, extractor_for_unreliable) = compression.converters();

        let (tx_unreliable_write_message, rx_unreliable_write_message) =
            if unreliable_writer.is_some() {
                let (tx, rx) = mpsc::channel(channel_size);
                (Some(tx), Some(rx))
            } else {
                (None, None)
            };
        let (tx_unreliable_read_loop_command, rx_unreliable_read_loop_command) =
            if unreliable_reader.is_some() {
                let (tx, rx) = mpsc::unbounded_channel();
                (Some(tx), Some(rx))
            } else {
                (None, None)
            };

        let inner = Arc::new(ConnInner {
            tx_write_message,
            tx_unreliable_write_message,
            tx_read_loop_command,
            tx_unreliable_read_loop_command,
            rx_downstream_call,
            response_senders: Mutex::new(fnv::FnvHashMap::default()),
            request_id_counter: RequestIdCounter::new(),
            ct: CancellationToken::new(),
            waiter,
            waiter_rw,
            response_message_timeout: timeout,
            downstream_stream_id_alias_counter: AtomicCell::new(1),
            channel_size,
        });

        // Spawn unreliable reader task
        if let Some(mut unreliable_reader) = unreliable_reader {
            let d = decoder.clone();
            let inner_clone = inner.clone();
            let wg_rw_clone = wg_rw.clone();
            let tx_pong_clone = tx_pong.clone();
            let tx_downstream_call_clone = tx_downstream_call.clone();
            tokio::spawn(async move {
                read_loop::read_loop(
                    inner_clone,
                    &mut unreliable_reader,
                    rx_unreliable_read_loop_command.unwrap(),
                    tx_pong_clone,
                    tx_downstream_call_clone,
                    d,
                    extractor_for_unreliable,
                )
                .await;
                if let Err(e) = unreliable_reader.close().await {
                    log::error!("transport reader close error {}", e);
                }
                log::debug!("exit read loop");
                std::mem::drop(wg_rw_clone);
            });
        }

        // Spawn reader task
        let inner_clone = inner.clone();
        let wg_rw_clone = wg_rw.clone();
        let d = decoder.clone();
        tokio::spawn(async move {
            read_loop::read_loop(
                inner_clone,
                &mut reader,
                rx_read_loop_command,
                tx_pong,
                tx_downstream_call,
                d,
                extractor,
            )
            .await;
            if let Err(e) = reader.close().await {
                log::error!("transport reader close error {}", e);
            }
            log::debug!("exit read loop");
            std::mem::drop(wg_rw_clone);
        });

        // Spawn writer task
        let inner_clone = inner.clone();
        let e = encoder.clone();
        tokio::spawn(async move {
            write_loop(inner_clone, &mut writer, rx_write_message, e, compressor).await;
            if let Err(e) = writer.close().await {
                log::error!("transport writer close error {}", e);
            }
            log::debug!("exit write loop");
            std::mem::drop(wg_rw);
        });

        // Spawn closer task
        let inner_clone = inner.clone();
        let wg_clone = wg.clone();
        tokio::spawn(async move {
            close_task::<C>(inner_clone, closer).await;
            log::debug!("exit close task");
            std::mem::drop(wg_clone);
        });

        // Spawn unreliable writer task
        let e = encoder.clone();
        if let Some(mut unreliable_writer) = unreliable_writer {
            let inner_clone = inner.clone();
            let wg_clone = wg.clone();
            let rx_write_message = rx_unreliable_write_message.unwrap();
            tokio::spawn(async move {
                write_loop(
                    inner_clone,
                    &mut unreliable_writer,
                    rx_write_message,
                    e,
                    compressor_for_unreliable,
                )
                .await;
                log::debug!("unreliable write loop");
                std::mem::drop(wg_clone);
            });
        }

        // Spawn ping pong task
        let inner_clone = inner.clone();
        tokio::spawn(async move {
            ping_pong_loop(inner_clone, rx_pong, ping_interval, ping_timeout).await;
            log::debug!("exit ping pong loop");
            std::mem::drop(wg);
        });

        Conn { inner }
    }

    pub async fn close(&self) -> Result<(), Error> {
        if self.inner.ct.is_cancelled() {
            return Ok(());
        }
        self.inner.ct.cancel();
        self.inner.waiter.wait().await;
        Ok(())
    }

    pub(crate) fn cancel(&self) {
        self.inner.ct.cancel();
    }

    pub(crate) async fn send_message<T: Into<Message>>(&self, msg: T) -> Result<(), Error> {
        let result = timeout_with_ct(
            &self.inner.ct,
            self.inner.response_message_timeout,
            self.inner.tx_write_message.send(msg.into()),
        )
        .await?;
        if result.is_err() {
            return Err(Error::unexpected("write loop closed"));
        }
        Ok(())
    }

    pub(crate) async fn request_message_need_response<T: RequestMessage>(
        &self,
        mut request: T,
    ) -> Result<T::Response, Error> {
        let request_id = self.inner.request_id_counter.get();
        let (tx, rx) = oneshot::channel();
        self.inner.add_response_sender(request_id, tx);
        request.set_request_id(request_id);
        self.send_message(request.into()).await?;
        let result =
            timeout_with_ct(&self.inner.ct, self.inner.response_message_timeout, rx).await?;
        let Ok(response) = result else {
            return Err(Error::unexpected("internal channel closed"));
        };
        let Ok(response) = TryInto::<T::Response>::try_into(response) else {
            return Err(Error::invalid_value("unexpected message returned"));
        };

        let Some(result_code) = response.result_code() else {
            return Err(Error::invalid_value("unknown result code"));
        };
        if result_code != crate::message::ResultCode::Succeeded {
            return Err(Error::FailedMessage {
                result_code,
                detail: response.result_string().to_owned(),
            });
        }
        Ok(response)
    }

    pub(crate) async fn send_message_unreliable<T: Into<Message>>(
        &self,
        msg: T,
    ) -> Result<(), Error> {
        let msg = msg.into();
        if let Some(tx) = &self.inner.tx_unreliable_write_message {
            let result = timeout_with_ct(
                &self.inner.ct,
                self.inner.response_message_timeout,
                tx.send(msg),
            )
            .await?;
            if result.is_err() {
                return Err(Error::unexpected("unreliable write loop closed"));
            }
            Ok(())
        } else {
            self.send_message(msg).await
        }
    }

    pub(crate) async fn send_message_with_qos<T: Into<Message>>(
        &self,
        msg: T,
        qos: crate::message::QoS,
    ) -> Result<(), Error> {
        if qos == crate::message::QoS::Unreliable {
            self.send_message_unreliable(msg).await
        } else {
            self.send_message(msg).await
        }
    }

    pub(crate) async fn cancelled(&self) {
        self.inner.ct.cancelled().await
    }

    pub(crate) async fn add_upstream(
        &self,
        stream_id_alias: u32,
    ) -> Result<
        (
            mpsc::Receiver<crate::message::UpstreamChunkAck>,
            SendCommandGuard,
        ),
        Error,
    > {
        log::debug!("add upstream stream_id_alias = {}", stream_id_alias);

        let (tx, rx) = mpsc::channel(self.inner.channel_size);
        let command = ReadLoopCommand::AddUpstream {
            stream_id_alias,
            tx_upstream_chunk_ack: tx,
        };

        self.inner
            .tx_read_loop_command
            .send(command)
            .map_err(|_| Error::ConnectionClosed)?;

        let guard = SendCommandGuard::new(
            &self.inner.tx_read_loop_command,
            ReadLoopCommand::RemoveUpstream { stream_id_alias },
        );

        Ok((rx, guard))
    }

    pub(crate) async fn add_downstream(
        &self,
        stream_id_alias: u32,
        unreliable: bool,
    ) -> Result<
        (
            mpsc::Receiver<ReceivableDownstreamMsg>,
            (SendCommandGuard, Option<SendCommandGuard>),
        ),
        Error,
    > {
        log::debug!("add downstream stream_id_alias = {}", stream_id_alias);

        let (tx, rx) = mpsc::channel(self.inner.channel_size);

        let unreliable_guard = if unreliable {
            if let Some(tx_command) = &self.inner.tx_unreliable_read_loop_command {
                let command = ReadLoopCommand::AddDownstream {
                    stream_id_alias,
                    tx_downstream_msg: tx.clone(),
                };
                tx_command
                    .send(command)
                    .map_err(|_| Error::ConnectionClosed)?;
                Some(SendCommandGuard::new(
                    tx_command,
                    ReadLoopCommand::RemoveDownstream { stream_id_alias },
                ))
            } else {
                None
            }
        } else {
            None
        };

        let command = ReadLoopCommand::AddDownstream {
            stream_id_alias,
            tx_downstream_msg: tx,
        };
        self.inner
            .tx_read_loop_command
            .send(command)
            .map_err(|_| Error::ConnectionClosed)?;

        let guard = SendCommandGuard::new(
            &self.inner.tx_read_loop_command,
            ReadLoopCommand::RemoveDownstream { stream_id_alias },
        );

        Ok((rx, (guard, unreliable_guard)))
    }

    pub(crate) fn downstream_stream_id_alias(&self) -> Result<u32, Error> {
        let stream_id_alias = self.inner.downstream_stream_id_alias_counter.fetch_add(1);
        if stream_id_alias == 0 {
            return Err(Error::unexpected("stream id alias max reached"));
        }
        Ok(stream_id_alias)
    }

    pub(crate) async fn subscribe_call_ack(
        &self,
        call_id: String,
    ) -> Result<(oneshot::Receiver<UpstreamCallAck>, SendCommandGuard), Error> {
        let (tx, rx) = oneshot::channel();
        let command = ReadLoopCommand::SubscribeCallAck {
            call_id: call_id.clone(),
            tx,
        };

        self.inner
            .tx_read_loop_command
            .send(command)
            .map_err(|_| Error::ConnectionClosed)?;

        let guard = SendCommandGuard::new(
            &self.inner.tx_read_loop_command,
            ReadLoopCommand::RemoveCallAck { call_id },
        );

        Ok((rx, guard))
    }

    pub(crate) fn subscribe_downstream_call(&self) -> DownstreamCallReceiver {
        DownstreamCallReceiver(self.inner.rx_downstream_call.resubscribe())
    }

    pub fn is_connected(&self) -> bool {
        !self.inner.ct.is_cancelled()
    }
}

async fn write_loop<T: TransportWriter>(
    inner: Arc<ConnInner>,
    writer: &mut T,
    mut rx_write_message: mpsc::Receiver<Message>,
    mut encoder: Encoder,
    mut compressor: Compressor,
) {
    let _ct_guard = inner.ct.clone().drop_guard();
    let mut buf = BytesMut::new();

    loop {
        buf.clear();
        let msg = tokio::select! {
            msg = rx_write_message.recv() => {
                if let Some(msg) = msg {
                    msg
                } else {
                    return;
                }
            }
            _ = inner.ct.cancelled() => {
                break;
            }
        };
        log::trace!("write message: {:?}", msg);
        if let Err(e) = encoder.encode_to(&mut buf, &msg) {
            log::warn!("cannot encode message: {}", e);
            continue;
        }
        if let Err(e) = compressor.compress(&mut buf) {
            log::error!("message compression error: {}", e);
            return;
        }
        if let Err(e) = writer.write(&buf).await {
            log::error!("transport write error: {}", e);
            return;
        }
    }

    // Write all remaining messages
    while let Ok(msg) = rx_write_message.try_recv() {
        buf.clear();
        log::trace!("write message: {:?}", msg);
        if let Err(e) = encoder.encode_to(&mut buf, &msg) {
            log::warn!("cannot encode message: {}", e);
            continue;
        }
        if let Err(e) = compressor.compress(&mut buf) {
            log::error!("message compression error: {}", e);
            return;
        }
        if let Err(e) = writer.write(&buf).await {
            log::error!("transport write error: {}", e);
            return;
        }
    }
}

async fn close_task<T: Connector>(inner: Arc<ConnInner>, mut closer: T::Closer) {
    let _ = inner.ct.cancelled().await;
    // closer.close() is called after reader and writer close
    inner.waiter_rw.wait().await;
    if let Err(e) = closer.close().await {
        log::error!("transport close error: {}", e);
    }
}

async fn ping_pong_loop(
    inner: Arc<ConnInner>,
    mut rx_pong: mpsc::Receiver<crate::message::Pong>,
    ping_interval: Duration,
    ping_timeout: Duration,
) {
    let _ct_guard = inner.ct.clone().drop_guard();
    loop {
        cancelled_return!(inner.ct, tokio::time::sleep(ping_interval));

        let request_id = inner.request_id_counter.get();
        let ping = crate::message::Ping {
            request_id,
            ..Default::default()
        };
        if cancelled_return!(
            inner.ct,
            inner
                .tx_write_message
                .send_timeout(ping.into(), ping_timeout)
        )
        .is_err()
        {
            log::error!("ping sending timeout reached");
            return;
        }
        loop {
            let timeout = tokio::time::Instant::now() + ping_timeout;
            let pong = tokio::select! {
                pong = rx_pong.recv() => pong,
                _ = tokio::time::sleep_until(timeout) => {
                    log::error!("ping timeout reached");
                    return;
                }
                _ = inner.ct.cancelled() => {
                    return;
                }
            };
            let Some(pong) = pong else {
                return;
            };
            if pong.request_id >= request_id {
                break;
            }
        }
    }
}

impl ConnInner {
    fn add_response_sender(&self, request_id: u32, sender: oneshot::Sender<Message>) {
        let old = self
            .response_senders
            .lock()
            .expect("add_response_sender")
            .insert(request_id, sender);
        if old.is_some() {
            log::warn!("request_id duplication detected");
        }
    }

    fn remove_response_sender(&self, request_id: u32) -> Option<oneshot::Sender<Message>> {
        self.response_senders
            .lock()
            .expect("remove_response_sender")
            .remove(&request_id)
    }
}

struct RequestIdCounter(AtomicCell<u32>);

impl RequestIdCounter {
    fn new() -> Self {
        Self(AtomicCell::new(0))
    }

    fn get(&self) -> u32 {
        self.0.fetch_add(2)
    }
}
