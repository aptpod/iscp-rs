//! アップストリームに関するモジュールです。

use std::{
    collections::HashMap,
    fmt,
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        Arc, Mutex, RwLock,
    },
    time::Duration,
};

use chrono::{DateTime, Utc};
use crossbeam::atomic::AtomicCell;
use log::*;
use tokio::sync::{broadcast, mpsc, watch};
use uuid::Uuid;

use crate::{
    msg, wire, Cancel, Conn, DataId, DataPointGroup, Error, FlushPolicy, IdAliasMap, QoS, Result,
    UpstreamChunk, UpstreamChunkResult, WaitGroup, Waiter,
};

/// Ack受信時のコールバックです。
/// コールバックの処理時間は短くしてください。処理時間が長くなると他の通信が遅延することがあります。
pub type ReceiveAckCallback = Arc<dyn Fn(Uuid, UpstreamChunkResult) + Send + Sync + 'static>;

/// データ送信時のコールバックです。
/// コールバックの処理時間は短くしてください。処理時間が長くなると他の通信が遅延することがあります。
pub type SendDataPointsCallback = Arc<dyn Fn(Uuid, UpstreamChunk) + Send + Sync + 'static>;

/// アップストリームの設定です。
#[derive(Clone)]
pub struct UpstreamConfig {
    /// セッションID
    pub session_id: String,
    /// Ackの返却間隔
    pub ack_interval: chrono::Duration,
    /// 有効期限
    pub expiry_interval: chrono::Duration,
    /// データIDリスト
    pub data_ids: Vec<DataId>,
    /// QoS
    pub qos: QoS,
    /// 永続化
    pub persist: bool,
    /// フラッシュポリシー
    pub flush_policy: FlushPolicy,
    /// Close時のタイムアウト
    pub close_timeout: Option<Duration>,
    /// Ack受信時のコールバック
    pub recv_ack_callback: Option<ReceiveAckCallback>,
    /// データ送信時のコールバック
    pub send_data_points_callback: Option<SendDataPointsCallback>,
    /// データ送信時のコールバックのキューサイズ
    pub callback_queue_size: usize,
}

impl Default for UpstreamConfig {
    fn default() -> Self {
        Self {
            session_id: String::new(),
            ack_interval: chrono::Duration::milliseconds(100),
            expiry_interval: chrono::Duration::zero(),
            data_ids: Vec::new(),
            qos: QoS::Unreliable,
            persist: false,
            flush_policy: FlushPolicy::default(),
            close_timeout: None,
            recv_ack_callback: None,
            send_data_points_callback: None,
            callback_queue_size: 10000,
        }
    }
}

impl msg::UpstreamOpenRequest {
    pub(crate) fn from_config(config: &UpstreamConfig) -> Self {
        Self {
            session_id: config.session_id.clone(),
            ack_interval: config.ack_interval,
            expiry_interval: config.expiry_interval,
            qos: config.qos,
            data_ids: config.data_ids.clone(),
            persist: Some(config.persist),
            ..Default::default()
        }
    }
}

/// アップストリームです。
#[derive(Clone)]
pub struct Upstream {
    conn: Arc<dyn wire::Connection>,
    cancel: Cancel,
    state: Arc<State>,
    write_state: Arc<WriteState>,
    repository: Arc<dyn super::UpstreamRepository>,
    close_notify: broadcast::Sender<UpstreamClosedEvent>,
    final_ack_received_notify: broadcast::Sender<()>,
    callback_channels: Arc<CallbackChannels>,
    config: Arc<UpstreamConfig>,
}

impl fmt::Debug for Upstream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Upstream")
            .field("stream_id", &self.state.stream_id)
            .field("alias", &self.state.stream_id_alias)
            .field(
                "current_serial",
                &self.write_state.latest_used_sequence_number(),
            )
            .field(
                "sent_data_point_count",
                &self.write_state.data_point_count(),
            )
            .finish()
    }
}

/// アップストリームの状態です。
#[derive(Clone, Debug)]
pub struct UpstreamState {
    /// データIDとエイリアスのマップ
    pub data_id_aliases: HashMap<u32, DataId>,
    /// 総送信データポイント数
    pub total_data_points: u64,
    /// 最後に払い出されたシーケンス番号
    pub last_issued_sequence_number: u32,
    /// 内部に保存しているデータポイントバッファ
    pub data_points_buffer: Vec<DataPointGroup>,
}

/// アップストリームのクローズイベントです。
#[derive(Clone)]
pub struct UpstreamClosedEvent {
    pub session_id: String,
    pub config: Arc<UpstreamConfig>,
    pub state: UpstreamState,
    pub caused_by: Option<Error>,
    pub error: Option<Error>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ConnectionState {
    Open,
    Closing,
    Closed,
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self::Open
    }
}

/// 内部的なアップストリームの状態
#[derive(Default, Debug)]
struct State {
    /// ストリームID
    stream_id: Uuid,
    stream_id_alias: u32,
    // データIDとエイリアスのマップ
    id_alias_map: Mutex<IdAliasMap>,
    conn: AtomicCell<ConnectionState>,

    /// UpstreamOpenResponseで返却されたサーバー時刻
    server_time: DateTime<Utc>,

    close_caused_by: Mutex<Option<Error>>,
    error_in_flush: Mutex<Option<Error>>,
}

impl State {
    fn check_open(&self) -> Result<()> {
        if !(ConnectionState::Open == self.conn.load()) {
            return Err(Error::ConnectionClosed("".into()));
        }
        Ok(())
    }
    fn check_not_closed(&self) -> Result<()> {
        if ConnectionState::Closed == self.conn.load() {
            return Err(Error::ConnectionClosed("".into()));
        }
        Ok(())
    }
    fn is_open(&self) -> bool {
        ConnectionState::Open == self.conn.load()
    }
    fn _is_closed(&self) -> bool {
        ConnectionState::Closed == self.conn.load()
    }
}

#[derive(Debug)]
struct WriteState {
    flush_policy: FlushPolicy,
    sequence_number: AtomicU32,
    data_point_count: AtomicU64,
    buf: Mutex<Vec<DataPointGroup>>,
    buffered_payload_len: AtomicU64,
    buf_on_err: Mutex<Vec<DataPointGroup>>,
    sent_storage: Arc<dyn super::SentStorage>,
}

impl Default for WriteState {
    fn default() -> Self {
        Self {
            flush_policy: FlushPolicy::default(),
            sequence_number: AtomicU32::new(1),
            data_point_count: AtomicU64::new(0),
            buf: Mutex::new(Vec::with_capacity(1024)),
            buffered_payload_len: AtomicU64::new(0),
            buf_on_err: Mutex::new(Vec::with_capacity(1024)),
            sent_storage: Arc::new(super::InMemSentStorageNoPayload::default()),
        }
    }
}

impl WriteState {
    fn push_data_points(&self, dpg: DataPointGroup) -> Result<()> {
        let mut buf = self.buf.lock().unwrap();
        if let Err(e) = self.check_sequence_number_overflow() {
            self.buf_on_err.lock().unwrap().push(dpg);
            return Err(e);
        }

        for dp in &dpg.data_points {
            self.buffered_payload_len
                .fetch_add(dp.payload.len() as u64, Ordering::Relaxed);
        }

        if let Err(e) = self.add_data_point_count(dpg.data_points.len() as u64) {
            self.buf_on_err.lock().unwrap().push(dpg);
            return Err(e);
        }
        buf.push(dpg);
        Ok(())
    }

    fn next_sequence_number(&self) -> u32 {
        self.sequence_number.fetch_add(1, Ordering::Release)
    }

    fn add_data_point_count(&self, count: u64) -> Result<u64> {
        let old = self.data_point_count.fetch_add(count, Ordering::Release);
        if old == u64::MAX {
            return Err(Error::MaxDataPointCount);
        }
        Ok(old + 1)
    }

    fn latest_used_sequence_number(&self) -> u32 {
        let s = self.sequence_number.load(Ordering::Acquire);
        if s == 0 {
            0
        } else {
            s - 1
        }
    }

    fn check_sequence_number_overflow(&self) -> Result<()> {
        if u32::MAX == self.sequence_number.load(Ordering::Acquire) {
            return Err(Error::MaxSequenceNumber);
        }
        Ok(())
    }

    fn data_point_count(&self) -> u64 {
        self.data_point_count.load(Ordering::Acquire)
    }

    fn buffered_payload_len(&self) -> u64 {
        self.buffered_payload_len.load(Ordering::Acquire)
    }

    fn take_buffered_data_points(&self) -> Vec<DataPointGroup> {
        let mut buf = self.buf.lock().unwrap();
        self.buffered_payload_len.store(0, Ordering::Release);

        std::mem::take(buf.as_mut())
    }
}

#[derive(Default)]
struct CallbackChannels {
    tx_recv_ack_callback: Option<RwLock<Option<mpsc::Sender<UpstreamChunkResult>>>>,
    tx_send_data_points_callback: Option<RwLock<Option<mpsc::Sender<UpstreamChunk>>>>,
    rx_recv_ack_callback_closed: Option<watch::Receiver<bool>>,
    rx_send_data_points_callback_closed: Option<watch::Receiver<bool>>,
}

pub(super) struct UpstreamParam {
    pub(super) stream_id: Uuid,
    pub(super) stream_id_alias: u32,
    pub(super) sent_strage: Arc<dyn super::SentStorage>,
    pub(super) repository: Arc<dyn super::UpstreamRepository>,
    pub(super) server_time: DateTime<Utc>,
}

impl Upstream {
    pub(super) fn new(
        conn: Arc<dyn wire::Connection>,
        config: Arc<UpstreamConfig>,
        param: UpstreamParam,
    ) -> Result<Self> {
        // validate
        if param.stream_id_alias == 0 {
            return Err(Error::invalid_value("stream_id_alias must be not zero"));
        }

        let (close_notify, _) = broadcast::channel(1);
        let (final_ack_received_notify, _) = broadcast::channel(1);
        let callback_channels = CallbackChannels::new(
            config.recv_ack_callback.clone(),
            config.send_data_points_callback.clone(),
            param.stream_id,
            config.callback_queue_size,
        );

        let up = Self {
            cancel: Cancel::new(),
            close_notify,
            final_ack_received_notify,
            callback_channels: Arc::new(callback_channels),
            conn,
            repository: param.repository,
            state: Arc::new(State {
                stream_id: param.stream_id,
                stream_id_alias: param.stream_id_alias,
                id_alias_map: Mutex::new(IdAliasMap::new()),
                conn: AtomicCell::new(ConnectionState::Open),
                server_time: param.server_time,
                close_caused_by: Mutex::default(),
                error_in_flush: Mutex::default(),
            }),
            write_state: Arc::new(WriteState {
                flush_policy: config.flush_policy,
                sent_storage: param.sent_strage,
                ..Default::default()
            }),
            config,
        };

        let (waiter, wg) = WaitGroup::new();

        let up_c = up.clone();
        let wg_c = wg.clone();
        let recv = up.conn.subscribe_upstream(param.stream_id_alias)?;
        tokio::spawn(async move { log_err!(debug, up_c.read_loop(wg_c, recv).await) });

        let up_c = up.clone();
        let wg_c = wg;
        let flush_policy = up_c.write_state.flush_policy;
        tokio::spawn(async move { up_c.flush_loop(wg_c, flush_policy).await });

        let up_c = up.clone();
        tokio::spawn(async move { up_c.close_waiter(waiter).await });

        Ok(up)
    }

    /// アップストリームを閉じます。
    pub async fn close(&self, options: Option<UpstreamCloseOptions>) -> Result<()> {
        self.state.check_open()?;
        self.async_flush().await?;
        self.close_without_flush(options).await
    }

    async fn close_without_flush(&self, opts: Option<UpstreamCloseOptions>) -> Result<()> {
        let result = if let Err(e) = self.close_without_flush_inner(opts).await {
            *self.state.error_in_flush.lock().unwrap() = Some(e.clone());
            Err(e)
        } else {
            Ok(())
        };
        if let Some(tx) = &self.callback_channels.tx_recv_ack_callback {
            *tx.write().unwrap() = None;
        }
        if let Some(tx) = &self.callback_channels.tx_send_data_points_callback {
            *tx.write().unwrap() = None;
        }
        if let Some(mut rx) = self.callback_channels.rx_recv_ack_callback_closed.clone() {
            if !*rx.borrow_and_update() {
                log_err!(warn, rx.changed().await);
            }
        }
        if let Some(mut rx) = self
            .callback_channels
            .rx_send_data_points_callback_closed
            .clone()
        {
            if !*rx.borrow_and_update() {
                log_err!(warn, rx.changed().await);
            }
        }
        result
    }

    async fn close_without_flush_inner(&self, opts: Option<UpstreamCloseOptions>) -> Result<()> {
        debug!(
            "close upstream: alias = {}, stream_id = {}",
            self.state.stream_id_alias, self.state.stream_id
        );
        let mut close_notified = self.close_notify.subscribe();
        let mut final_ack_received_notified = self.final_ack_received_notify.subscribe();
        self.state.conn.store(ConnectionState::Closing);

        if let Some(d) = self.config.close_timeout {
            if !self.is_final_ack_received() {
                match tokio::time::timeout(d, final_ack_received_notified.recv()).await {
                    Ok(_) => debug!("final ack received"),
                    Err(e) => {
                        error!("final ack receive error: {}", e);
                    }
                }
            }
        }

        log_err!(
            debug,
            self.conn.unsubscribe_upstream(self.state.stream_id_alias)
        );
        log_err!(trace, self.cancel.notify());

        log_err!(trace, close_notified.recv().await);

        let resp = self
            .conn
            .upstream_close_request(msg::UpstreamCloseRequest {
                stream_id: self.state.stream_id,
                total_data_points: self.write_state.data_point_count(),
                final_sequence_number: self.write_state.latest_used_sequence_number(),
                extension_fields: opts.map(|ext| -> msg::UpstreamCloseRequestExtensionFields {
                    msg::UpstreamCloseRequestExtensionFields {
                        close_session: ext.close_session,
                    }
                }),
                ..Default::default()
            })
            .await?;

        if !resp.result_code.is_succeeded() {
            return Err(Error::from(resp));
        }

        log_err!(
            error,
            self.repository.remove_upstream_by_id(self.stream_id())
        );

        Ok(())
    }

    /// データポイントを内部バッファに書き込みます。
    pub async fn write_data_points(&self, dpg: DataPointGroup) -> Result<()> {
        if let Err(e) = self.write_state.push_data_points(dpg) {
            *self.state.close_caused_by.lock().unwrap() = Some(e.clone());
            if self.state.check_open().is_ok() {
                self.close_without_flush(None).await?;
            }
            return Err(e);
        }

        if let Err(e) = self.state.check_open() {
            let _ = self.process_data_points().await?;
            return Err(e);
        }

        let should_flush =
            (self.write_state.data_point_count() == u64::MAX) || self.check_by_flush_policy();

        if should_flush {
            self.async_flush().await?;
        }

        Ok(())
    }

    pub(crate) fn check_by_flush_policy(&self) -> bool {
        let policy = self.write_state.flush_policy;

        if matches!(policy, FlushPolicy::Immediately) {
            return true;
        }

        if let Some(buffer_size) = policy.buffer_size() {
            if self.write_state.buffered_payload_len() >= buffer_size {
                return true;
            }
        }

        false
    }

    /// データポイントの内部バッファをUpstreamChunkとしてサーバーへ送信します。
    pub async fn flush(&self) -> Result<()> {
        self.state.check_open()?;
        self.async_flush().await
    }

    /// アップストリームの設定を返します。
    pub fn config(&self) -> Arc<UpstreamConfig> {
        self.config.clone()
    }

    /// アップストリームの状態を取得します。
    pub fn state(&self) -> UpstreamState {
        let data_id_aliases = self
            .state
            .id_alias_map
            .lock()
            .unwrap()
            .iter()
            .map(|(id, i)| (*i, id.clone()))
            .collect();
        let total_data_points = self.write_state.data_point_count();
        let last_issued_sequence_number = self.write_state.latest_used_sequence_number();
        let data_points_buffer_on_error = self.write_state.buf_on_err.lock().unwrap().clone();
        let mut data_points_buffer = self.write_state.buf.lock().unwrap().clone();
        data_points_buffer.extend(data_points_buffer_on_error);

        UpstreamState {
            data_id_aliases,
            total_data_points,
            last_issued_sequence_number,
            data_points_buffer,
        }
    }

    /// ストリームIDを取得します
    pub fn stream_id(&self) -> uuid::Uuid {
        self.state.stream_id
    }

    /// UpstreamOpenResponseで返却されたサーバー時刻を取得します
    pub fn server_time(&self) -> DateTime<Utc> {
        self.state.server_time
    }

    async fn async_flush(&self) -> Result<()> {
        let upstream_chunk_msg = match self.process_data_points().await {
            Ok(upstream_chunk_msg) => upstream_chunk_msg,
            Err(e) => {
                *self.state.close_caused_by.lock().unwrap() = Some(e.clone());
                if self.state.check_open().is_ok() {
                    self.close_without_flush(None).await?;
                }
                return Err(e);
            }
        };

        self.state.check_not_closed()?;

        if let Some(upstream_chunk_msg) = upstream_chunk_msg {
            if let Err(e) = self.conn.upstream_chunk(upstream_chunk_msg).await {
                *self.state.close_caused_by.lock().unwrap() = Some(e.clone());
                if self.state.check_open().is_ok() {
                    self.close_without_flush(None).await?;
                }
                return Err(e);
            }
        }

        Ok(())
    }

    /// Process data points in the buffer and send to a storage
    async fn process_data_points(&self) -> Result<Option<msg::UpstreamChunk>> {
        let mut data_ids = Vec::new();

        let dpgs = self.write_state.take_buffered_data_points();

        if dpgs.is_empty() {
            return Ok(None);
        }
        let dpgs_to_callback = dpgs.clone();
        let sequence_number = self.write_state.next_sequence_number();

        // flush callback
        let tx = if let Some(tx) = &self.callback_channels.tx_send_data_points_callback {
            tx.read().unwrap().as_ref().cloned()
        } else {
            None
        };
        if let Some(tx) = tx {
            let upstream_chunk = UpstreamChunk {
                sequence_number,
                data_point_groups: dpgs_to_callback.clone(),
            };
            log_err!(warn, tx.send(upstream_chunk).await);
        }

        let id_alias_map = self.state.id_alias_map.lock().unwrap();

        let mut point_map = HashMap::new();
        for dpg in dpgs.into_iter() {
            //let id = dpg.id;
            for p in dpg.data_points.into_iter() {
                let g = point_map.entry(dpg.id.clone()).or_insert_with(Vec::new);
                g.push(msg::DataPoint {
                    payload: p.payload,
                    elapsed_time: p.elapsed_time.num_nanoseconds().unwrap_or(0),
                });
            }
        }

        let data_point_groups = point_map
            .into_iter()
            .map(|(id, data_points)| {
                let (data_id_or_alias, request_alias) = id_alias_map
                    .get(&id)
                    .map(|u| (msg::DataIdOrAlias::DataIdAlias(*u), false))
                    .unwrap_or_else(|| (msg::DataIdOrAlias::DataId(id.clone()), true));

                if request_alias {
                    data_ids.push(id);
                }

                msg::DataPointGroup {
                    data_points,
                    data_id_or_alias,
                }
            })
            .collect::<Vec<_>>();

        self.write_state.sent_storage.store(
            self.state.stream_id,
            sequence_number,
            dpgs_to_callback,
        )?;

        let stream_chunk = msg::StreamChunk {
            sequence_number,
            data_point_groups,
        };

        Ok(Some(msg::UpstreamChunk {
            stream_id_alias: self.state.stream_id_alias,
            stream_chunk,
            data_ids,
        }))
    }

    /// アップストリームのクローズをサブスクライブします。
    pub fn subscribe_close(&self) -> UpstreamClosedNotificationReceiver {
        UpstreamClosedNotificationReceiver(self.close_notify.subscribe())
    }

    fn process_chunk_results(
        &self,
        results: Vec<msg::UpstreamChunkResult>,
    ) -> Vec<UpstreamChunkResult> {
        results
            .into_iter()
            .map(|res| {
                (
                    res.result_code,
                    res.result_string,
                    self.write_state
                        .sent_storage
                        .remove(self.state.stream_id, res.sequence_number),
                    res.sequence_number,
                )
            })
            .filter(|(_, _, sent, _)| sent.is_some())
            .map(
                |(result_code, result_string, _, sequence_number)| UpstreamChunkResult {
                    sequence_number,
                    result_code,
                    result_string,
                },
            )
            .collect()
    }

    fn update_data_id_alias(&self, alias: HashMap<u32, DataId>) {
        if alias.is_empty() {
            return;
        }

        let mut g = self.state.id_alias_map.lock().unwrap();
        alias.into_iter().for_each(|(a, m_id)| {
            g.insert(m_id, a);
        })
    }

    async fn read_result_loop(&self, mut recv: mpsc::Receiver<Vec<msg::UpstreamChunkResult>>) {
        while let Some(results) = recv.recv().await {
            let chunk_results = self.process_chunk_results(results);
            if chunk_results.is_empty() {
                continue;
            }
            if self.is_final_ack_received() {
                log_err!(debug, self.final_ack_received_notify.send(()));
            }
            let tx = if let Some(tx) = &self.callback_channels.tx_recv_ack_callback {
                tx.read().unwrap().as_ref().cloned()
            } else {
                None
            };
            if let Some(tx) = tx {
                for chunk_result in chunk_results.into_iter() {
                    log_err!(warn, tx.send(chunk_result).await);
                }
            }
        }
    }

    async fn read_alias_loop(&self, mut recv: mpsc::Receiver<HashMap<u32, msg::DataId>>) {
        while let Some(map) = recv.recv().await {
            self.update_data_id_alias(map);
        }
    }

    async fn read_loop(&self, _wg: WaitGroup, mut r: mpsc::Receiver<msg::Message>) -> Result<()> {
        let (result_s, result_r) = mpsc::channel(1);
        let up = self.clone();
        tokio::spawn(async move { up.read_result_loop(result_r).await });

        let (alias_s, alias_r) = mpsc::channel(1);
        let up = self.clone();
        tokio::spawn(async move { up.read_alias_loop(alias_r).await });

        while let Some(m) = r.recv().await {
            let got_ack = match m {
                msg::Message::UpstreamChunkAck(ack) => ack,
                _ => continue,
            };

            let msg::UpstreamChunkAck {
                data_id_aliases,
                results,
                ..
            } = got_ack;

            log_err!(warn, alias_s.send(data_id_aliases).await);
            log_err!(warn, result_s.send(results).await);
        }
        Ok(())
    }

    async fn close_waiter(&self, mut waiter: Waiter) {
        let mut conn_close_notified = self.conn.subscribe_disconnect_notify();

        tokio::select! {
            _ = waiter.wait() => {},
            _ = conn_close_notified.recv() => {},
        }

        self.state.conn.store(ConnectionState::Closed);
        log_err!(error, self.repository.save_upstream(&self.info()));
        log_err!(
            trace,
            self.close_notify.send(UpstreamClosedEvent {
                session_id: self.config.session_id.clone(),
                config: self.config.clone(),
                state: self.state(),
                caused_by: self.state.close_caused_by.lock().unwrap().clone(),
                error: self.state.error_in_flush.lock().unwrap().clone(),
            })
        );
        trace!("exit close waiter");
    }

    fn is_final_ack_received(&self) -> bool {
        !self.state.is_open()
            && self
                .write_state
                .sent_storage
                .remaining(self.state.stream_id)
                == 0
    }

    async fn flush_loop(&self, _wg: WaitGroup, flush_policy: FlushPolicy) {
        let interval = flush_policy.interval().unwrap_or_default();
        if interval == Duration::default() {
            return;
        }

        let mut ticker = tokio::time::interval(interval);
        while self.state.is_open() || { !self.write_state.buf.lock().unwrap().is_empty() } {
            ticker.tick().await;
            if let Err(e) = self.async_flush().await {
                error!("error during flush:\n{:?}", e);
                if matches!(e, Error::ConnectionClosed(_)) {
                    break;
                }
            }
        }
    }

    fn info(&self) -> super::UpstreamInfo {
        super::UpstreamInfo {
            stream_id: self.state.stream_id,
            stream_id_alias: self.state.stream_id_alias,
            qos: self.config.qos,
            data_point_count: self.write_state.data_point_count(),
            flush_policy: self.write_state.flush_policy,
            session_id: self.config.session_id.clone(),
            sequence_number: self.write_state.sequence_number.load(Ordering::Acquire),
            id_alias_map: { self.state.id_alias_map.lock().unwrap().clone() },
            server_time: self.state.server_time,
        }
    }
}

impl CallbackChannels {
    fn new(
        recv_ack_callback: Option<ReceiveAckCallback>,
        send_data_points_callback: Option<SendDataPointsCallback>,
        stream_id: Uuid,
        callback_queue_size: usize,
    ) -> Self {
        let (tx_recv_ack_callback, rx_recv_ack_callback_closed) =
            if let Some(callback) = recv_ack_callback {
                let (tx, mut rx) = mpsc::channel(callback_queue_size);
                let (tx_closed, rx_closed) = watch::channel(false);
                std::thread::spawn(move || {
                    while let Some(chunk_result) = rx.blocking_recv() {
                        callback(stream_id, chunk_result);
                    }
                    log_err!(warn, tx_closed.send(true));
                });
                (Some(RwLock::new(Some(tx))), Some(rx_closed))
            } else {
                (None, None)
            };
        let (tx_send_data_points_callback, rx_send_data_points_callback_closed) =
            if let Some(callback) = send_data_points_callback {
                let (tx, mut rx) = mpsc::channel(callback_queue_size);
                let (tx_closed, rx_closed) = watch::channel(false);
                std::thread::spawn(move || {
                    while let Some(upstream_chunk) = rx.blocking_recv() {
                        callback(stream_id, upstream_chunk);
                    }
                    log_err!(warn, tx_closed.send(true));
                });
                (Some(RwLock::new(Some(tx))), Some(rx_closed))
            } else {
                (None, None)
            };

        CallbackChannels {
            tx_recv_ack_callback,
            tx_send_data_points_callback,
            rx_recv_ack_callback_closed,
            rx_send_data_points_callback_closed,
        }
    }
}

/// Upstreamクローズ時のオプションです。
#[derive(Default)]
pub struct UpstreamCloseOptions {
    /// セッションをクローズするかどうか
    pub close_session: bool,
}

impl From<UpstreamCloseOptions> for msg::UpstreamCloseRequestExtensionFields {
    fn from(r: UpstreamCloseOptions) -> Self {
        Self {
            close_session: r.close_session,
        }
    }
}

/// アップストリームのクローズイベントを受け取るレシーバーです。
#[derive(Debug)]
pub struct UpstreamClosedNotificationReceiver(broadcast::Receiver<UpstreamClosedEvent>);

impl UpstreamClosedNotificationReceiver {
    pub async fn recv(&mut self) -> Option<UpstreamClosedEvent> {
        self.0.recv().await.ok()
    }
}

/// アップストリームのビルダーです。
pub struct UpstreamBuilder<'a> {
    pub(crate) conn: &'a Conn,
    pub(crate) config: UpstreamConfig,
}

impl<'a> UpstreamBuilder<'a> {
    /// アップストリームのオープン
    pub async fn build(self) -> Result<Upstream> {
        self.conn.open_upstream(self.config).await
    }

    /// Ackの返却間隔
    pub fn ack_interval(mut self, ack_interval: chrono::Duration) -> Self {
        self.config.ack_interval = ack_interval;
        self
    }

    /// 有効期限
    pub fn expiry_interval(mut self, expiry_interval: chrono::Duration) -> Self {
        self.config.expiry_interval = expiry_interval;
        self
    }

    /// データIDリスト
    pub fn data_ids(mut self, data_ids: Vec<DataId>) -> Self {
        self.config.data_ids = data_ids;
        self
    }

    /// QoS
    pub fn qos(mut self, qos: QoS) -> Self {
        self.config.qos = qos;
        self
    }

    /// 永続化
    pub fn persist(mut self, persist: bool) -> Self {
        self.config.persist = persist;
        self
    }

    /// フラッシュポリシー
    pub fn flush_policy(mut self, flush_policy: FlushPolicy) -> Self {
        self.config.flush_policy = flush_policy;
        self
    }

    /// Close時のタイムアウト
    pub fn close_timeout(mut self, close_timeout: Option<Duration>) -> Self {
        self.config.close_timeout = close_timeout;
        self
    }

    /// Ack受信時のコールバック
    pub fn recv_ack_callback(mut self, recv_ack_callback: Option<ReceiveAckCallback>) -> Self {
        self.config.recv_ack_callback = recv_ack_callback;
        self
    }

    /// データ送信時のコールバック
    pub fn send_data_points_callback(
        mut self,
        send_data_points_callback: Option<SendDataPointsCallback>,
    ) -> Self {
        self.config.send_data_points_callback = send_data_points_callback;
        self
    }

    /// データ送信時のコールバックのキューサイズ
    pub fn callback_queue_size(mut self, callback_queue_size: usize) -> Self {
        self.config.callback_queue_size = callback_queue_size;
        self
    }
}

#[cfg(test)]
mod test {

    use chrono::TimeZone;
    use std::sync::atomic::AtomicBool;

    use crate::{iscp::test::new_mock_conn, sync::Cancel, wire, Conn, DataPoint, UpstreamConfig};

    use super::*;

    fn new_upstream(
        conn: &Conn,
        recv_ack_callback: Option<ReceiveAckCallback>,
        send_data_points_callback: Option<SendDataPointsCallback>,
    ) -> Upstream {
        let (close_notify, _) = broadcast::channel(1);
        let (final_ack_received_notify, _) = broadcast::channel(1);
        let stream_id = uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let callback_channels = CallbackChannels::new(
            recv_ack_callback.clone(),
            send_data_points_callback.clone(),
            stream_id,
            0xFF,
        );
        Upstream {
            conn: conn.wire_conn.clone(),
            state: Arc::new(State {
                stream_id,
                stream_id_alias: 1,
                server_time: Utc.timestamp_opt(123_456_789, 123_456_789).unwrap(),
                ..Default::default()
            }),
            repository: conn.upstream_repository.clone(),
            write_state: Arc::new(WriteState {
                sent_storage: conn.sent_storage.clone(),
                ..Default::default()
            }),
            close_notify,
            final_ack_received_notify,
            callback_channels: Arc::new(callback_channels),
            cancel: Cancel::new(),
            config: Arc::new(UpstreamConfig {
                session_id: "session".into(),
                recv_ack_callback,
                send_data_points_callback,
                ..Default::default()
            }),
        }
    }

    fn new_upstream_with_write_state(conn: Conn, write_state: WriteState) -> Upstream {
        let (close_notify, _) = broadcast::channel(1);
        let (final_ack_received_notify, _) = broadcast::channel(1);
        Upstream {
            conn: conn.wire_conn.clone(),
            state: Arc::new(State {
                stream_id_alias: 1,
                server_time: Utc.timestamp_opt(123_456_789, 123_456_789).unwrap(),
                ..Default::default()
            }),
            write_state: Arc::new(write_state),
            close_notify,
            final_ack_received_notify,
            callback_channels: Arc::default(),
            cancel: Cancel::new(),
            repository: conn.upstream_repository.clone(),
            config: Arc::new(UpstreamConfig {
                session_id: "session".into(),
                ..Default::default()
            }),
        }
    }

    #[tokio::test]
    async fn flush() {
        let mut mock = wire::MockMockConnection::new();
        mock.expect_upstream_chunk().return_once(|p| {
            assert_eq!(
                msg::DataIdOrAlias::DataIdAlias(1),
                p.stream_chunk.data_point_groups[0].data_id_or_alias
            );
            assert_eq!(1, p.stream_id_alias);
            Ok(())
        });

        #[derive(Default)]
        struct FlushCallback {
            called: AtomicBool,
        }
        impl FlushCallback {
            fn callback(&self, sid: uuid::Uuid, chunk: UpstreamChunk) {
                let stream_id =
                    uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
                assert_eq!(sid, stream_id);
                assert_eq!(chunk.sequence_number, 1);
                assert_eq!(chunk.data_point_groups.len(), 10);
                self.called.store(true, Ordering::Release);
            }
        }
        impl Drop for FlushCallback {
            fn drop(&mut self) {
                assert!(self.called.load(Ordering::Acquire))
            }
        }

        let mock_conn = new_mock_conn("test", mock);
        let flush_callback = FlushCallback::default();
        let up = new_upstream(
            &mock_conn,
            None,
            Some(Arc::new(move |sid, chunk| {
                flush_callback.callback(sid, chunk);
            })),
        );

        let test_id = msg::DataId {
            name: "1".to_string(),
            type_: "1".to_string(),
        };

        up.state
            .id_alias_map
            .lock()
            .unwrap()
            .insert(test_id.clone(), 1);

        for i in 0..10 {
            up.write_data_points(DataPointGroup {
                id: test_id.clone(),
                data_points: vec![DataPoint {
                    elapsed_time: chrono::Duration::seconds(i),
                    payload: vec![1, 2, 3, 4].into(),
                }],
            })
            .await
            .unwrap();
        }

        let state = up.state();
        assert_eq!(state.data_points_buffer.len(), 10);

        up.async_flush().await.unwrap();
        assert_eq!(up.write_state.sequence_number.load(Ordering::Acquire), 2);
        assert_eq!(up.write_state.data_point_count.load(Ordering::Acquire), 10);

        // do nothing
        up.async_flush().await.unwrap();
        assert_eq!(up.write_state.sequence_number.load(Ordering::Acquire), 2);
        assert_eq!(up.write_state.data_point_count.load(Ordering::Acquire), 10);

        let state = up.state();
        assert_eq!(state.total_data_points, 10);
        assert!(state.data_points_buffer.is_empty());
    }

    #[tokio::test]
    async fn send_flush() {
        let mut mock = wire::MockMockConnection::new();
        mock.expect_upstream_chunk().returning(|_| Ok(()));

        let mock_conn = new_mock_conn("test", mock);
        let write_state = WriteState {
            buf: Mutex::new(Vec::with_capacity(10)),
            flush_policy: FlushPolicy::BufferSizeOnly { buffer_size: 40 },
            ..Default::default()
        };
        let up = new_upstream_with_write_state(mock_conn, write_state);

        let test_id = msg::DataId::new("1", "1");
        // fill buffer
        for i in 0..10 {
            up.write_data_points(DataPointGroup {
                id: test_id.clone(),
                data_points: vec![DataPoint {
                    elapsed_time: chrono::Duration::seconds(i),
                    payload: vec![1, 2, 3, 4].into(),
                }],
            })
            .await
            .unwrap();
        }

        assert_eq!(up.write_state.sequence_number.load(Ordering::Acquire), 2);
        assert_eq!(up.write_state.data_point_count.load(Ordering::Acquire), 10);

        // data count max
        up.write_state
            .data_point_count
            .store(u64::MAX - 1, Ordering::Release);
        up.write_data_points(DataPointGroup {
            id: test_id.clone(),
            data_points: vec![DataPoint {
                elapsed_time: chrono::Duration::seconds(1),
                payload: vec![1, 2, 3, 4].into(),
            }],
        })
        .await
        .unwrap();

        assert_eq!(up.write_state.sequence_number.load(Ordering::Acquire), 3);
        assert_eq!(
            up.write_state.data_point_count.load(Ordering::Acquire),
            u64::MAX
        );
    }

    #[tokio::test]
    async fn send_data_data_point_count_overflow() {
        let mut mock = wire::MockMockConnection::new();
        mock.expect_upstream_open_request().return_once(|_| {
            Ok(msg::UpstreamOpenResponse {
                result_code: msg::ResultCode::Succeeded,
                assigned_stream_id_alias: 1,
                ..Default::default()
            })
        });
        mock.expect_subscribe_upstream().return_once(|_| {
            let (_, r) = mpsc::channel(1);
            Ok(r)
        });
        mock.expect_unsubscribe_upstream().returning(|_| Ok(()));
        mock.expect_upstream_close_request().return_once(|_| {
            Ok(msg::UpstreamCloseResponse {
                result_code: msg::ResultCode::Succeeded,
                ..Default::default()
            })
        });

        let mock_conn = new_mock_conn("test", mock);

        let up = mock_conn
            .upstream_builder("test")
            .flush_policy(FlushPolicy::IntervalOnly {
                interval: Duration::from_millis(10),
            })
            .ack_interval(chrono::Duration::milliseconds(10))
            .close_timeout(Some(std::time::Duration::from_secs(1)))
            .build()
            .await
            .unwrap();
        let mut close_event = up.subscribe_close();

        let test_data_point = DataPoint {
            elapsed_time: chrono::Duration::zero(),
            payload: vec![1, 2, 3, 4].into(),
        };
        let test_data_point_group = DataPointGroup {
            id: msg::DataId::new("1", "1"),
            data_points: vec![test_data_point],
        };

        // max data point count
        up.write_state
            .data_point_count
            .store(u64::MAX, Ordering::Release);
        let err = up
            .write_data_points(test_data_point_group.clone())
            .await
            .expect_err("want error");
        assert!(matches!(err, Error::MaxDataPointCount));

        let close_event = close_event.recv().await.unwrap();
        assert_eq!(close_event.caused_by, Some(Error::MaxDataPointCount));
        assert_eq!(close_event.state.data_points_buffer.len(), 1);
    }

    #[tokio::test]
    async fn send_data_serial_number_overflow() {
        let mut mock = wire::MockMockConnection::new();
        mock.expect_upstream_open_request().return_once(|_| {
            Ok(msg::UpstreamOpenResponse {
                result_code: msg::ResultCode::Succeeded,
                assigned_stream_id_alias: 1,
                ..Default::default()
            })
        });
        mock.expect_subscribe_upstream().return_once(|_| {
            let (_, r) = mpsc::channel(1);
            Ok(r)
        });
        mock.expect_unsubscribe_upstream().returning(|_| Ok(()));
        mock.expect_upstream_close_request().return_once(|_| {
            Ok(msg::UpstreamCloseResponse {
                result_code: msg::ResultCode::Succeeded,
                ..Default::default()
            })
        });

        let mock_conn = new_mock_conn("test", mock);

        let up = mock_conn
            .upstream_builder("test")
            .flush_policy(FlushPolicy::IntervalOnly {
                interval: Duration::from_millis(10),
            })
            .ack_interval(chrono::Duration::milliseconds(10))
            .close_timeout(Some(std::time::Duration::from_secs(1)))
            .build()
            .await
            .unwrap();
        let mut close_event = up.subscribe_close();

        let test_data_point = DataPoint {
            elapsed_time: chrono::Duration::zero(),
            payload: vec![1, 2, 3, 4].into(),
        };
        let test_data_point_group = DataPointGroup {
            id: msg::DataId::new("1", "1"),
            data_points: vec![test_data_point],
        };

        // max serial number
        up.write_state.data_point_count.store(0, Ordering::Release);
        up.write_state
            .sequence_number
            .store(u32::MAX, Ordering::Release);
        let err = up
            .write_data_points(test_data_point_group.clone())
            .await
            .expect_err("want error");
        assert!(matches!(err, Error::MaxSequenceNumber));

        let close_event = close_event.recv().await.unwrap();
        assert_eq!(close_event.caused_by, Some(Error::MaxSequenceNumber));
        assert_eq!(close_event.state.data_points_buffer.len(), 1);
    }

    #[tokio::test]
    async fn read_loop() {
        let (tx_read_loop, recv) = mpsc::channel(1);
        let mut mock = wire::MockMockConnection::new();
        mock.expect_subscribe_upstream()
            .return_once(move |_| Ok(recv));
        mock.expect_unsubscribe_upstream().return_once(|_| Ok(()));

        let dpgs = vec![DataPointGroup {
            id: msg::DataId::new("1", "1"),
            data_points: vec![DataPoint {
                elapsed_time: chrono::Duration::seconds(1),
                payload: vec![1, 2, 3, 4].into(),
            }],
        }];

        struct RecvAckCallback {
            want_sid: uuid::Uuid,
            want_seq: u32,
            called: AtomicBool,
        }
        impl RecvAckCallback {
            fn callback(&self, sid: uuid::Uuid, chunk_result: UpstreamChunkResult) {
                assert_eq!(sid, self.want_sid);
                assert_eq!(chunk_result.sequence_number, self.want_seq);
                self.called.store(true, Ordering::Release);
            }
        }
        impl Drop for RecvAckCallback {
            fn drop(&mut self) {
                assert!(self.called.load(Ordering::Acquire))
            }
        }

        let recv_ack_callback = RecvAckCallback {
            want_sid: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            want_seq: 1,
            called: AtomicBool::default(),
        };
        let mock_conn = new_mock_conn("test", mock);
        let up = new_upstream(
            &mock_conn,
            Some(Arc::new(move |sid, chunk_result| {
                recv_ack_callback.callback(sid, chunk_result)
            })),
            None,
        );
        up.write_state
            .sent_storage
            .store(up.stream_id(), 1, dpgs)
            .unwrap();

        let up_c = up.clone();
        let r = up.conn.subscribe_upstream(1).unwrap();
        let (_, wg) = WaitGroup::new();
        tokio::spawn(async move { up_c.read_loop(wg, r).await });

        // send alias
        let mut first_map = msg::DataIdAliasMap::new();
        first_map.insert(1, msg::DataId::new("1", "1"));
        tx_read_loop
            .send(
                msg::UpstreamChunkAck {
                    data_id_aliases: first_map.clone(),
                    ..Default::default()
                }
                .into(),
            )
            .await
            .unwrap();

        let mut second_map = msg::DataIdAliasMap::new();
        second_map.insert(2, msg::DataId::new("2", "2"));
        tx_read_loop
            .send(
                msg::UpstreamChunkAck {
                    data_id_aliases: second_map.clone(),
                    ..Default::default()
                }
                .into(),
            )
            .await
            .unwrap();

        // send ack
        let mock_ack = msg::UpstreamChunkAck {
            stream_id_alias: 1,
            results: vec![msg::UpstreamChunkResult {
                sequence_number: 1,
                result_code: msg::ResultCode::Succeeded,
                result_string: "".to_string(),
            }],
            ..Default::default()
        };
        tx_read_loop
            .send(mock_ack.clone().into())
            .await
            .expect("ok");

        // confirm map
        let want_map: msg::DataIdAliasMap = first_map.into_iter().chain(second_map).collect();
        {
            let got_map = up.state.id_alias_map.lock().unwrap();
            want_map.into_iter().for_each(|(a, id)| {
                let alias = got_map.get(&id).unwrap();
                assert_eq!(*alias, a);
            });
        }

        for _ in 0..100 {
            if 0 == up.write_state.sent_storage.remaining(up.stream_id()) {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        unreachable!("")
    }

    #[tokio::test]
    async fn close() {
        let mut mock = wire::MockMockConnection::new();
        mock.expect_upstream_open_request().return_once(|_| {
            Ok(msg::UpstreamOpenResponse {
                result_code: msg::ResultCode::Succeeded,
                assigned_stream_id_alias: 1,
                assigned_stream_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111")
                    .unwrap(),
                ..Default::default()
            })
        });
        mock.expect_subscribe_upstream().return_once(|_| {
            let (_, r) = mpsc::channel(1);
            Ok(r)
        });
        mock.expect_unsubscribe_upstream().returning(|_| Ok(()));
        mock.expect_upstream_close_request().return_once(|req| {
            let request_id = req.request_id;
            assert_eq!(
                req,
                msg::UpstreamCloseRequest {
                    request_id,
                    stream_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
                    final_sequence_number: 99,
                    total_data_points: 100_000,
                    ..Default::default()
                }
            );
            Ok(msg::UpstreamCloseResponse {
                result_code: msg::ResultCode::Succeeded,
                ..Default::default()
            })
        });

        let mock_conn = new_mock_conn("test", mock);

        let up = mock_conn
            .upstream_builder("test")
            .close_timeout(Some(std::time::Duration::from_secs(1)))
            .build()
            .await
            .unwrap();

        up.write_state.sequence_number.store(100, Ordering::Release);
        up.write_state
            .data_point_count
            .store(100_000, Ordering::Release);

        up.close(None).await.unwrap();
        assert!(up.is_final_ack_received());

        //    let maybe_ack = up.next_ack().await;
        //    assert!(maybe_ack.is_none());

        let err = up.close(None).await.expect_err("want error");
        assert!(matches!(err, Error::ConnectionClosed(_)));
    }

    #[tokio::test]
    async fn close_no_final_ack() {
        let mut mock = wire::MockMockConnection::new();
        mock.expect_upstream_open_request().return_once(|_| {
            Ok(msg::UpstreamOpenResponse {
                result_code: msg::ResultCode::Succeeded,
                assigned_stream_id_alias: 1,
                ..Default::default()
            })
        });
        mock.expect_subscribe_upstream().return_once(|_| {
            let (_, r) = mpsc::channel(1);
            Ok(r)
        });
        mock.expect_unsubscribe_upstream().returning(|_| Ok(()));
        mock.expect_upstream_close_request().return_once(|_| {
            Ok(msg::UpstreamCloseResponse {
                result_code: msg::ResultCode::Succeeded,
                ..Default::default()
            })
        });

        let mock_conn = new_mock_conn("test", mock);

        let up = mock_conn
            .upstream_builder("test")
            .flush_policy(FlushPolicy::IntervalOnly {
                interval: Duration::from_millis(10),
            })
            .ack_interval(chrono::Duration::milliseconds(10))
            .close_timeout(Some(std::time::Duration::from_secs(1)))
            .build()
            .await
            .unwrap();

        up.write_state
            .sent_storage
            .store(
                up.stream_id(),
                0,
                vec![DataPointGroup {
                    id: msg::DataId {
                        name: "test".to_string(),
                        type_: "test".to_string(),
                    },
                    data_points: vec![DataPoint {
                        elapsed_time: chrono::Duration::zero(),
                        payload: vec![1, 2, 3, 4].into(),
                    }],
                }],
            )
            .unwrap();

        up.close(None).await.unwrap();
        assert!(!up.is_final_ack_received());
        // assert!(up.next_ack().await.is_none());
    }
}
