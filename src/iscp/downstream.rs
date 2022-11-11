//! ダウンストリームに関するモジュールです。
//!
use std::{
    collections::{hash_map::Entry, HashMap},
    fmt,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use chrono::{DateTime, Utc};
use crossbeam::atomic::AtomicCell;
use log::*;
use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;

use crate::{
    msg, Cancel, DownstreamDataPoint, DownstreamFilter, DownstreamMetadata, Error, QoS, Result,
    WaitGroup, Waiter,
};

/// ダウンストリームの設定です。
#[derive(Clone, Debug)]
pub struct DownstreamConfig {
    /// ダウンストリームフィルター
    pub filters: Vec<DownstreamFilter>,
    /// データIDエイリアス
    pub expiry_interval: chrono::Duration,
    /// QoS
    pub qos: QoS,
    // TODO: add データIDエイリアス
    // TODO: add Ackのインターバル
}

impl Default for DownstreamConfig {
    fn default() -> Self {
        Self {
            filters: Vec::new(),
            expiry_interval: chrono::Duration::zero(),
            qos: QoS::Unreliable,
        }
    }
}

pub type BoxedDownstreamOption = Box<dyn Fn(&mut DownstreamConfig)>;

impl DownstreamConfig {
    pub fn new_with(filters: Vec<DownstreamFilter>, opts: Vec<BoxedDownstreamOption>) -> Self {
        let mut cfg = DownstreamConfig {
            filters,
            ..Default::default()
        };
        for opt in opts.iter() {
            opt(&mut cfg);
        }
        cfg
    }
}

impl From<DownstreamConfig> for msg::DownstreamOpenRequest {
    fn from(c: DownstreamConfig) -> Self {
        Self {
            expiry_interval: c.expiry_interval,
            downstream_filters: c.filters,
            qos: c.qos,
            ..Default::default()
        }
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

/// ダウンストリームの状態です。
#[derive(Default, Debug)]
struct State {
    /// ID
    stream_id: Uuid,
    stream_id_alias: u32,

    // TODO: add データIDエイリアスとデータIDのマップ
    // TODO: add 最後に払い出されたデータIDエイリアス
    // TODO: add アップストリームエイリアスとアップストリーム情報のマップ
    // TODO: add 最後に払い出されたアップストリーム情報のエイリアス
    // TODO: add 最後に払い出されたAckのシーケンス番号
    /// DownstreamOpenResponseで返却されたサーバー時刻
    server_time: DateTime<Utc>,

    ack_id: AtomicU32,
    conn: AtomicCell<ConnectionState>,
    upstreams_info: AliasState<msg::UpstreamInfo>,
    data_ids: AliasState<msg::DataId>,
    ack_buf: Mutex<Vec<msg::DownstreamChunkResult>>,
    source_node_ids: Vec<String>,
    last_recv_sequences: Mutex<HashMap<msg::UpstreamInfo, u32>>,
}

impl State {
    fn next_ack_id(&self) -> u32 {
        self.ack_id.fetch_add(1, Ordering::Release)
    }
    fn last_used_ack_id(&self) -> u32 {
        let res = self.ack_id.load(Ordering::Acquire);
        if res == 0 {
            return 0;
        }
        res - 1
    }
    fn check_open(&self) -> Result<()> {
        if !self.is_open() {
            return Err(Error::ConnectionClosed("".into()));
        }
        Ok(())
    }

    fn is_open(&self) -> bool {
        ConnectionState::Open == self.conn.load()
    }

    fn update_last_recv_sequences(&self, u: msg::UpstreamInfo, s: u32) -> Option<u32> {
        match self.last_recv_sequences.lock().unwrap().entry(u) {
            Entry::Occupied(mut e) => {
                if e.get() > &s {
                    return None;
                }
                Some(e.insert(s))
            }
            Entry::Vacant(e) => {
                e.insert(s);

                Some(s)
            }
        }
    }
}

#[derive(Debug)]
struct AliasState<T> {
    map: Mutex<HashMap<u32, T>>,
    alias: AtomicU32,
    buf: Mutex<Vec<(u32, T)>>,
}

impl<T> Default for AliasState<T> {
    fn default() -> Self {
        Self {
            map: Mutex::default(),
            alias: AtomicU32::new(1),
            buf: Mutex::default(),
        }
    }
}

impl<T: Clone> Clone for AliasState<T> {
    fn clone(&self) -> Self {
        let map = self.map.lock().unwrap().clone();
        let alias = self.alias.load(Ordering::Acquire);
        let buf = self.buf.lock().unwrap().clone();

        Self {
            map: Mutex::new(map),
            alias: AtomicU32::new(alias),
            buf: Mutex::new(buf),
        }
    }
}

impl<T> AliasState<T>
where
    T: Eq + Clone,
{
    #[allow(dead_code)]
    fn new(map: HashMap<u32, T>) -> Self {
        Self {
            alias: AtomicU32::new(map.len() as u32 + 1u32),
            map: Mutex::new(map),
            buf: Mutex::default(),
        }
    }
    fn next_alias(&self) -> u32 {
        self.alias.fetch_add(1, Ordering::Release)
    }
    fn exist_alias(&self, data: &T) -> bool {
        let g = self.map.lock().unwrap();
        g.iter().any(|(_, v)| data == v)
    }
    fn assign_alias(&self, data: &T) -> Option<u32> {
        if self.exist_alias(data) {
            return None;
        }

        let alias = self.next_alias();

        self.map.lock().unwrap().insert(alias, data.clone());
        self.buf.lock().unwrap().push((alias, data.clone()));

        Some(alias)
    }

    fn want_send(&self) -> HashMap<u32, T> {
        let mut buf = self.buf.lock().unwrap();
        if buf.is_empty() {
            return HashMap::new();
        }
        let buf: Vec<(u32, T)> = std::mem::take(buf.as_mut());
        buf.into_iter().collect()
    }
    fn find(&self, alias: u32) -> Option<T> {
        let mut g = self.map.lock().unwrap();
        match g.entry(alias) {
            Entry::Occupied(e) => Some(e.get().clone()),
            Entry::Vacant(_) => None,
        }
    }
    fn map(&self) -> HashMap<u32, T> {
        self.map.lock().unwrap().clone()
    }
}

type DataPointsSender = oneshot::Sender<DownstreamDataPoint>;
type MetadataSender = oneshot::Sender<DownstreamMetadata>;

pub(super) struct DownstreamParam {
    pub(super) stream_id: Uuid,
    pub(super) stream_id_alias: u32,
    pub(super) data_points_subscriber: mpsc::Receiver<msg::Message>,
    pub(super) metadata_subscriber: broadcast::Receiver<msg::DownstreamMetadata>,
    pub(super) source_node_ids: Vec<String>,
    pub(super) repository: Arc<dyn super::DownstreamRepository>,
    pub(super) server_time: DateTime<Utc>,
}

/// ダウンストリームです。
#[derive(Clone)]
pub struct Downstream {
    cancel: Cancel,
    conn: Arc<dyn crate::wire::Connection>,
    state: Arc<State>,
    data_points_cmd_sender: mpsc::Sender<DataPointsSender>,
    metadata_cmd_sender: mpsc::Sender<MetadataSender>,
    notify_close: broadcast::Sender<()>,
    repository: Arc<dyn super::DownstreamRepository>,
    config: DownstreamConfig,
}

impl fmt::Debug for Downstream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Downstream")
            .field("stream_id", &self.state.stream_id)
            .field("alias", &self.state.stream_id_alias)
            .field("current_ack_id", &self.state.last_used_ack_id())
            .finish()
    }
}

impl Downstream {
    pub(super) fn new(
        conn: Arc<dyn crate::wire::Connection>,
        config: DownstreamConfig,
        param: DownstreamParam,
    ) -> Self {
        let (data_points_cmd_sender, data_points_cmd_receiver) = mpsc::channel(1);
        let (metadata_cmd_sender, metadata_cmd_receiver) = mpsc::channel(1);

        let (notify_close, _) = broadcast::channel(1);

        let down = Self {
            cancel: Cancel::new(),
            conn,
            state: Arc::new(State {
                stream_id: param.stream_id,
                stream_id_alias: param.stream_id_alias,
                ack_id: AtomicU32::new(0),
                conn: AtomicCell::new(ConnectionState::Open),
                source_node_ids: param.source_node_ids,
                server_time: param.server_time,
                ..Default::default()
            }),
            data_points_cmd_sender,
            metadata_cmd_sender,
            notify_close,
            repository: param.repository,
            config,
        };

        let (waiter, wg) = WaitGroup::new();

        let down_c = down.clone();
        let subscriber = param.data_points_subscriber;
        let wg_c = wg.clone();
        tokio::spawn(async move {
            down_c
                .data_points_read_loop(wg_c, subscriber, data_points_cmd_receiver)
                .await;
        });

        let down_c = down.clone();
        let subscriber = param.metadata_subscriber;
        let wg_c = wg.clone();
        tokio::spawn(async move {
            down_c
                .metadata_read_loop(wg_c, subscriber, metadata_cmd_receiver)
                .await;
        });

        let down_c = down.clone();
        tokio::spawn(async move {
            down_c.ack_flush_loop(wg, Duration::from_millis(10)).await;
        });

        let down_c = down.clone();
        tokio::spawn(async move { down_c.close_waiter(waiter).await });

        down
    }

    /// ダウンストリームデータポイントを受信します。
    pub async fn read_data_point(&self) -> Option<DownstreamDataPoint> {
        if !self.state.is_open() {
            warn!("closed");
            return None;
        }

        let (s, r) = oneshot::channel();

        if self.data_points_cmd_sender.send(s).await.is_err() {
            warn!("send error");
            return None;
        }

        match r.await {
            Ok(points) => Some(points),
            Err(e) => {
                debug!("channel is closed: {}", e);
                None
            }
        }
    }

    /// ダウンストリームメタデータを受信します。
    pub async fn recv_metadata(&self) -> Option<DownstreamMetadata> {
        if !self.state.is_open() {
            return None;
        }

        let (s, r) = oneshot::channel();

        self.metadata_cmd_sender.send(s).await.ok()?;

        match r.await {
            Ok(meta) => Some(meta),
            Err(e) => {
                debug!("channel is closed: {}", e);
                None
            }
        }
    }

    /// ダウンストリームを閉じます。
    pub async fn close(&self) -> Result<()> {
        self.state.check_open()?;
        debug!(
            "close downstream: alias = {}, stream_id = {}",
            self.state.stream_id_alias, self.state.stream_id
        );

        let mut close_notified = self.notify_close.subscribe();
        self.state.conn.store(ConnectionState::Closing);
        log_err!(trace, self.cancel.notify());

        log_err!(
            warn,
            self.conn.unsubscribe_downstream(self.state.stream_id_alias)
        );

        log_err!(warn, close_notified.recv().await);

        let resp = self
            .conn
            .downstream_close_request(msg::DownstreamCloseRequest {
                stream_id: self.state.stream_id,
                ..Default::default()
            })
            .await?;

        if !resp.result_code.is_succeeded() {
            return Err(Error::from(resp));
        }

        log_err!(
            error,
            self.repository
                .remove_downstream_by_id(self.state.stream_id)
        );

        Ok(())
    }

    /// ダウンストリームの生成に使用したパラメーターを返します。
    pub fn to_config(&self) -> DownstreamConfig {
        self.config.clone()
    }
}

impl Downstream {
    async fn data_points_cmd_loop(
        &self,
        mut cmd_receiver: mpsc::Receiver<DataPointsSender>,
        mut data_points_receiver: mpsc::Receiver<DownstreamDataPoint>,
    ) -> Result<()> {
        let mut points_to_send = None;
        while let Some(sender) = cmd_receiver.recv().await {
            let points = if let Some(points) = points_to_send.take() {
                points
            } else {
                match data_points_receiver.recv().await {
                    Some(points) => points,
                    None => {
                        debug!("read loop closed, exit cmd_loop");
                        break;
                    }
                }
            };
            if let Err(points) = sender.send(points) {
                points_to_send = Some(points);
            }
        }
        Ok(())
    }

    fn process_ack_complete(&self, _ack: msg::DownstreamChunkAckComplete) {
        if _ack.result_code.is_succeeded() {
            return;
        }
        // todo handing
        debug!("ack complete error: {}", _ack.result_string);
    }

    async fn read_data_points_loop(
        &self,
        mut recv: mpsc::Receiver<msg::DownstreamChunk>,
        sender: mpsc::Sender<DownstreamDataPoint>,
    ) {
        while let Some(points) = recv.recv().await {
            let err = match self.process_data_points(points) {
                Ok(data) => {
                    for d in data.into_iter() {
                        log_err!(warn, sender.try_send(d));
                    }
                    continue;
                }
                Err(err) => err,
            };
            match err {
                Error::FailedMessage {
                    code: result_code,
                    detail: result_string,
                } => {
                    let future = self.conn.disconnect(msg::Disconnect {
                        result_code,
                        result_string,
                    });
                    let res = tokio::time::timeout(Duration::from_millis(100), future).await;
                    log_err!(warn, res);
                    break;
                }
                _ => continue,
            };
        }
    }

    async fn read_data_point_ack_complete_loop(
        &self,
        mut recv: mpsc::Receiver<msg::DownstreamChunkAckComplete>,
    ) {
        while let Some(ack_comp) = recv.recv().await {
            self.process_ack_complete(ack_comp);
        }
    }

    async fn data_points_read_loop(
        &self,
        _wg: WaitGroup,
        mut receiver: mpsc::Receiver<msg::Message>,
        cmd_receiver: mpsc::Receiver<DataPointsSender>,
    ) {
        let (dps_s, dps_r) = mpsc::channel(1);
        let (res_s, res_r) = mpsc::channel(1024);

        let down = self.clone();
        tokio::task::spawn(async move { down.read_data_points_loop(dps_r, res_s).await });

        let (ack_comp_s, ack_comp_r) = mpsc::channel(1);
        let down = self.clone();
        tokio::task::spawn(async move { down.read_data_point_ack_complete_loop(ack_comp_r).await });

        let down = self.clone();
        tokio::task::spawn(async move { down.data_points_cmd_loop(cmd_receiver, res_r).await });

        while let Some(m) = receiver.recv().await {
            match m {
                msg::Message::DownstreamChunk(points) => {
                    log_err!(warn, dps_s.send(points).await);
                }
                msg::Message::DownstreamChunkAckComplete(ack) => {
                    log_err!(warn, ack_comp_s.send(ack).await);
                }
                _ => continue,
            };
        }
    }

    fn filter_metadata(&self, msg: msg::DownstreamMetadata) -> Option<DownstreamMetadata> {
        for id in self.state.source_node_ids.iter() {
            if id == &msg.source_node_id {
                return Some(DownstreamMetadata {
                    source_node_id: msg.source_node_id,
                    metadata: msg.metadata,
                });
            }
        }

        None
    }

    async fn metadata_cmd_loop(
        &self,
        mut cmd_receiver: mpsc::Receiver<MetadataSender>,
        mut metadata_receiver: mpsc::Receiver<DownstreamMetadata>,
    ) -> Result<()> {
        let mut metadata_to_send = None;
        while let Some(sender) = cmd_receiver.recv().await {
            let meta = if let Some(meta) = metadata_to_send.take() {
                meta
            } else {
                match metadata_receiver.recv().await {
                    Some(meta) => meta,
                    None => {
                        debug!("read loop closed, exit cmd_loop");
                        break;
                    }
                }
            };
            if let Err(meta) = sender.send(meta) {
                metadata_to_send = Some(meta);
            }
        }
        Ok(())
    }

    async fn metadata_read_loop(
        &self,
        _wg: WaitGroup,
        mut receiver: broadcast::Receiver<msg::DownstreamMetadata>,
        cmd_receiver: mpsc::Receiver<MetadataSender>,
    ) {
        let (s, r) = mpsc::channel(1);
        let down = self.clone();
        tokio::spawn(async move { down.metadata_cmd_loop(cmd_receiver, r).await });

        loop {
            let data = tokio::select! {
                _ = self.cancel.notified() => break,
                res = receiver.recv() => {
                    match res {
                       Ok(data) => data,
                       Err(err) => {
                           error!("{}", err);
                           continue
                       }
                    }
                }
            };
            if let Some(meta) = self.filter_metadata(data) {
                log_err!(debug, s.try_send(meta));
            }
        }
        trace!("exit metadata read loop");
    }

    fn process_data_points(
        &self,
        downstream_chunk: msg::DownstreamChunk,
    ) -> Result<Vec<DownstreamDataPoint>> {
        let info = match downstream_chunk.upstream_or_alias {
            msg::UpstreamOrAlias::UpstreamInfo(info) => {
                self.update_upstream_alias(&info);
                info
            }
            msg::UpstreamOrAlias::Alias(alias) => match self.state.upstreams_info.find(alias) {
                Some(info) => info,
                None => {
                    return Err(Error::failed_message(
                        msg::ResultCode::ProtocolError,
                        format!("invalid upstream alias: {}", alias),
                    ));
                }
            },
        };

        let mut data = Vec::new();
        for data_point_group in downstream_chunk.stream_chunk.data_point_groups.into_iter() {
            let id = match data_point_group.data_id_or_alias {
                msg::DataIdOrAlias::DataIdAlias(alias) => match self.state.data_ids.find(alias) {
                    Some(id) => id,
                    None => {
                        return Err(Error::failed_message(
                            msg::ResultCode::ProtocolError,
                            format!("invalid data id alias: {}", alias),
                        ))
                    }
                },
                msg::DataIdOrAlias::DataId(id) => {
                    self.update_data_id_alias(&id);
                    if let Err(e) = id.validate() {
                        return Err(Error::failed_message(msg::ResultCode::InvalidDataId, e));
                    }
                    id
                }
            };

            for data_point in data_point_group.data_points.into_iter() {
                data.push(super::DataPoint {
                    id: id.clone(),
                    payload: data_point.payload,
                    elapsed_time: chrono::Duration::nanoseconds(data_point.elapsed_time),
                });
            }
        }

        let sequence_number_in_upstream = downstream_chunk.stream_chunk.sequence_number;

        self.state.ack_buf.lock().unwrap().push({
            msg::DownstreamChunkResult {
                stream_id_of_upstream: info.stream_id,
                sequence_number_in_upstream,
                result_code: msg::ResultCode::Succeeded,
                result_string: "OK".to_string(),
            }
        });

        if self
            .state
            .update_last_recv_sequences(info.clone(), sequence_number_in_upstream)
            .is_none()
        {
            return Ok(Vec::new());
        }

        let res = data
            .into_iter()
            .map(|d| DownstreamDataPoint {
                data_point: d,
                upstream: info.clone(),
            })
            .collect::<Vec<_>>();

        Ok(res)
    }

    async fn close_waiter(&self, mut waiter: Waiter) {
        let mut conn_close_notified = self.conn.subscribe_close_notify();

        tokio::select! {
            _ = waiter.wait() => {
                info!("all tasks closed")
            },
            _ = conn_close_notified.recv() => {
                info!("conn closed")
            },
        };

        log_err!(error, self.repository.save_downstream(&self.info()));
        self.state.conn.store(ConnectionState::Closed);
        log_err!(trace, self.notify_close.send(()));
    }

    fn is_final_ack_sended(&self) -> bool {
        !self.state.is_open() && self.state.ack_buf.lock().unwrap().is_empty()
    }

    async fn send_ack(&self) -> Result<()> {
        let results = {
            let mut buf = self.state.ack_buf.lock().unwrap();
            if !buf.is_empty() {
                std::mem::take::<Vec<msg::DownstreamChunkResult>>(buf.as_mut())
            } else {
                Vec::new()
            }
        };

        let upstream_aliases = self.state.upstreams_info.want_send();
        let data_id_aliases = self.state.data_ids.want_send();

        if results.is_empty() && upstream_aliases.is_empty() && data_id_aliases.is_empty() {
            return Ok(());
        }

        self.conn
            .downstream_chunk_ack(msg::DownstreamChunkAck {
                ack_id: self.state.next_ack_id(),
                stream_id_alias: self.state.stream_id_alias,
                results,
                upstream_aliases,
                data_id_aliases,
            })
            .await
    }

    fn update_upstream_alias(&self, info: &msg::UpstreamInfo) {
        if let Some(alias) = self.state.upstreams_info.assign_alias(info) {
            debug!("assign alias num {} to upstream info {:?}", alias, info);
        }
    }

    fn update_data_id_alias(&self, id: &msg::DataId) {
        if id.validate().is_err() {
            return;
        }
        if let Some(alias) = self.state.data_ids.assign_alias(id) {
            debug!("assign alias num {} to data id {}", alias, id);
        }
    }

    async fn ack_flush_loop(&self, _wg: WaitGroup, interval: std::time::Duration) {
        let mut ticker = tokio::time::interval(interval);
        while !self.is_final_ack_sended() {
            ticker.tick().await;
            if let Err(e) = self.send_ack().await {
                error!("send ack: {}", e);
                break;
            }
        }
    }

    fn info(&self) -> super::DownstreamInfo {
        super::DownstreamInfo {
            stream_id: self.state.stream_id,
            ack_id: self.state.ack_id.load(Ordering::Acquire),
            source_node_ids: self.state.source_node_ids.clone(),
            qos: self.config.qos,
            data_ids: self.state.data_ids.map(),
            upstreams_info: self.state.upstreams_info.map(),
            last_recv_sequences: self.state.last_recv_sequences.lock().unwrap().clone(),
            server_time: self.state.server_time,
        }
    }
}

#[allow(clippy::vec_init_then_push)]
#[cfg(test)]
mod test {
    use tokio::sync::broadcast;

    use super::*;
    use crate::wire::{self, CloseNotificationReceiver};

    fn new_downstream_with_wire_conn(mut mock: wire::MockMockConnection) -> Downstream {
        let (notify_close, _) = broadcast::channel(1);
        let (data_points_cmd_sender, _) = mpsc::channel(1);
        let (metadata_cmd_sender, _) = mpsc::channel(1);

        let r = notify_close.subscribe();
        mock.expect_subscribe_close_notify()
            .return_once(|| CloseNotificationReceiver::new(r));

        Downstream {
            cancel: Cancel::new(),
            notify_close,
            conn: Arc::new(mock),
            state: Arc::default(),
            data_points_cmd_sender,
            metadata_cmd_sender,
            repository: Arc::new(super::super::InMemStreamRepository::new()),
            config: DownstreamConfig::default(),
        }
    }

    #[test]
    fn filter_metadata() {
        let mock = wire::MockMockConnection::new();
        let mut down = new_downstream_with_wire_conn(mock);
        down.state = Arc::new(State {
            source_node_ids: vec!["edge".to_string()],
            ..Default::default()
        });
        assert!(down
            .filter_metadata(msg::DownstreamMetadata {
                source_node_id: "edge".to_string(),
                ..Default::default()
            })
            .is_some());

        assert!(down
            .filter_metadata(msg::DownstreamMetadata {
                source_node_id: "hoge".to_string(),
                ..Default::default()
            })
            .is_none());
    }
}
