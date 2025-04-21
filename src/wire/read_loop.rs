use std::collections::HashMap;

use fnv::FnvHashMap;
use tokio::sync::{broadcast, mpsc};

use super::*;
use crate::{
    encoding::Decoder,
    message::{
        message::Message as MessageEnum, DownstreamCall, DownstreamChunk,
        DownstreamChunkAckComplete, DownstreamMetadata, Message, UpstreamCallAck, UpstreamChunkAck,
    },
    transport::Extractor,
};

#[derive(Debug)]
pub enum ReceivableDownstreamMsg {
    Chunk(DownstreamChunk),
    Metadata(DownstreamMetadata),
    ChunkAckComplete(DownstreamChunkAckComplete),
}

pub(super) async fn read_loop<T: TransportReader>(
    inner: Arc<ConnInner>,
    reader: &mut T,
    mut rx_read_loop_command: mpsc::UnboundedReceiver<ReadLoopCommand>,
    tx_pong: mpsc::Sender<crate::message::Pong>,
    tx_downstream_call: broadcast::Sender<DownstreamCall>,
    mut decoder: Decoder,
    mut extractor: Extractor,
) {
    let _ct_guard = inner.ct.clone().drop_guard();
    let mut channels = ReadLoopChannels::default();
    let mut buf = BytesMut::new();
    loop {
        buf.clear();
        tokio::select! {
            command = rx_read_loop_command.recv() => {
                if let Some(command) = command {
                    channels.process_command(command);
                } else {
                    return;
                }
            }
            result = reader.read(&mut buf) => {
                if let Err(e) = result {
                    log::error!("transport read error: {}", e);
                    return;
                }
            }
            _ = inner.ct.cancelled() => {
                return;
            }
        }

        if let Err(e) = extractor.extract(&mut buf) {
            log::error!("message extraction error: {}", e);
            return;
        }
        let msg = match decoder.decode_from(&buf) {
            Ok(msg) => msg,
            Err(e) => {
                log::error!("cannot decode message from stream: {}", e);
                return;
            }
        };
        log::trace!("read message: {:?}", msg);

        let msg = match msg.message {
            Some(MessageEnum::Ping(ping)) => {
                let pong = crate::message::Pong {
                    request_id: ping.request_id(),
                    ..Default::default()
                };
                if inner.tx_write_message.send(pong.into()).await.is_err() {
                    return;
                }
                continue;
            }
            Some(MessageEnum::Pong(pong)) => {
                if tx_pong.send(pong).await.is_err() {
                    return;
                }
                continue;
            }
            Some(MessageEnum::Disconnect(disconnect)) => {
                log::info!(
                    "receive disconnect message, result_code = {:?}, {}",
                    disconnect.result_code(),
                    disconnect.result_string,
                );
                return;
            }
            Some(MessageEnum::DownstreamCall(call)) => {
                if tx_downstream_call.send(call).is_err() {
                    return;
                }
                continue;
            }
            _ => msg,
        };

        let Err(msg) = channels.process_message(msg).await else {
            continue;
        };

        if let Some(request_id) = msg.request_id() {
            if request_id % 2 == 0 {
                if let Some(sender) = inner.remove_response_sender(request_id) {
                    check_result!(debug, sender.send(msg), "response channel closed");
                    continue;
                }
            }
        }

        log::trace!("drop received message");
        std::mem::drop(msg);
    }
}

#[derive(Default)]
pub struct ReadLoopChannels {
    tx_upstream_chunk_ack: FnvHashMap<u32, mpsc::Sender<UpstreamChunkAck>>,
    tx_downstream_msg: FnvHashMap<u32, mpsc::Sender<ReceivableDownstreamMsg>>,
    tx_call_ack: HashMap<String, oneshot::Sender<UpstreamCallAck>>,
}

pub enum ReadLoopCommand {
    AddUpstream {
        stream_id_alias: u32,
        tx_upstream_chunk_ack: mpsc::Sender<UpstreamChunkAck>,
    },
    RemoveUpstream {
        stream_id_alias: u32,
    },
    AddDownstream {
        stream_id_alias: u32,
        tx_downstream_msg: mpsc::Sender<ReceivableDownstreamMsg>,
    },
    RemoveDownstream {
        stream_id_alias: u32,
    },
    SubscribeCallAck {
        call_id: String,
        tx: oneshot::Sender<UpstreamCallAck>,
    },
    RemoveCallAck {
        call_id: String,
    },
}

impl ReadLoopChannels {
    pub fn process_command(&mut self, command: ReadLoopCommand) {
        match command {
            ReadLoopCommand::AddUpstream {
                stream_id_alias,
                tx_upstream_chunk_ack,
            } => {
                if self
                    .tx_upstream_chunk_ack
                    .insert(stream_id_alias, tx_upstream_chunk_ack)
                    .is_some()
                {
                    log::warn!("stream id alias {} (up) may be duplicated", stream_id_alias);
                }
            }
            ReadLoopCommand::RemoveUpstream { stream_id_alias } => {
                if self
                    .tx_upstream_chunk_ack
                    .remove(&stream_id_alias)
                    .is_none()
                {
                    log::warn!("stream id alias {} (up) not registered", stream_id_alias);
                }
            }
            ReadLoopCommand::AddDownstream {
                stream_id_alias,
                tx_downstream_msg,
            } => {
                if self
                    .tx_downstream_msg
                    .insert(stream_id_alias, tx_downstream_msg)
                    .is_some()
                {
                    log::warn!(
                        "stream id alias {} (down) may be duplicated",
                        stream_id_alias
                    );
                }
            }
            ReadLoopCommand::RemoveDownstream { stream_id_alias } => {
                if self.tx_downstream_msg.remove(&stream_id_alias).is_none() {
                    log::warn!("stream id alias {} (down) not registered", stream_id_alias);
                }
            }
            ReadLoopCommand::SubscribeCallAck { call_id, tx } => {
                if self.tx_call_ack.insert(call_id, tx).is_some() {
                    log::warn!("call id duplication detected");
                }
            }
            ReadLoopCommand::RemoveCallAck { call_id } => {
                self.tx_call_ack.remove(&call_id);
            }
        }
    }

    pub async fn process_message(&mut self, msg: Message) -> Result<(), Message> {
        match msg.message {
            Some(MessageEnum::UpstreamChunkAck(ack)) => {
                if let Some(tx) = self.tx_upstream_chunk_ack.get(&ack.stream_id_alias) {
                    let _ = tx.send(ack).await;
                }
            }
            Some(MessageEnum::DownstreamChunk(chunk)) => {
                if let Some(tx) = self.tx_downstream_msg.get(&chunk.stream_id_alias) {
                    let _ = tx.send(ReceivableDownstreamMsg::Chunk(chunk)).await;
                }
            }
            Some(MessageEnum::DownstreamMetadata(metadata)) => {
                if let Some(tx) = self.tx_downstream_msg.get(&metadata.stream_id_alias) {
                    let _ = tx.send(ReceivableDownstreamMsg::Metadata(metadata)).await;
                }
            }
            Some(MessageEnum::DownstreamChunkAckComplete(complete)) => {
                if let Some(tx) = self.tx_downstream_msg.get(&complete.stream_id_alias) {
                    let _ = tx
                        .send(ReceivableDownstreamMsg::ChunkAckComplete(complete))
                        .await;
                }
            }
            Some(MessageEnum::UpstreamCallAck(ack)) => {
                if let Some(tx) = self.tx_call_ack.remove(&ack.call_id) {
                    let _ = tx.send(ack);
                }
            }
            _ => {
                return Err(msg);
            }
        }
        Ok(())
    }
}

pub(crate) struct SendCommandGuard {
    command: Option<ReadLoopCommand>,
    tx: mpsc::UnboundedSender<ReadLoopCommand>,
}

impl SendCommandGuard {
    pub fn new(tx: &mpsc::UnboundedSender<ReadLoopCommand>, command: ReadLoopCommand) -> Self {
        Self {
            command: Some(command),
            tx: tx.clone(),
        }
    }
}

impl Drop for SendCommandGuard {
    fn drop(&mut self) {
        let _ = self.tx.send(self.command.take().unwrap());
    }
}

pub(crate) struct DownstreamCallReceiver(pub(super) broadcast::Receiver<DownstreamCall>);

impl DownstreamCallReceiver {
    pub async fn recv(&mut self) -> Result<DownstreamCall, Error> {
        loop {
            match self.0.recv().await {
                Ok(msg) => {
                    return Ok(msg);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    return Err(Error::ConnectionClosed);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => (),
            }
        }
    }
}
