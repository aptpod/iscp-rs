//! アップストリームに関するモジュールです。

use std::{
    collections::HashMap,
    fmt,
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use chrono::{DateTime, Utc};
use crossbeam::atomic::AtomicCell;
use log::*;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

use crate::{
    msg, wire, Ack, Cancel, DataId, DataPoint, DataPointId, Error, FlushPolicy, IdAliasMap,
    NopReceiveAckHooker, NopSendDataPointsHooker, QoS, ReceiveAckHooker, Result,
    SendDataPointsHooker, WaitGroup, Waiter,
};

/// アップストリームの設定です。
#[derive(Clone)]
pub struct UpstreamConfig {
    pub session_id: String,
    pub ack_interval: chrono::Duration,
    pub expiry_interval: chrono::Duration,
    pub data_ids: Vec<DataId>,
    pub qos: QoS,
    pub persist: Option<bool>,
    pub flush_policy: FlushPolicy,
    pub recv_ack_hooker: Arc<dyn ReceiveAckHooker>,
    pub send_data_points_hooker: Arc<dyn SendDataPointsHooker>,
}

impl Default for UpstreamConfig {
    fn default() -> Self {
        Self {
            session_id: "".to_string(),
            ack_interval: chrono::Duration::milliseconds(100),
            expiry_interval: chrono::Duration::zero(),
            data_ids: Vec::new(),
            qos: QoS::Unreliable,
            persist: None,
            flush_policy: FlushPolicy::default(),
            recv_ack_hooker: Arc::new(NopReceiveAckHooker),
            send_data_points_hooker: Arc::new(NopSendDataPointsHooker),
        }
    }
}

pub type BoxedUpstreamOption = Box<dyn Fn(&mut UpstreamConfig)>;

impl UpstreamConfig {
    pub fn new_with(session_id: &str, opts: Vec<BoxedUpstreamOption>) -> Self {
        let mut cfg = UpstreamConfig {
            session_id: session_id.to_string(),
            ..Default::default()
        };
        for opt in opts.iter() {
            opt(&mut cfg);
        }
        cfg
    }
}

impl From<&UpstreamConfig> for msg::UpstreamOpenRequest {
    fn from(c: &UpstreamConfig) -> Self {
        Self {
            session_id: c.session_id.clone(),
            ack_interval: c.ack_interval,
            expiry_interval: c.expiry_interval,
            qos: c.qos,
            data_ids: c.data_ids.clone(),
            persist: c.persist,
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
    close_notify: broadcast::Sender<()>,
    final_ack_received_notify: broadcast::Sender<()>,

    recv_ack_hooker: Arc<dyn ReceiveAckHooker>,
    send_data_points_hooker: Arc<dyn SendDataPointsHooker>,

    config: UpstreamConfig,
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

/// UpstreamStateは、アップストリーム情報です。
#[derive(Default, Debug)]
struct State {
    /// ストリームID
    stream_id: Uuid,
    stream_id_alias: u32,
    // データIDとエイリアスのマップ
    id_alias_map: Mutex<IdAliasMap>,
    conn: AtomicCell<ConnectionState>,

    // TODO: add 総送信データポイント数
    // TODO: add 最後に払い出されたシーケンス番号
    // TODO: add 受信したUpstreamChunkResult内での最大シーケンス番号
    /// UpstreamOpenResponseで返却されたサーバー時刻
    server_time: DateTime<Utc>,
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
    buf: Mutex<Vec<DataPoint>>,
    buffered_payload_len: AtomicU64,
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
            sent_storage: Arc::new(super::InMemSentStorageNoPayload::default()),
        }
    }
}

impl WriteState {
    fn push_data_point(&self, dp: DataPoint) -> Result<()> {
        let mut buf = self.buf.lock().unwrap();
        self.check_sequence_number_overflow()?;

        self.buffered_payload_len
            .fetch_add(dp.payload.len() as u64, Ordering::Release);
        buf.push(dp);
        self.add_data_point_count(1)?;
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

    fn take_buffered_data_points(&self) -> Vec<DataPoint> {
        let mut buf = self.buf.lock().unwrap();
        self.buffered_payload_len.store(0, Ordering::Release);

        std::mem::take(buf.as_mut())
    }
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
        config: UpstreamConfig,
        param: UpstreamParam,
    ) -> Result<Self> {
        // validate
        if param.stream_id_alias == 0 {
            return Err(Error::invalid_value("stream_id_alias must be not zero"));
        }

        let (close_notify, _) = broadcast::channel(1);
        let (final_ack_received_notify, _) = broadcast::channel(1);

        let up = Self {
            cancel: Cancel::new(),
            close_notify,
            final_ack_received_notify,
            conn,
            repository: param.repository,
            state: Arc::new(State {
                stream_id: param.stream_id,
                stream_id_alias: param.stream_id_alias,
                id_alias_map: Mutex::new(IdAliasMap::new()),
                conn: AtomicCell::new(ConnectionState::Open),
                server_time: param.server_time,
            }),
            write_state: Arc::new(WriteState {
                flush_policy: config.flush_policy,
                sent_storage: param.sent_strage,
                ..Default::default()
            }),
            recv_ack_hooker: config.recv_ack_hooker.clone(),
            send_data_points_hooker: config.send_data_points_hooker.clone(),
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

    pub(crate) fn stream_id(&self) -> uuid::Uuid {
        self.state.stream_id
    }

    /// アップストリームを閉じます。
    pub async fn close(
        &self,
        opts: Option<UpstreamCloseOptions>,
        final_ack_wait_timeout: Option<Duration>,
    ) -> Result<()> {
        self.state.check_open()?;

        debug!(
            "close upstream: alias = {}, stream_id = {}",
            self.state.stream_id_alias, self.state.stream_id
        );

        self.async_flush().await?;

        let mut close_notified = self.close_notify.subscribe();
        let mut final_ack_received_notified = self.final_ack_received_notify.subscribe();
        self.state.conn.store(ConnectionState::Closing);

        if let Some(d) = final_ack_wait_timeout {
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
    pub async fn write_data_point(&self, d: DataPoint) -> Result<()> {
        self.write_state.push_data_point(d)?;

        if let Err(e) = self.state.check_open() {
            let _ = self.process_data_points()?;
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

    /// アップストリームの生成に使用したパラメーターを返します。
    pub fn to_config(&self) -> UpstreamConfig {
        self.config.clone()
    }

    async fn async_flush(&self) -> Result<()> {
        let upstream_chunk_msg = self.process_data_points()?;

        self.state.check_not_closed()?;

        if let Some(upstream_chunk_msg) = upstream_chunk_msg {
            self.conn.upstream_chunk(upstream_chunk_msg).await?;
        }

        Ok(())
    }

    /// Process data points in the buffer and send to a storage
    fn process_data_points(&self) -> Result<Option<msg::UpstreamChunk>> {
        let mut data_ids = Vec::new();

        let dps = self.write_state.take_buffered_data_points();
        let id_alias_map = self.state.id_alias_map.lock().unwrap();
        if dps.is_empty() {
            return Ok(None);
        }
        let data_points = dps.clone();
        let dps_to_hooker = dps.clone();
        let sequence_number = self.write_state.next_sequence_number();

        // flush hooker
        self.send_data_points_hooker.hook_before_send_data_points(
            self.stream_id(),
            sequence_number,
            dps_to_hooker,
        );

        let mut point_map = HashMap::new();
        for p in dps.into_iter() {
            let g = point_map.entry(p.id).or_insert_with(Vec::new);
            g.push(msg::DataPoint {
                payload: p.payload,
                elapsed_time: p.elapsed_time.num_nanoseconds().unwrap_or(0),
            });
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

        self.write_state
            .sent_storage
            .save(self.state.stream_id, sequence_number, data_points)?;

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

    fn results_to_acks(&self, results: Vec<msg::UpstreamChunkResult>) -> Vec<(Ack, u32)> {
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
            .map(|(result_code, result_string, sent, seq)| {
                (
                    Ack {
                        result_code,
                        result_string,
                        data_point_ids: sent
                            .map(|dps| {
                                dps.into_iter()
                                    .map(|dp| DataPointId {
                                        id: dp.id,
                                        elapsed_time: dp.elapsed_time,
                                    })
                                    .collect::<Vec<_>>()
                            })
                            .unwrap(),
                    },
                    seq,
                )
            })
            .collect::<Vec<(_, _)>>()
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
            let acks = self.results_to_acks(results);
            if acks.is_empty() {
                continue;
            }
            if self.is_final_ack_received() {
                log_err!(debug, self.final_ack_received_notify.send(()));
            }
            acks.iter().for_each(|(ack, seq)| {
                self.recv_ack_hooker
                    .hook_after_recv_ack(self.stream_id(), *seq, ack.clone());
            });
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
        let mut conn_close_notified = self.conn.subscribe_close_notify();

        tokio::select! {
            _ = waiter.wait() => {},
            _ = conn_close_notified.recv() => {},
        }

        self.state.conn.store(ConnectionState::Closed);
        log_err!(error, self.repository.save_upstream(&self.info()));
        log_err!(trace, self.close_notify.send(()));
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

#[cfg(test)]
mod test {

    use chrono::TimeZone;
    use std::sync::atomic::AtomicBool;

    use crate::{
        iscp::test::new_mock_conn, sync::Cancel, up_opts, wire, Conn, NopReceiveAckHooker,
        NopSendDataPointsHooker, TokenSource, UpstreamConfig,
    };

    use super::*;

    fn new_upstream<T>(
        conn: &Conn<T>,
        recv_ack_hooker: Arc<dyn ReceiveAckHooker>,
        send_data_points_hooker: Arc<dyn SendDataPointsHooker>,
    ) -> Upstream
    where
        T: TokenSource + Clone,
    {
        let (close_notify, _) = broadcast::channel(1);
        let (final_ack_received_notify, _) = broadcast::channel(1);
        Upstream {
            conn: conn.wire_conn.clone(),
            state: Arc::new(State {
                stream_id: uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
                stream_id_alias: 1,
                server_time: Utc.timestamp(123_456_789, 123_456_789),
                ..Default::default()
            }),
            repository: conn.upstream_repository.clone(),
            write_state: Arc::new(WriteState {
                sent_storage: conn.sent_storage.clone(),
                ..Default::default()
            }),
            close_notify,
            final_ack_received_notify,
            cancel: Cancel::new(),
            recv_ack_hooker,
            send_data_points_hooker,
            config: UpstreamConfig {
                session_id: "session".to_string(),
                ..Default::default()
            },
        }
    }

    fn new_upstream_with_write_state<T>(conn: Conn<T>, write_state: WriteState) -> Upstream
    where
        T: TokenSource + Clone,
    {
        let (close_notify, _) = broadcast::channel(1);
        let (final_ack_received_notify, _) = broadcast::channel(1);
        Upstream {
            conn: conn.wire_conn.clone(),
            state: Arc::new(State {
                stream_id_alias: 1,
                server_time: Utc.timestamp(123_456_789, 123_456_789),
                ..Default::default()
            }),
            write_state: Arc::new(write_state),
            close_notify,
            final_ack_received_notify,
            cancel: Cancel::new(),
            repository: conn.upstream_repository.clone(),
            recv_ack_hooker: UpstreamConfig::default().recv_ack_hooker,
            send_data_points_hooker: UpstreamConfig::default().send_data_points_hooker,
            config: UpstreamConfig::default(),
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
        struct FlushHooker {
            called: AtomicBool,
        }
        impl SendDataPointsHooker for FlushHooker {
            fn hook_before_send_data_points(
                &self,
                sid: uuid::Uuid,
                sequence: u32,
                dps: Vec<DataPoint>,
            ) {
                let stream_id =
                    uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
                assert_eq!(sid, stream_id);
                assert_eq!(sequence, 1);
                assert_eq!(dps.len(), 10);
                self.called.store(true, Ordering::Release);
            }
        }
        impl Drop for FlushHooker {
            fn drop(&mut self) {
                assert!(self.called.load(Ordering::Acquire))
            }
        }

        let mock_conn = new_mock_conn("test", mock);
        let up = new_upstream(
            &mock_conn,
            Arc::new(NopReceiveAckHooker),
            Arc::new(FlushHooker::default()),
        );

        let test_id = msg::DataId {
            name: "1".to_string(),
            r#type: "1".to_string(),
        };

        up.state
            .id_alias_map
            .lock()
            .unwrap()
            .insert(test_id.clone(), 1);

        for i in 0..10 {
            up.write_data_point(DataPoint {
                id: test_id.clone(),
                elapsed_time: chrono::Duration::seconds(i),
                payload: vec![1, 2, 3, 4],
            })
            .await
            .unwrap();
        }

        up.async_flush().await.unwrap();
        assert_eq!(up.write_state.sequence_number.load(Ordering::Acquire), 2);
        assert_eq!(up.write_state.data_point_count.load(Ordering::Acquire), 10);

        // do nothing
        up.async_flush().await.unwrap();
        assert_eq!(up.write_state.sequence_number.load(Ordering::Acquire), 2);
        assert_eq!(up.write_state.data_point_count.load(Ordering::Acquire), 10);
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
            up.write_data_point(DataPoint {
                id: test_id.clone(),
                elapsed_time: chrono::Duration::seconds(i),
                payload: vec![1, 2, 3, 4],
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
        up.write_data_point(DataPoint {
            id: test_id.clone(),
            elapsed_time: chrono::Duration::seconds(1),
            payload: vec![1, 2, 3, 4],
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
    async fn send_overflow() {
        let mut mock = wire::MockMockConnection::new();
        mock.expect_upstream_chunk().return_once(|_| Ok(()));

        let mock_conn = new_mock_conn("test", mock);

        let test_data_point = DataPoint {
            id: msg::DataId::new("1", "1"),
            elapsed_time: chrono::Duration::zero(),
            payload: vec![1, 2, 3, 4],
        };

        let write_state = WriteState {
            buf: Mutex::new(Vec::with_capacity(10)),
            ..Default::default()
        };
        let up = new_upstream_with_write_state(mock_conn, write_state);

        // max data point count
        up.write_state
            .data_point_count
            .store(u64::MAX, Ordering::Release);
        let err = up
            .write_data_point(test_data_point.clone())
            .await
            .expect_err("want error");
        assert!(matches!(err, Error::MaxDataPointCount));

        // max serial number
        up.write_state.data_point_count.store(0, Ordering::Release);
        up.write_state
            .sequence_number
            .store(u32::MAX, Ordering::Release);
        let err = up
            .write_data_point(test_data_point.clone())
            .await
            .expect_err("want error");
        assert!(matches!(err, Error::MaxSequenceNumber));
    }

    #[tokio::test]
    async fn read_loop() {
        let (tx_read_loop, recv) = mpsc::channel(1);
        let mut mock = wire::MockMockConnection::new();
        mock.expect_subscribe_upstream()
            .return_once(move |_| Ok(recv));
        mock.expect_unsubscribe_upstream().return_once(|_| Ok(()));

        let dps = vec![DataPoint {
            id: msg::DataId::new("1", "1"),
            elapsed_time: chrono::Duration::seconds(1),
            payload: vec![1, 2, 3, 4],
        }];

        let ids = dps.iter().map(|dp| dp.data_point_id()).collect::<Vec<_>>();
        struct RecvAckHooker {
            want_sid: uuid::Uuid,
            want_seq: u32,
            want_ids: Vec<DataPointId>,
            called: AtomicBool,
        }
        impl ReceiveAckHooker for RecvAckHooker {
            fn hook_after_recv_ack(&self, sid: uuid::Uuid, sequence: u32, ack: Ack) {
                assert_eq!(sid, self.want_sid);
                assert_eq!(sequence, self.want_seq);
                assert_eq!(ack.data_point_ids, self.want_ids.clone());
                self.called.store(true, Ordering::Release);
            }
        }
        impl Drop for RecvAckHooker {
            fn drop(&mut self) {
                assert!(self.called.load(Ordering::Acquire))
            }
        }

        let mock_conn = new_mock_conn("test", mock);
        let up = new_upstream(
            &mock_conn,
            Arc::new(RecvAckHooker {
                want_sid: uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
                want_ids: ids,
                want_seq: 0,
                called: AtomicBool::default(),
            }),
            Arc::new(NopSendDataPointsHooker),
        );
        up.write_state
            .sent_storage
            .save(up.stream_id(), 0, dps)
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
            .open_upstream(
                "test",
                vec![up_opts::with_flush_policy(FlushPolicy::default())],
            )
            .await
            .unwrap();

        up.write_state.sequence_number.store(100, Ordering::Release);
        up.write_state
            .data_point_count
            .store(100_000, Ordering::Release);

        up.close(None, Some(std::time::Duration::from_secs(1)))
            .await
            .unwrap();
        assert!(up.is_final_ack_received());

        //    let maybe_ack = up.next_ack().await;
        //    assert!(maybe_ack.is_none());

        let err = up.close(None, None).await.expect_err("want error");
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
            .open_upstream(
                "test",
                vec![
                    up_opts::with_flush_policy(FlushPolicy::IntervalOnly {
                        interval: Duration::from_millis(10),
                    }),
                    up_opts::with_ack_interval(chrono::Duration::milliseconds(10)),
                ],
            )
            .await
            .unwrap();

        up.write_state
            .sent_storage
            .save(
                up.stream_id(),
                0,
                vec![DataPoint {
                    id: msg::DataId {
                        name: "test".to_string(),
                        r#type: "test".to_string(),
                    },
                    elapsed_time: chrono::Duration::zero(),
                    payload: vec![1, 2, 3, 4],
                }],
            )
            .unwrap();

        up.close(None, Some(std::time::Duration::from_secs(1)))
            .await
            .unwrap();
        assert!(!up.is_final_ack_received());
        // assert!(up.next_ack().await.is_none());
    }
}
