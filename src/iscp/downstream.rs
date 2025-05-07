use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use crossbeam::atomic::AtomicCell;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    down_order::*,
    down_state::*,
    misc::ReconnectWaiter,
    types::{DataPointGroup, DownstreamChunk, DownstreamMetadata},
    SharedWireConn,
};
use crate::{
    error::Error,
    internal::{timeout_with_ct, WaitGroup},
    message::{
        data_point_group::DataIdOrAlias, downstream_chunk::UpstreamOrAlias, DataId,
        DownstreamFilter, QoS, ResultCode,
    },
    wire::Conn as WireConn,
};

/// ダウンストリームの設定
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct DownstreamConfig {
    /// フィルタ
    pub filters: Vec<DownstreamFilter>,
    /// ストリーム再開の有効期限
    pub expiry_interval: Duration,
    /// ストリームのQoS
    pub qos: QoS,
    /// データIDエイリアスの設定に用いるデータIDのリスト
    pub data_ids: Vec<DataId>,
    /// ACKの返信間隔
    pub ack_interval: Duration,
    /// 空のチャンクを捨てるかの設定
    pub omit_empty_chunk: bool,
    /// Reliable用のチャンク並び替えの設定
    pub reordering: DownstreamReordering,
    /// 並び替えを待つチャンクの最大値
    pub reordering_chunks: usize,
    /// クローズのタイムアウト
    pub close_timeout: Duration,
}

/// Reliable用のチャンク並び替えの設定
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum DownstreamReordering {
    #[default]
    None,
    BestEffort,
    Strict,
}

impl Default for DownstreamConfig {
    fn default() -> Self {
        Self {
            filters: Vec::new(),
            expiry_interval: Duration::from_secs(60),
            qos: QoS::Unreliable,
            data_ids: Vec::new(),
            ack_interval: Duration::from_millis(100),
            omit_empty_chunk: false,
            reordering: DownstreamReordering::default(),
            reordering_chunks: 0xFF,
            close_timeout: Duration::from_secs(10),
        }
    }
}

/// iSCPのダウンストリーム
pub struct Downstream {
    inner: Arc<DownstreamInner>,
    rx_downstream_chunk: mpsc::Receiver<DownstreamChunk>,
}

/// メタデータの読み込みオブジェクト
pub struct DownstreamMetadataReader {
    inner: Arc<DownstreamInner>,
    rx: mpsc::Receiver<DownstreamMetadata>,
}

pub struct DownstreamInner {
    config: Arc<DownstreamConfig>,
    stream_id: Uuid,
    stream_id_alias: AtomicCell<u32>,
    state: State,
    tx_result: AtomicCell<Option<oneshot::Sender<Result<(), Error>>>>,
    server_time: SystemTime,
    close_cause: AtomicCell<Option<Error>>,
    ct: CancellationToken,
}

impl Downstream {
    pub(crate) async fn new(
        config: Arc<DownstreamConfig>,
        shared_wire_conn: SharedWireConn,
        wg: WaitGroup,
        channel_size: usize,
    ) -> Result<(Self, DownstreamMetadataReader, CancellationToken), Error> {
        let wire_conn = shared_wire_conn.get();
        let (response, stream_id_alias, data_id_aliases) =
            request_open(&wire_conn, &config).await?;

        let stream_id = super::misc::parse_stream_id(&response.assigned_stream_id)?;
        let server_time = super::misc::unix_epoch_to_system_time(response.server_time)?;

        let ct = CancellationToken::new();

        let (tx_downstream_chunk, rx_downstream_chunk) = mpsc::channel(channel_size);
        let (tx_metadata, rx_metadata) = mpsc::channel(channel_size);

        let inner = Arc::new(DownstreamInner {
            config,
            stream_id,
            stream_id_alias: AtomicCell::new(stream_id_alias),
            state: State::new(data_id_aliases),
            tx_result: AtomicCell::new(None),
            server_time,
            close_cause: AtomicCell::new(None),
            ct: ct.clone(),
        });

        let inner_clone = inner.clone();
        tokio::spawn(async move {
            if let Err(e) = downstream_loop(
                wire_conn,
                shared_wire_conn,
                inner_clone.clone(),
                tx_downstream_chunk,
                tx_metadata,
            )
            .await
            {
                inner_clone.close_cause.store(Some(e));
            }
            log::debug!("exit downstream loop");
            std::mem::drop(wg);
        });

        log::info!("opened downstream {}", stream_id);
        Ok((
            Self {
                inner: inner.clone(),
                rx_downstream_chunk,
            },
            DownstreamMetadataReader {
                inner,
                rx: rx_metadata,
            },
            ct,
        ))
    }

    /// ダウンストリームからチャンクを読み込む
    pub async fn read_chunk(&mut self) -> Result<DownstreamChunk, Error> {
        tokio::select! {
            result = self.rx_downstream_chunk.recv() => {
                return result.ok_or_else(|| self.inner.close_cause.take().unwrap_or(Error::StreamClosed));
            }
            _ = self.inner.ct.cancelled() => (),
        }

        self.rx_downstream_chunk
            .try_recv()
            .map_err(|_| self.inner.close_cause.take().unwrap_or(Error::StreamClosed))
    }

    /// このダウンストリームを閉じる
    pub async fn close(&mut self) -> Result<(), Error> {
        let (tx, rx) = oneshot::channel();
        self.inner.tx_result.store(Some(tx));
        self.inner.ct.cancel();
        tokio::time::timeout(self.inner.config.close_timeout, rx)
            .await
            .map_err(|_| Error::unexpected("close timeout"))?
            .map_err(|_| Error::unexpected("cannot get close result"))?
    }

    /// ストリームのIDを取得
    pub fn stream_id(&self) -> Uuid {
        self.inner.stream_id
    }

    /// ダウンストリームを開いた時刻を取得
    pub fn server_time(&self) -> SystemTime {
        self.inner.server_time
    }

    /// ダウンストリームの設定を取得
    pub fn config(&self) -> Arc<DownstreamConfig> {
        self.inner.config.clone()
    }

    /// ダウンストリームの状態を取得
    pub fn state(&self) -> DownstreamState {
        self.inner.state.state()
    }
}

impl DownstreamMetadataReader {
    pub async fn read(&mut self) -> Result<DownstreamMetadata, Error> {
        tokio::select! {
            result = self.rx.recv() => {
                return result.ok_or_else(|| Error::StreamClosed);
            }
            _ = self.inner.ct.cancelled() => (),
        }

        self.rx.try_recv().map_err(|_| Error::StreamClosed)
    }
}

#[allow(clippy::while_let_loop)]
async fn downstream_loop(
    mut wire_conn: WireConn,
    mut shared_wire_conn: SharedWireConn,
    inner: Arc<DownstreamInner>,
    tx_downstream_chunk: mpsc::Sender<DownstreamChunk>,
    tx_metadata: mpsc::Sender<DownstreamMetadata>,
) -> Result<(), Error> {
    let _ct_guard = inner.ct.clone().drop_guard();
    let mut need_resume = false;
    let mut ack_id_complete = 0;
    let ct = &inner.ct;
    let mut reorderer = DownReorderer::new(&inner.config);
    let omit_empty_chunk = inner.config.omit_empty_chunk && !reorderer.enabled();

    loop {
        let mut resume_retry_waiter = ReconnectWaiter::new();
        // Resume loop. timeout by expiry_interval
        let result = timeout_with_ct(ct, inner.config.expiry_interval, async {
            loop {
                if wire_conn.is_connected() {
                    if need_resume {
                        if inner.config.expiry_interval.is_zero() {
                            break None;
                        }
                        match request_resume(&wire_conn, inner.stream_id).await {
                            Ok(stream_id_alias) => {
                                inner.stream_id_alias.store(stream_id_alias);
                                log::info!("resume success downstream {}", inner.stream_id);
                                need_resume = false;
                            }
                            Err(e) => {
                                let result_code = e.result_code();
                                if result_code == Some(crate::message::ResultCode::StreamNotFound) {
                                    log::warn!("cancel resume by stream not found: {}", e);
                                    break None;
                                }
                                if e.can_retry() || result_code.is_some() {
                                    log::warn!("cannot resume and retry: {}", e);
                                    resume_retry_waiter.wait().await;
                                } else {
                                    log::warn!("cannot resume: {}", e);
                                    break None;
                                }
                            }
                        }
                    } else if let Ok(result) = wire_conn
                        .add_downstream(
                            inner.stream_id_alias.load(),
                            inner.config.qos == QoS::Unreliable,
                        )
                        .await
                    {
                        break Some(result);
                    }
                    continue;
                }
                wire_conn = if let Ok(wire_conn) = shared_wire_conn.get_updated().await {
                    need_resume = true;
                    wire_conn
                } else {
                    break None;
                };
            }
        })
        .await;

        let (mut rx_msg, _guard) = match result {
            Ok(Some(result)) => result,
            Ok(None) => {
                return Ok(());
            }
            Err(_) => {
                log::error!("resume timeout in downstream {}", inner.stream_id);
                return Ok(());
            }
        };

        // Read loop
        let mut ack_send = Instant::now() + inner.config.ack_interval;

        loop {
            let msg = tokio::select! {
                _ = ct.cancelled() => { break; },
                msg = rx_msg.recv() => {
                    if let Some(msg) = msg {
                        msg
                    } else {
                        // wire conn will be closed if message receiver is closed.
                        break;
                    }
                }
                _ = tokio::time::sleep_until(ack_send.into()) => {
                    ack_send = Instant::now() + inner.config.ack_interval;
                    if let Some(ack) = inner.state.take_ack(inner.stream_id_alias.load()) {
                        if let Err(e) = wire_conn.send_message(ack).await {
                            log::warn!("cannot send downstream ack: {}", e);
                            break;
                        }
                    }
                    continue;
                }
            };

            match msg {
                crate::wire::ReceivableDownstreamMsg::Chunk(chunk) => {
                    match convert_downstream_chunk(chunk, &inner, omit_empty_chunk) {
                        Ok(Some(chunk)) => {
                            for chunk in reorderer.iter(chunk)? {
                                if tx_downstream_chunk.send(chunk).await.is_err() {
                                    ct.cancel(); // If receiver is closed, close this downstream.
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("{}", e);
                        }
                        _ => (),
                    }
                }
                crate::wire::ReceivableDownstreamMsg::Metadata(metadata) => {
                    match convert_downstream_metadata(metadata) {
                        Ok((metadata, ack)) => {
                            if tx_metadata.send(metadata).await.is_err() {
                                ct.cancel(); // If receiver is closed, close this downstream.
                                break;
                            }
                            if let Err(e) = wire_conn.send_message(ack).await {
                                log::warn!("cannot send metadata ack: {}", e);
                                break;
                            }
                        }
                        Err(e) => {
                            log::error!("{}", e);
                        }
                    }
                }
                crate::wire::ReceivableDownstreamMsg::ChunkAckComplete(complete) => {
                    check_chunk_ack_complete(complete, &mut ack_id_complete);
                }
            }
        }

        // Process remaining data in channels
        while let Ok(msg) = rx_msg.try_recv() {
            match msg {
                crate::wire::ReceivableDownstreamMsg::Chunk(chunk) => {
                    match convert_downstream_chunk(chunk, &inner, omit_empty_chunk) {
                        Ok(Some(chunk)) => {
                            for chunk in reorderer.iter(chunk)? {
                                let _ = tx_downstream_chunk.try_send(chunk);
                            }
                        }
                        Err(e) => {
                            log::error!("{}", e);
                        }
                        _ => (),
                    }
                }
                crate::wire::ReceivableDownstreamMsg::Metadata(metadata) => {
                    match convert_downstream_metadata(metadata) {
                        Ok((metadata, ack)) => {
                            let _ = tx_metadata.try_send(metadata);
                            if wire_conn.is_connected() {
                                if let Err(e) = wire_conn.send_message(ack).await {
                                    log::warn!("cannot send metadata ack: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("{}", e);
                        }
                    }
                }
                crate::wire::ReceivableDownstreamMsg::ChunkAckComplete(complete) => {
                    check_chunk_ack_complete(complete, &mut ack_id_complete);
                }
            }
        }

        // Send ack
        if wire_conn.is_connected() {
            if let Some(ack) = inner.state.take_ack(inner.stream_id_alias.load()) {
                if let Err(e) = wire_conn.send_message(ack).await {
                    log::warn!("cannot send downstream ack: {}", e);
                }
            }
        }

        if inner.config.expiry_interval.is_zero() || ct.is_cancelled() {
            // Close this downstream
            if wire_conn.is_connected() {
                // Wait for ack complete
                let result = tokio::time::timeout(inner.config.close_timeout, async {
                    loop {
                        if let Some(last_issued) = inner.state.last_issued_chunk_ack_id() {
                            if last_issued <= ack_id_complete {
                                break;
                            }
                        } else {
                            break;
                        }

                        match rx_msg.recv().await {
                            Some(crate::wire::ReceivableDownstreamMsg::ChunkAckComplete(
                                complete,
                            )) => {
                                check_chunk_ack_complete(complete, &mut ack_id_complete);
                            }
                            None => break,
                            _ => (),
                        }
                    }
                })
                .await;
                if result.is_err() {
                    log::warn!("close timeout at downstream {}", inner.stream_id);
                }

                // Send close message
                let close_msg = crate::message::DownstreamCloseRequest {
                    stream_id: inner.stream_id.as_bytes().to_vec().into(),
                    ..Default::default()
                };
                let result =
                    if let Err(e) = wire_conn.request_message_need_response(close_msg).await {
                        log::warn!("cannot send downstream close message: {}", e);
                        Err(Error::ConnectionClosed)
                    } else {
                        Ok(())
                    };
                if let Some(tx_result) = inner.tx_result.take() {
                    let _ = tx_result.send(result);
                }
            }
            return Ok(());
        }

        log::info!("try to resume downstream: {}", inner.stream_id);
    }
}

async fn request_open(
    wire_conn: &WireConn,
    config: &DownstreamConfig,
) -> Result<
    (
        crate::message::DownstreamOpenResponse,
        u32,
        HashMap<u32, DataId>,
    ),
    Error,
> {
    let Ok(expiry_interval) = config.expiry_interval.as_secs().try_into() else {
        return Err(Error::invalid_value("expiry_interval overflow"));
    };
    let desired_stream_id_alias = wire_conn.downstream_stream_id_alias()?;

    let data_id_aliases: HashMap<_, _> = config
        .data_ids
        .iter()
        .enumerate()
        .map(|(i, data_id)| (i as u32 + 1, data_id.clone()))
        .collect();

    let request = crate::message::DownstreamOpenRequest {
        desired_stream_id_alias,
        downstream_filters: config.filters.clone(),
        expiry_interval,
        qos: config.qos.into(),
        omit_empty_chunk: config.omit_empty_chunk,
        data_id_aliases: data_id_aliases.clone(),
        ..Default::default()
    };

    log::debug!("downstream open request: {:?}", request);

    let response = wire_conn.request_message_need_response(request).await?;
    Ok((response, desired_stream_id_alias, data_id_aliases))
}

async fn request_resume(wire_conn: &WireConn, stream_id: Uuid) -> Result<u32, Error> {
    let desired_stream_id_alias = wire_conn.downstream_stream_id_alias()?;
    let request = crate::message::DownstreamResumeRequest {
        desired_stream_id_alias,
        stream_id: stream_id.as_bytes().to_vec().into(),
        ..Default::default()
    };

    let _response = wire_conn.request_message_need_response(request).await?;
    Ok(desired_stream_id_alias)
}

fn convert_downstream_chunk(
    chunk: crate::message::DownstreamChunk,
    inner: &DownstreamInner,
    omit_empty_chunk: bool,
) -> Result<Option<DownstreamChunk>, Error> {
    let Some(stream_chunk) = chunk.stream_chunk else {
        return Ok(None);
    };
    if omit_empty_chunk && stream_chunk.data_point_groups.is_empty() {
        return Ok(None);
    }
    let sequence_number = stream_chunk.sequence_number;

    let mut data_point_groups = Vec::new();
    for dpg in stream_chunk.data_point_groups.into_iter() {
        let data_id = match dpg.data_id_or_alias {
            Some(DataIdOrAlias::DataId(data_id)) => data_id,
            Some(DataIdOrAlias::DataIdAlias(alias)) => {
                if let Some(data_id) = inner.state.get_data_id(alias) {
                    data_id
                } else {
                    return Err(Error::invalid_value(format!("unknown data id {}", alias)));
                }
            }
            None => {
                return Err(Error::invalid_value("invalid data_id_or_alias"));
            }
        };
        data_point_groups.push(DataPointGroup {
            data_id,
            data_points: dpg.data_points,
        });
    }

    let upstream = match chunk.upstream_or_alias {
        Some(UpstreamOrAlias::UpstreamInfo(info)) => inner.state.add_upstream_info(info)?,
        Some(UpstreamOrAlias::UpstreamAlias(alias)) => inner.state.get_upstream_info(alias)?,
        None => {
            return Err(Error::invalid_value("invalid upstream_or_alias"));
        }
    };

    inner
        .state
        .add_sequence_number(upstream.stream_id, sequence_number);

    Ok(Some(DownstreamChunk {
        sequence_number,
        data_point_groups,
        upstream,
    }))
}

fn convert_downstream_metadata(
    metadata: crate::message::DownstreamMetadata,
) -> Result<(DownstreamMetadata, crate::message::DownstreamMetadataAck), Error> {
    let Some(m) = metadata.metadata else {
        return Err(Error::invalid_value("invalid metadata"));
    };
    let m = super::metadata::ReceivableMetadata::from_prost(m)?;
    let m = DownstreamMetadata {
        source_node_id: metadata.source_node_id,
        metadata: m,
    };
    let ack = crate::message::DownstreamMetadataAck {
        request_id: metadata.request_id,
        result_code: ResultCode::Succeeded.into(),
        result_string: "OK".into(),
        ..Default::default()
    };

    Ok((m, ack))
}

fn check_chunk_ack_complete(
    complete: crate::message::DownstreamChunkAckComplete,
    ack_id_complete: &mut u32,
) {
    if *ack_id_complete < complete.ack_id {
        *ack_id_complete = complete.ack_id;
    }
    if complete.result_code != ResultCode::Succeeded as i32 {
        log::error!(
            "failed downstream chunk ack complete: result_code = {}, {}",
            complete.result_code,
            complete.result_string
        );
    }
}

impl std::fmt::Debug for Downstream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Downstream")
            .field("stream_id", &self.inner.stream_id)
            .finish()
    }
}

impl std::fmt::Debug for DownstreamMetadataReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DownstreamMetadataReader")
            .field("stream_id", &self.inner.stream_id)
            .finish()
    }
}
