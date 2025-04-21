use std::{
    collections::HashMap,
    num::NonZeroU32,
    sync::{Arc, RwLock},
    time::Duration,
};

use crossbeam::atomic::AtomicCell;
use tokio::sync::{oneshot, Notify};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    flush_policy::FlushPolicy,
    misc::CallbackReturnValue,
    types::{UpstreamChunk, UpstreamChunkResult},
    DataPoint, DataPointGroup, SharedWireConn,
};
use super::{misc::ReconnectWaiter, storage::*};
use crate::{
    error::Error,
    internal::{may_send_err, timeout_with_ct, WaitGroup},
    message::{data_point_group::DataIdOrAlias, DataId, QoS},
    wire::Conn as WireConn,
};

/// Callback of upstream chunk flush.
pub type SendDataPointsCallback =
    Arc<dyn Fn(Uuid, UpstreamChunk) -> CallbackReturnValue + Send + Sync + 'static>;
/// Callback of upstream ack receive.
pub type ReceiveAckCallback =
    Arc<dyn Fn(Uuid, UpstreamChunkResult) -> CallbackReturnValue + Send + Sync + 'static>;
/// Callback of upstream resumed.
pub type UpstreamResumedCallback = Arc<
    dyn Fn(Uuid, Arc<UpstreamConfig>, UpstreamState) -> CallbackReturnValue + Send + Sync + 'static,
>;

/// Upstream configuration.
#[derive(Clone)]
#[non_exhaustive]
pub struct UpstreamConfig {
    /// Session id.
    pub session_id: String,
    /// Ack interval.
    pub ack_interval: Duration,
    /// Resume expiry interval.
    pub expiry_interval: Duration,
    /// Data ids for setting data id alias.
    pub data_ids: Vec<DataId>,
    /// Stream QoS.
    pub qos: QoS,
    /// Persist.
    pub persist: bool,
    /// Flush policy of this stream.
    pub flush_policy: FlushPolicy,
    /// Close timeout.
    pub close_timeout: Duration,
    /// Callback for sending data points.
    pub send_data_points_callback: Option<SendDataPointsCallback>,
    /// Callback for receiving ack.
    pub receive_ack_callback: Option<ReceiveAckCallback>,
    /// Callback for resume.
    pub resumed_callback: Option<UpstreamResumedCallback>,
}

impl Default for UpstreamConfig {
    fn default() -> Self {
        Self {
            session_id: String::new(),
            ack_interval: Duration::from_millis(100),
            expiry_interval: Duration::from_secs(60),
            data_ids: Vec::new(),
            qos: QoS::Unreliable,
            persist: false,
            flush_policy: FlushPolicy::default(),
            close_timeout: Duration::from_secs(10),
            send_data_points_callback: None,
            receive_ack_callback: None,
            resumed_callback: None,
        }
    }
}

/// An iSCP upstream.
pub struct Upstream {
    inner: Arc<UpstreamInner>,
}

pub struct UpstreamInner {
    config: Arc<UpstreamConfig>,
    stream_id: Uuid,
    stream_id_alias: AtomicCell<u32>,
    notify_flush: Notify,
    is_connected: AtomicCell<bool>,
    tx_result: AtomicCell<Option<oneshot::Sender<Result<(), Error>>>>,
    close_opts: AtomicCell<Option<UpstreamCloseOptions>>,
    state: State,
    ct: CancellationToken,
}

/// Upstream state.
#[derive(Clone, Debug)]
pub struct UpstreamState {
    pub data_id_aliases: HashMap<u32, DataId>,
    pub total_data_points: u64,
    pub last_issued_sequence_number: Option<NonZeroU32>,
    pub data_points_buffer: Vec<DataPointGroup>,
    pub buffer_payload_size: usize,
}

/// Options for closing an upstream.
#[derive(Clone, Default, Debug)]
pub struct UpstreamCloseOptions {
    pub close_session: bool,
}

impl Upstream {
    pub(crate) async fn new(
        config: Arc<UpstreamConfig>,
        shared_wire_conn: SharedWireConn,
        wg: WaitGroup,
    ) -> Result<(Self, CancellationToken), Error> {
        let wire_conn = shared_wire_conn.get();

        // let response = request_open(&wire_conn, &config).await?;
        // TODO: Remove workaround to detect disconnect
        static ERR_COUNTER: AtomicCell<usize> = AtomicCell::new(0);
        let result = request_open(&wire_conn, &config).await;
        if result.is_err() {
            ERR_COUNTER.fetch_add(1);
            if ERR_COUNTER.load() >= 4 {
                log::error!("upstream request failed, explicit disconnect");
                wire_conn.cancel();
                ERR_COUNTER.store(0);
            }
        } else {
            ERR_COUNTER.store(0);
        }
        let response = result?;

        let stream_id = super::misc::parse_stream_id(&response.assigned_stream_id)?;

        let ct = CancellationToken::new();

        let inner = Arc::new(UpstreamInner {
            config,
            stream_id,
            stream_id_alias: AtomicCell::new(response.assigned_stream_id_alias),
            state: State::new(response.data_id_aliases),
            notify_flush: Notify::new(),
            is_connected: AtomicCell::new(true),
            tx_result: AtomicCell::new(None),
            close_opts: AtomicCell::default(),
            ct: ct.clone(),
        });

        let inner_clone = inner.clone();
        tokio::spawn(async move {
            let inner = inner_clone.clone();
            let mut flusher = UpstreamFlusher::new(&inner_clone);
            upstream_loop(wire_conn, shared_wire_conn, &mut flusher, inner).await;
            flusher.process_remaining_data_points().await;
            log::debug!("exit upstream loop");
            std::mem::drop(wg);
        });

        log::info!("opened upstream {}", stream_id);
        Ok((Self { inner }, ct))
    }

    /// Write data points.
    pub async fn write_data_points<T: IntoIterator<Item = DataPoint>>(
        &mut self,
        data_id: DataId,
        dps: T,
    ) -> Result<(), Error> {
        let mut add_size = 0;
        let dpg = DataPointGroup {
            data_id,
            data_points: dps
                .into_iter()
                .inspect(|dp| add_size += dp.payload.len())
                .collect(),
        };

        let current_size = {
            let mut data_points_buffer = self.inner.state.data_points_buffer.write().unwrap();
            data_points_buffer.push(dpg);
            self.inner.state.buffer_payload_size.fetch_add(add_size);
            self.inner.state.buffer_payload_size.load()
        };

        if self.inner.ct.is_cancelled() {
            return Err(Error::StreamClosed);
        }

        // Check flush is needed
        if self.inner.config.flush_policy.need_flush(current_size) {
            self.inner.notify_flush.notify_one();
        }

        Ok(())
    }

    /// Flush buffered data points to the upstream.
    pub async fn flush(&mut self) -> Result<(), Error> {
        if self.inner.ct.is_cancelled() {
            return Err(Error::StreamClosed);
        }
        let (tx, rx) = oneshot::channel();
        self.inner.tx_result.store(Some(tx));
        self.inner.notify_flush.notify_one();
        rx.await.map_err(|_| {
            Error::unexpected("unexpected upstream close when waiting flush complete")
        })?
    }

    /// Close this upstream.
    pub async fn close<T: Into<Option<UpstreamCloseOptions>>>(
        &mut self,
        opts: T,
    ) -> Result<(), Error> {
        self.inner.close_opts.store(opts.into());
        if self.inner.ct.is_cancelled() {
            return Ok(());
        }
        let (tx, rx) = oneshot::channel();
        self.inner.tx_result.store(Some(tx));
        self.inner.ct.cancel();
        tokio::time::timeout(self.inner.config.close_timeout, rx)
            .await
            .map_err(|_| Error::unexpected("close timeout"))?
            .map_err(|_| Error::unexpected("cannot get close result"))?
    }

    /// Get the configuration of this upstream.
    pub fn config(&self) -> Arc<UpstreamConfig> {
        self.inner.config.clone()
    }

    /// Get the stream id.
    pub fn stream_id(&self) -> Uuid {
        self.inner.stream_id
    }

    /// Get the current upstream state.
    pub fn state(&self) -> UpstreamState {
        self.inner.upstream_state()
    }

    /// This upstream is connected or not.
    pub fn is_connected(&self) -> bool {
        self.inner.is_connected.load()
    }
}

async fn upstream_loop(
    mut wire_conn: WireConn,
    shared_wire_conn: SharedWireConn,
    flusher: &mut UpstreamFlusher<'_>,
    inner: Arc<UpstreamInner>,
) {
    let ct = &inner.ct;
    let _ct_guard = inner.ct.clone().drop_guard();
    let need_resume = Arc::new(AtomicCell::new(false));

    'connection_loop: loop {
        // Spawn Resume task
        let mut jh = tokio::spawn(get_conn_and_resume(
            wire_conn.clone(),
            shared_wire_conn.clone(),
            inner.clone(),
            need_resume.clone(),
        ));

        let result = loop {
            tokio::select! {
                result = (&mut jh) => {
                    break result.expect("resume task panic");
                }
                _ = inner.notify_flush.notified() => {
                    if may_send_err(
                        &inner.tx_result,
                        "flush failure",
                        flusher.flush(None).await,
                    ) {
                        break 'connection_loop;
                    }
                },
                _ = inner.config.flush_policy.sleep_by_interval() => {
                    if may_send_err(
                        &inner.tx_result,
                        "flush failure",
                        flusher.flush(None).await,
                    ) {
                        break 'connection_loop;
                    }
                }
            }
        };
        let (mut rx_ack, _guard) = match result {
            Ok(Some((result, new_wire_conn))) => {
                wire_conn = new_wire_conn;
                result
            }
            Ok(None) => {
                return;
            }
            Err(_) => {
                log::error!("resume timeout in upstream {}", inner.stream_id);
                return;
            }
        };
        inner.is_connected.store(true);

        // Resend in reliable
        if let Err(e) = flusher.after_resume(&wire_conn).await {
            log::warn!("cannot resend: {}", e);
            inner.is_connected.store(false);
            if inner.config.expiry_interval.is_zero() || ct.is_cancelled() {
                return;
            }
            log::info!("try to resume upstream: {}", inner.stream_id);
            continue;
        }

        // Flush loop
        loop {
            let close = tokio::select! {
                _ = inner.notify_flush.notified() => false,
                _ = inner.config.flush_policy.sleep_by_interval() => false,
                _ = ct.cancelled() => true,
                ack = rx_ack.recv() => {
                    if let Some(ack) = ack {
                        flusher.process_ack(ack).await;
                        continue;
                    } else {
                        // wire conn will be closed if ack sender is closed.
                        break;
                    }
                }
            };

            // Flush and send the result by registered channel
            if may_send_err(
                &inner.tx_result,
                "flush failure",
                flusher.flush(Some(&wire_conn)).await,
            ) {
                break;
            }

            let max_seq_number = inner.state.last_issued_sequence_number.load() == u32::MAX;
            if close || max_seq_number {
                if max_seq_number {
                    log::error!("reached maximum sequence number");
                }
                // Wait for the last ack is received
                let waiting_sequence_number = inner.state.last_issued_sequence_number.load();
                if waiting_sequence_number > 0 {
                    let result = tokio::time::timeout(inner.config.close_timeout, async {
                        loop {
                            if flusher.storage.last_received_sequence_number()
                                == Some(waiting_sequence_number)
                            {
                                break;
                            }
                            if let Some(ack) = rx_ack.recv().await {
                                flusher.process_ack(ack).await;
                            } else {
                                break;
                            }
                        }
                    })
                    .await;
                    if result.is_err() {
                        log::warn!("close timeout at upstream {}", inner.stream_id);
                    }
                }

                // Send close request
                let extension_fields = inner.close_opts.take().map(|opts| {
                    crate::message::extensions::UpstreamCloseRequestExtensionFields {
                        close_session: opts.close_session,
                    }
                });
                let close_msg = crate::message::UpstreamCloseRequest {
                    stream_id: inner.stream_id.as_bytes().to_vec().into(),
                    total_data_points: inner.state.total_data_points.load(),
                    final_sequence_number: inner.state.last_issued_sequence_number.load(),
                    extension_fields,
                    ..Default::default()
                };
                may_send_err(
                    &inner.tx_result,
                    "close",
                    wire_conn
                        .request_message_need_response(close_msg)
                        .await
                        .map(|_| ()),
                );
                return;
            }
        }

        inner.is_connected.store(false);
        if inner.config.expiry_interval.is_zero() || ct.is_cancelled() {
            return;
        }
        log::info!("try to resume upstream: {}", inner.stream_id);
        need_resume.store(true);
    }
}

struct UpstreamFlusher<'a> {
    inner: &'a UpstreamInner,
    rev_data_id_aliases: HashMap<DataId, u32>,
    data_size: u64,
    storage: UpstreamStorage,
}

impl<'a> UpstreamFlusher<'a> {
    fn new(inner: &'a UpstreamInner) -> Self {
        let rev_data_id_aliases = inner
            .state
            .data_id_aliases
            .read()
            .unwrap()
            .iter()
            .map(|(k, v)| (v.clone(), *k))
            .collect();
        Self {
            inner,
            rev_data_id_aliases,
            data_size: 0,
            storage: UpstreamStorage::new(
                inner.stream_id,
                inner.config.qos,
                inner.config.send_data_points_callback.clone(),
                inner.config.receive_ack_callback.clone(),
            ),
        }
    }

    async fn flush(&mut self, wire_conn: Option<&WireConn>) -> Result<(), Error> {
        if let Some((chunk, dpg)) = self.take_chunk() {
            log::trace!("flush in upstream {}", self.inner.stream_id);
            self.storage.add_chunk(&chunk, dpg).await;
            if let Some(wire_conn) = wire_conn {
                wire_conn
                    .send_message_with_qos(chunk, self.inner.config.qos)
                    .await
            } else {
                Ok(())
            }
        } else {
            log::trace!("no data to flush in upstream {}", self.inner.stream_id);
            Ok(())
        }
    }

    // Returns chunk and Vec<DataPointGroup> for callback.
    fn take_chunk(&mut self) -> Option<(crate::message::UpstreamChunk, Vec<DataPointGroup>)> {
        let dpgs = {
            let mut data_points_buffer = self.inner.state.data_points_buffer.write().unwrap();
            self.inner.state.buffer_payload_size.store(0);
            std::mem::take(&mut *data_points_buffer)
        };
        if dpgs.is_empty() {
            return None;
        }

        // Collect datapoints for callback
        let dpg_callback = if self.inner.config.send_data_points_callback.is_some() {
            let mut data_point_vecs: HashMap<DataId, Vec<DataPoint>> = HashMap::new();
            for dpg in dpgs.iter() {
                if let Some(dps) = data_point_vecs.get_mut(&dpg.data_id) {
                    dps.append(&mut dpg.data_points.clone());
                } else {
                    data_point_vecs.insert(dpg.data_id.clone(), dpg.data_points.clone());
                }
            }

            data_point_vecs
                .into_iter()
                .map(|(data_id, mut data_points)| {
                    data_points.sort_by_key(|dp| dp.elapsed_time);
                    DataPointGroup {
                        data_id,
                        data_points,
                    }
                })
                .collect()
        } else {
            Vec::new()
        };

        // Assign alias and reorder datapoints
        let mut data_point_vecs: HashMap<DataIdOrAlias, Vec<DataPoint>> = HashMap::new();
        let mut additional_data_points = 0;

        for mut dpg in dpgs.into_iter() {
            additional_data_points += dpg.data_points.len();
            for dp in &dpg.data_points {
                self.data_size += dp.payload.len() as u64;
            }

            let data_id_or_alias = if let Some(alias) = self.rev_data_id_aliases.get(&dpg.data_id) {
                DataIdOrAlias::DataIdAlias(*alias)
            } else {
                DataIdOrAlias::DataId(dpg.data_id)
            };
            if let Some(dps) = data_point_vecs.get_mut(&data_id_or_alias) {
                dps.append(&mut dpg.data_points);
            } else {
                data_point_vecs.insert(data_id_or_alias, dpg.data_points);
            }
        }

        let data_point_groups: Vec<_> = data_point_vecs
            .into_iter()
            .map(|(data_id_or_alias, mut data_points)| {
                data_points.sort_by_key(|dp| dp.elapsed_time);
                crate::message::DataPointGroup {
                    data_id_or_alias: Some(data_id_or_alias),
                    data_points,
                }
            })
            .collect();

        let sequence_number = self.inner.state.last_issued_sequence_number.fetch_add(1) + 1;

        let chunk = crate::message::UpstreamChunk {
            stream_id_alias: self.inner.stream_id_alias.load(),
            stream_chunk: Some(crate::message::StreamChunk {
                data_point_groups,
                sequence_number,
            }),
            data_ids: vec![],
            ..Default::default()
        };

        self.inner
            .state
            .total_data_points
            .fetch_add(additional_data_points as u64);

        Some((chunk, dpg_callback))
    }

    async fn process_ack(&mut self, ack: crate::message::UpstreamChunkAck) {
        self.storage.process_ack(ack).await;
    }

    async fn after_resume(&mut self, wire_conn: &WireConn) -> Result<(), Error> {
        // Callback
        if let Some(callback) = &self.inner.config.resumed_callback {
            callback(
                self.inner.stream_id,
                self.inner.config.clone(),
                self.inner.upstream_state(),
            )
            .await;
        }

        if self.inner.config.qos == QoS::Reliable {
            // Resend
            for chunk in self.storage.iter_chunks() {
                wire_conn
                    .send_message_with_qos(chunk, self.inner.config.qos)
                    .await?;
            }
        } else {
            // Take chunk from buffer
            if let Some((chunk, dpg)) = self.take_chunk() {
                // call send_data_points_callback
                self.storage.add_chunk(&chunk, dpg).await;
            }
        }
        Ok(())
    }

    async fn process_remaining_data_points(&mut self) {
        if let Some((chunk, dpg)) = self.take_chunk() {
            // call send_data_points_callback
            self.storage.add_chunk(&chunk, dpg).await;
        }
    }
}

impl UpstreamInner {
    fn upstream_state(&self) -> UpstreamState {
        UpstreamState {
            data_id_aliases: self.state.data_id_aliases.read().unwrap().clone(),
            total_data_points: self.state.total_data_points.load(),
            last_issued_sequence_number: NonZeroU32::new(
                self.state.last_issued_sequence_number.load(),
            ),
            data_points_buffer: self.state.data_points_buffer.read().unwrap().clone(),
            buffer_payload_size: self.state.buffer_payload_size.load(),
        }
    }
}

struct State {
    data_id_aliases: RwLock<HashMap<u32, DataId>>,
    total_data_points: AtomicCell<u64>,
    last_issued_sequence_number: AtomicCell<u32>,
    data_points_buffer: RwLock<Vec<DataPointGroup>>,
    buffer_payload_size: AtomicCell<usize>,
}

impl State {
    fn new(data_id_aliases: HashMap<u32, DataId>) -> Self {
        Self {
            data_id_aliases: RwLock::new(data_id_aliases),
            total_data_points: AtomicCell::new(0),
            last_issued_sequence_number: AtomicCell::new(0),
            data_points_buffer: RwLock::default(),
            buffer_payload_size: AtomicCell::new(0),
        }
    }
}

async fn request_open(
    wire_conn: &WireConn,
    config: &UpstreamConfig,
) -> Result<crate::message::UpstreamOpenResponse, Error> {
    let Ok(ack_interval) = config.ack_interval.as_millis().try_into() else {
        return Err(Error::invalid_value("ack_interval overflow"));
    };
    let Ok(expiry_interval) = config.expiry_interval.as_secs().try_into() else {
        return Err(Error::invalid_value("expiry_interval overflow"));
    };

    let request = crate::message::UpstreamOpenRequest {
        session_id: config.session_id.clone(),
        ack_interval,
        expiry_interval,
        data_ids: config.data_ids.clone(),
        qos: config.qos.into(),
        extension_fields: Some(
            crate::message::extensions::UpstreamOpenRequestExtensionFields {
                persist: config.persist,
            },
        ),
        ..Default::default()
    };

    log::debug!("upstream open request: {:?}", request);

    static FAIL_TEST: std::sync::LazyLock<bool> =
        std::sync::LazyLock::new(|| std::env::var("ISCP_OPEN_UPSTREAM_FAIL_TEST").is_ok());
    if *FAIL_TEST {
        // Workaround test
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        Err(Error::Timeout("".into()))
    } else {
        let response = wire_conn.request_message_need_response(request).await?;
        Ok(response)
    }
    // let response = wire_conn.request_message_need_response(request).await?;
    // Ok(response)
}

async fn get_conn_and_resume(
    mut wire_conn: WireConn,
    mut shared_wire_conn: SharedWireConn,
    inner: Arc<UpstreamInner>,
    need_resume: Arc<AtomicCell<bool>>,
) -> Result<
    Option<(
        (
            tokio::sync::mpsc::Receiver<crate::message::UpstreamChunkAck>,
            crate::wire::SendCommandGuard,
        ),
        WireConn,
    )>,
    Error,
> {
    let mut resume_retry_waiter = ReconnectWaiter::new();
    let result = timeout_with_ct(&inner.ct, inner.config.expiry_interval, async {
        loop {
            if wire_conn.is_connected() {
                if need_resume.load() {
                    if inner.config.expiry_interval.is_zero() {
                        break None;
                    }
                    match request_resume(&wire_conn, inner.stream_id).await {
                        Ok(stream_id_alias) => {
                            inner.stream_id_alias.store(stream_id_alias);
                            log::info!("resume success upstream {}", inner.stream_id);
                            need_resume.store(false);
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
                } else if let Ok(result) =
                    wire_conn.add_upstream(inner.stream_id_alias.load()).await
                {
                    break Some(result);
                }
                continue;
            }
            wire_conn = if let Ok(wire_conn) = shared_wire_conn.get_updated().await {
                need_resume.store(true);
                wire_conn
            } else {
                break None;
            };
        }
    })
    .await;
    result.map(|result| result.map(|result| (result, wire_conn)))
}

async fn request_resume(wire_conn: &WireConn, stream_id: Uuid) -> Result<u32, Error> {
    let request = crate::message::UpstreamResumeRequest {
        stream_id: stream_id.as_bytes().to_vec().into(),
        ..Default::default()
    };

    let response = wire_conn.request_message_need_response(request).await?;
    Ok(response.assigned_stream_id_alias)
}

impl std::fmt::Debug for Upstream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Upstream")
            .field("stream_id", &self.inner.stream_id)
            .finish()
    }
}
