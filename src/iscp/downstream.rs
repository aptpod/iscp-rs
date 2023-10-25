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
    msg, Cancel, Conn, DataId, DataPoint, DataPointGroup, DownstreamChunk, DownstreamFilter,
    DownstreamMetadata, Error, QoS, Result, WaitGroup, Waiter,
};

/// ダウンストリームの設定です。
#[derive(Clone, Debug)]
pub struct DownstreamConfig {
    /// フィルターのリスト
    pub filters: Vec<DownstreamFilter>,
    /// 有効期限
    pub expiry_interval: chrono::Duration,
    /// QoS
    pub qos: QoS,
    /// データIDエイリアス
    pub data_ids: Vec<DataId>,
    /// Ackのインターバル
    pub ack_interval: chrono::Duration,
    /// 空チャンク省略フラグ。trueの場合、StreamChunk内のDataPointGroupが空の時、DownstreamChunkの送信を省略します。
    pub omit_empty_chunk: bool,
}

impl Default for DownstreamConfig {
    fn default() -> Self {
        Self {
            filters: Vec::new(),
            expiry_interval: chrono::Duration::zero(),
            qos: QoS::Unreliable,
            data_ids: Vec::new(),
            ack_interval: chrono::Duration::milliseconds(10),
            omit_empty_chunk: false,
        }
    }
}

impl msg::DownstreamOpenRequest {
    pub(crate) fn from_config(config: &DownstreamConfig) -> Self {
        let data_id_aliases: HashMap<u32, DataId> = config
            .data_ids
            .iter()
            .enumerate()
            .map(|(i, data_id)| ((i + 1) as u32, data_id.clone()))
            .collect();

        Self {
            expiry_interval: config.expiry_interval,
            downstream_filters: config.filters.clone(),
            qos: config.qos,
            data_id_aliases,
            omit_empty_chunk: config.omit_empty_chunk,
            ..Default::default()
        }
    }
}

/// ダウンストリームの状態です。
#[derive(Clone, Debug)]
pub struct DownstreamState {
    /// データIDエイリアスとデータIDのマップ
    pub data_id_aliases: HashMap<u32, DataId>,
    /// 最後に払い出されたデータIDエイリアス
    pub last_issued_data_id_alias: u32,
    /// アップストリームエイリアスとアップストリーム情報のマップ
    pub upstream_infos: HashMap<u32, msg::UpstreamInfo>,
    /// 最後に払い出されたアップストリーム情報のエイリアス
    pub last_issued_upstream_info_alias: u32,
    /// 最後に払い出されたAckのシーケンス番号
    pub last_issued_chunk_ack_id: u32,
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

/// 内部的なダウンストリームの状態
#[derive(Default, Debug)]
struct State {
    /// ID
    stream_id: Uuid,
    stream_id_alias: u32,

    /// DownstreamOpenResponseで返却されたサーバー時刻
    server_time: DateTime<Utc>,

    ack_id: AtomicU32,
    conn: AtomicCell<ConnectionState>,
    upstreams_info: AliasState<msg::UpstreamInfo>,
    data_ids: AliasState<msg::DataId>,
    ack_buf: Mutex<Vec<msg::DownstreamChunkResult>>,
    source_node_ids: Vec<String>,
    last_recv_sequences: Mutex<HashMap<msg::UpstreamInfo, u32>>,
    error_in_close: Mutex<Option<Error>>,
}

/// ダウンストリームのクローズイベントです。
#[derive(Clone, Debug)]
pub struct DownstreamClosedEvent {
    /// 切断したダウンストリームの設定
    pub config: Arc<DownstreamConfig>,
    /// 切断したダウンストリームの状態
    pub state: DownstreamState,
    /// 切断に失敗した場合のエラー情報
    pub error: Option<Error>,
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

type DownstreamChunkSender = oneshot::Sender<DownstreamChunk>;
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
    data_points_cmd_sender: mpsc::Sender<DownstreamChunkSender>,
    metadata_cmd_sender: mpsc::Sender<MetadataSender>,
    notify_close: broadcast::Sender<DownstreamClosedEvent>,
    repository: Arc<dyn super::DownstreamRepository>,
    config: Arc<DownstreamConfig>,
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
        config: Arc<DownstreamConfig>,
        data_id_aliases: HashMap<u32, DataId>,
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
                data_ids: AliasState::new(data_id_aliases),
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
        let ack_interval = down
            .config
            .ack_interval
            .to_std()
            .unwrap_or_else(|_| Duration::from_millis(10));
        tokio::spawn(async move {
            down_c.ack_flush_loop(wg, ack_interval).await;
        });

        let down_c = down.clone();
        tokio::spawn(async move { down_c.close_waiter(waiter).await });

        down
    }

    /// ダウンストリームデータポイントを受信します。
    pub async fn read_data_points(&self) -> Option<DownstreamChunk> {
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
    ///
    /// 内部のチャンネルサイズの制限により、ダウンストリームが1024個より多い場合、または処理しきれないほどのメタデータが送られた場合、メタデータが失われることがあります。
    pub async fn read_metadata(&self) -> Option<DownstreamMetadata> {
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
            let e = Error::from(resp);
            *self.state.error_in_close.lock().unwrap() = Some(e.clone());
            return Err(e);
        }

        log_err!(
            error,
            self.repository
                .remove_downstream_by_id(self.state.stream_id)
        );

        Ok(())
    }

    /// ダウンストリームの設定を返します。
    pub fn config(&self) -> Arc<DownstreamConfig> {
        self.config.clone()
    }

    /// ダウンストリームの状態を取得します
    pub fn state(&self) -> DownstreamState {
        let data_id_aliases = self.state.data_ids.map();
        let last_issued_data_id_alias = self.state.data_ids.alias.load(Ordering::Acquire) - 1;
        let upstream_infos = self.state.upstreams_info.map();
        let last_issued_upstream_info_alias =
            self.state.upstreams_info.alias.load(Ordering::Acquire) - 1;
        let last_issued_chunk_ack_id = self.state.ack_id.load(Ordering::Acquire);

        DownstreamState {
            data_id_aliases,
            last_issued_data_id_alias,
            upstream_infos,
            last_issued_upstream_info_alias,
            last_issued_chunk_ack_id,
        }
    }

    /// ストリームIDを取得します
    pub fn stream_id(&self) -> uuid::Uuid {
        self.state.stream_id
    }

    /// DownstreamOpenResponseで返却されたサーバー時刻を取得します
    pub fn server_time(&self) -> DateTime<Utc> {
        self.state.server_time
    }

    /// ダウンストリームのクローズをサブスクライブします。
    pub fn subscribe_close(&self) -> DownstreamClosedNotificationReceiver {
        DownstreamClosedNotificationReceiver(self.notify_close.subscribe())
    }
}

impl Downstream {
    async fn data_points_cmd_loop(
        &self,
        mut cmd_receiver: mpsc::Receiver<DownstreamChunkSender>,
        mut data_points_receiver: mpsc::Receiver<DownstreamChunk>,
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
        sender: mpsc::Sender<DownstreamChunk>,
    ) {
        while let Some(received_chunk) = recv.recv().await {
            let err = match self.process_data_points(received_chunk) {
                Ok(data) => {
                    log_err!(warn, sender.try_send(data));
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
        cmd_receiver: mpsc::Receiver<DownstreamChunkSender>,
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
            if id == &msg.source_node_id && self.state.stream_id_alias == msg.stream_id_alias {
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
                           error!("cannot receive in metadata read loop: {}", err);
                           break;
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
    ) -> Result<DownstreamChunk> {
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

        let mut data_point_groups = Vec::new();
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

            let data_points: Vec<DataPoint> = data_point_group
                .data_points
                .into_iter()
                .map(|dp| DataPoint {
                    payload: dp.payload,
                    elapsed_time: chrono::Duration::nanoseconds(dp.elapsed_time),
                })
                .collect();

            data_point_groups.push(DataPointGroup { id, data_points });
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
            return Ok(DownstreamChunk {
                sequence_number: downstream_chunk.stream_chunk.sequence_number,
                data_point_groups: vec![],
                upstream: info,
            });
        }

        Ok(DownstreamChunk {
            sequence_number: downstream_chunk.stream_chunk.sequence_number,
            data_point_groups,
            upstream: info,
        })
    }

    async fn close_waiter(&self, mut waiter: Waiter) {
        let mut conn_disconnect_notified = self.conn.subscribe_disconnect_notify();

        tokio::select! {
            _ = waiter.wait() => {
                info!("all tasks closed")
            },
            _ = conn_disconnect_notified.recv() => {
                info!("conn closed")
            },
        };

        log_err!(error, self.repository.save_downstream(&self.info()));
        self.state.conn.store(ConnectionState::Closed);
        let closed_event = DownstreamClosedEvent {
            config: self.config.clone(),
            state: self.state(),
            error: self
                .state
                .error_in_close
                .lock()
                .ok()
                .and_then(|e| e.clone()),
        };
        log_err!(trace, self.notify_close.send(closed_event));
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

    async fn ack_flush_loop(&self, _wg: WaitGroup, ack_interval: std::time::Duration) {
        let mut ticker = tokio::time::interval(ack_interval);
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

/// ダウンストリームのクローズイベントを受け取るレシーバーです。
#[derive(Debug)]
pub struct DownstreamClosedNotificationReceiver(broadcast::Receiver<DownstreamClosedEvent>);

impl DownstreamClosedNotificationReceiver {
    pub async fn recv(&mut self) -> Option<DownstreamClosedEvent> {
        self.0.recv().await.ok()
    }
}

/// ダウンストリームのビルダーです。
pub struct DownstreamBuilder<'a> {
    pub(crate) conn: &'a Conn,
    pub(crate) config: DownstreamConfig,
}

impl<'a> DownstreamBuilder<'a> {
    /// ダウンストリームのオープン
    pub async fn build(self) -> Result<Downstream> {
        self.conn.open_downstream(self.config).await
    }

    /// 有効期限
    pub fn expiry_interval(mut self, expiry_interval: chrono::Duration) -> Self {
        self.config.expiry_interval = expiry_interval;
        self
    }

    /// QoS
    pub fn qos(mut self, qos: QoS) -> Self {
        self.config.qos = qos;
        self
    }

    /// データIDエイリアス
    pub fn data_ids(mut self, data_ids: Vec<DataId>) -> Self {
        self.config.data_ids = data_ids;
        self
    }

    /// Ackのインターバル
    pub fn ack_interval(mut self, ack_interval: chrono::Duration) -> Self {
        self.config.ack_interval = ack_interval;
        self
    }

    /// 空チャンク省略フラグ。trueの場合、StreamChunk内のDataPointGroupが空の時、DownstreamChunkの送信を省略します。
    pub fn omit_empty_chunk(mut self, omit_empty_chunk: bool) -> Self {
        self.config.omit_empty_chunk = omit_empty_chunk;
        self
    }
}

#[allow(clippy::vec_init_then_push)]
#[cfg(test)]
mod test {
    use tokio::sync::broadcast;

    use super::*;
    use crate::wire::{self, DisconnectNotificationReceiver};

    fn new_downstream_with_wire_conn(mock: wire::MockMockConnection) -> Downstream {
        let (notify_close, _) = broadcast::channel(1);
        let (data_points_cmd_sender, _) = mpsc::channel(1);
        let (metadata_cmd_sender, _) = mpsc::channel(1);

        Downstream {
            cancel: Cancel::new(),
            notify_close,
            conn: Arc::new(mock),
            state: Arc::default(),
            data_points_cmd_sender,
            metadata_cmd_sender,
            repository: Arc::new(super::super::InMemStreamRepository::new()),
            config: Arc::new(DownstreamConfig::default()),
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

    #[test]
    fn alias_update() {
        struct Case {
            name: String,
            init_state: State,
            points: msg::DownstreamChunk,
            want_id_aliases: HashMap<u32, msg::DataId>,
            want_upstream_aliases: HashMap<u32, msg::UpstreamInfo>,
        }

        let mut cases = Vec::new();
        cases.push(Case {
            name: "exist upstream alias".to_string(),
            init_state: State {
                upstreams_info: AliasState {
                    alias: AtomicU32::new(1),
                    map: Mutex::new(
                        [(
                            1,
                            msg::UpstreamInfo {
                                source_node_id: "source_node_id".to_string(),
                                stream_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111")
                                    .unwrap(),
                                session_id: "session_id".to_string(),
                            },
                        )]
                        .iter()
                        .cloned()
                        .collect(),
                    ),
                    buf: Mutex::default(),
                },
                ..Default::default()
            },
            points: msg::DownstreamChunk {
                upstream_or_alias: 1.into(),
                stream_chunk: msg::StreamChunk {
                    sequence_number: 1,
                    data_point_groups: vec![msg::DataPointGroup {
                        data_id_or_alias: msg::DataId::new("1", "1").into(),
                        data_points: vec![],
                    }],
                },
                ..Default::default()
            },
            want_id_aliases: [(
                1,
                msg::DataId {
                    name: "1".to_string(),
                    type_: "1".to_string(),
                },
            )]
            .iter()
            .cloned()
            .collect(),
            want_upstream_aliases: [(
                1,
                msg::UpstreamInfo {
                    source_node_id: "source_node_id".to_string(),
                    stream_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
                    session_id: "session_id".to_string(),
                },
            )]
            .iter()
            .cloned()
            .collect(),
        });

        cases.push(Case {
            name: "exist data id alias".to_string(),
            init_state: State {
                data_ids: AliasState {
                    alias: AtomicU32::new(1),
                    map: Mutex::new(
                        [(
                            1,
                            msg::DataId {
                                name: "1".to_string(),
                                type_: "1".to_string(),
                            },
                        )]
                        .iter()
                        .cloned()
                        .collect(),
                    ),
                    buf: Mutex::default(),
                },
                ..Default::default()
            },
            points: msg::DownstreamChunk {
                upstream_or_alias: msg::UpstreamInfo {
                    source_node_id: "source_node_id".to_string(),
                    stream_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
                    session_id: "session_id".to_string(),
                }
                .into(),
                stream_chunk: msg::StreamChunk {
                    sequence_number: 1,
                    data_point_groups: vec![msg::DataPointGroup {
                        data_points: vec![],
                        data_id_or_alias: msg::DataId::new("1", "1").into(),
                    }],
                },
                ..Default::default()
            },
            want_id_aliases: [(1, msg::DataId::new("1", "1"))].iter().cloned().collect(),
            want_upstream_aliases: [(
                1,
                msg::UpstreamInfo {
                    source_node_id: "source_node_id".to_string(),
                    stream_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
                    session_id: "session_id".to_string(),
                },
            )]
            .iter()
            .cloned()
            .collect(),
        });
        cases.push(Case {
            name: "exist data id and upstream alias".to_string(),
            init_state: State {
                data_ids: AliasState {
                    alias: AtomicU32::new(1),
                    map: Mutex::new([(1, msg::DataId::new("1", "1"))].iter().cloned().collect()),
                    buf: Mutex::default(),
                },
                upstreams_info: AliasState {
                    alias: AtomicU32::new(1),
                    map: Mutex::new(
                        [(
                            1,
                            msg::UpstreamInfo {
                                source_node_id: "source_node_id".to_string(),
                                stream_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111")
                                    .unwrap(),
                                session_id: "session_id".to_string(),
                            },
                        )]
                        .iter()
                        .cloned()
                        .collect(),
                    ),
                    buf: Mutex::default(),
                },
                ..Default::default()
            },
            points: msg::DownstreamChunk {
                upstream_or_alias: 1.into(),
                stream_chunk: msg::StreamChunk {
                    sequence_number: 1,
                    data_point_groups: vec![msg::DataPointGroup {
                        data_points: vec![],
                        data_id_or_alias: 1.into(),
                    }],
                },
                ..Default::default()
            },
            want_id_aliases: [(1, msg::DataId::new("1", "1"))].iter().cloned().collect(),
            want_upstream_aliases: [(
                1,
                msg::UpstreamInfo {
                    source_node_id: "source_node_id".to_string(),
                    stream_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
                    session_id: "session_id".to_string(),
                },
            )]
            .iter()
            .cloned()
            .collect(),
        });

        impl Case {
            async fn test(self) {
                let mock_conn = wire::MockMockConnection::new();
                let mut down = new_downstream_with_wire_conn(mock_conn);
                down.state = Arc::new(self.init_state);

                down.process_data_points(self.points.clone())
                    .expect(&self.name);

                {
                    let upstream_aliases = down.state.upstreams_info.map.lock().unwrap();
                    let data_id_aliases = down.state.data_ids.map.lock().unwrap();

                    assert_eq!(*upstream_aliases, self.want_upstream_aliases);
                    assert_eq!(*data_id_aliases, self.want_id_aliases);
                }
                assert_eq!(down.state().data_id_aliases, self.want_id_aliases);
            }
        }

        let run = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        cases.into_iter().for_each(|case| run.block_on(case.test()));
    }

    #[test]
    fn process_data_points() {
        struct Case {
            input: msg::DownstreamChunk,
            state: Arc<State>,
            want: DownstreamChunk,
            want_result: Result<msg::DownstreamChunkResult>,
        }
        impl Case {
            fn test(&self) {
                let mut down = new_downstream_with_wire_conn(wire::MockMockConnection::new());
                down.state = self.state.clone();
                let res = down.process_data_points(self.input.to_owned());
                if self.want_result.is_err() {
                    assert!(res.is_err());
                    return;
                } else {
                    assert!(res.is_ok());
                    let got_points = res.unwrap();
                    assert_eq!(got_points, self.want);
                }

                if let Ok(result) = &self.want_result {
                    assert_eq!(
                        result.clone(),
                        *down.state.ack_buf.lock().unwrap().first().unwrap()
                    );
                }
            }
        }

        let mut cases = Vec::new();
        // ok: no alias
        cases.push(Case {
            input: msg::DownstreamChunk {
                upstream_or_alias: msg::UpstreamInfo {
                    source_node_id: "source_node_id".to_string(),
                    stream_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
                    session_id: "session".to_string(),
                }
                .into(),
                stream_chunk: msg::StreamChunk {
                    sequence_number: 1,
                    data_point_groups: vec![msg::DataPointGroup {
                        data_id_or_alias: msg::DataId::new("name", "type").into(),
                        data_points: vec![msg::DataPoint {
                            elapsed_time: chrono::Duration::seconds(1).num_nanoseconds().unwrap(),
                            payload: vec![1, 2, 3, 4].into(),
                        }],
                    }],
                },
                ..Default::default()
            },
            state: Arc::default(),
            want: DownstreamChunk {
                sequence_number: 1,
                data_point_groups: vec![DataPointGroup {
                    id: DataId::new("name", "type"),
                    data_points: vec![DataPoint {
                        payload: vec![1, 2, 3, 4].into(),
                        elapsed_time: chrono::Duration::seconds(1),
                    }],
                }],
                upstream: msg::UpstreamInfo {
                    source_node_id: "source_node_id".to_string(),
                    stream_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
                    session_id: "session".to_string(),
                },
            },
            want_result: Ok(msg::DownstreamChunkResult {
                sequence_number_in_upstream: 1,
                stream_id_of_upstream: Uuid::parse_str("11111111-1111-1111-1111-111111111111")
                    .unwrap(),
                result_code: msg::ResultCode::Succeeded,
                result_string: "OK".to_string(),
            }),
        });
        // ok: exist alias
        cases.push(Case {
            input: msg::DownstreamChunk {
                upstream_or_alias: 1.into(),
                stream_chunk: msg::StreamChunk {
                    sequence_number: 1,
                    data_point_groups: vec![msg::DataPointGroup {
                        data_id_or_alias: 1.into(),
                        data_points: vec![msg::DataPoint {
                            elapsed_time: chrono::Duration::seconds(1).num_nanoseconds().unwrap(),
                            payload: vec![1, 2, 3, 4].into(),
                        }],
                    }],
                },
                ..Default::default()
            },
            state: Arc::new(State {
                data_ids: AliasState {
                    map: Mutex::new(
                        [(1, msg::DataId::new("name", "type"))]
                            .iter()
                            .cloned()
                            .collect(),
                    ),
                    ..Default::default()
                },
                upstreams_info: AliasState {
                    map: Mutex::new(
                        [(
                            1,
                            msg::UpstreamInfo {
                                source_node_id: "source_node_id".to_string(),
                                stream_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111")
                                    .unwrap(),
                                session_id: "session".to_string(),
                            },
                        )]
                        .iter()
                        .cloned()
                        .collect(),
                    ),
                    ..Default::default()
                },
                ..Default::default()
            }),
            want: DownstreamChunk {
                sequence_number: 1,
                data_point_groups: vec![DataPointGroup {
                    id: DataId::new("name", "type"),
                    data_points: vec![DataPoint {
                        payload: vec![1, 2, 3, 4].into(),
                        elapsed_time: chrono::Duration::seconds(1),
                    }],
                }],
                upstream: msg::UpstreamInfo {
                    source_node_id: "source_node_id".to_string(),
                    stream_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
                    session_id: "session".to_string(),
                },
            },
            want_result: Ok(msg::DownstreamChunkResult {
                sequence_number_in_upstream: 1,
                stream_id_of_upstream: Uuid::parse_str("11111111-1111-1111-1111-111111111111")
                    .unwrap(),
                result_code: msg::ResultCode::Succeeded,
                result_string: "OK".to_string(),
            }),
        });
        // ng: not exist upstream alias
        cases.push(Case {
            input: msg::DownstreamChunk {
                upstream_or_alias: 1.into(),
                stream_chunk: msg::StreamChunk {
                    sequence_number: 1,
                    data_point_groups: vec![msg::DataPointGroup {
                        data_id_or_alias: 1.into(),
                        data_points: vec![msg::DataPoint {
                            elapsed_time: chrono::Duration::seconds(1).num_nanoseconds().unwrap(),
                            payload: vec![1, 2, 3, 4].into(),
                        }],
                    }],
                },
                ..Default::default()
            },
            state: Arc::new(State {
                data_ids: AliasState {
                    map: Mutex::new(
                        [(1, msg::DataId::new("name", "type"))]
                            .iter()
                            .cloned()
                            .collect(),
                    ),
                    ..Default::default()
                },
                upstreams_info: AliasState::default(),
                ..Default::default()
            }),
            want: DownstreamChunk::default(),
            want_result: Err(Error::FailedMessage {
                code: msg::ResultCode::ProtocolError,
                detail: "".into(),
            }),
        });
        // ng: not exist data id alias
        cases.push(Case {
            input: msg::DownstreamChunk {
                upstream_or_alias: 1.into(),
                stream_chunk: msg::StreamChunk {
                    sequence_number: 1,
                    data_point_groups: vec![msg::DataPointGroup {
                        data_id_or_alias: 1.into(),
                        data_points: vec![msg::DataPoint {
                            elapsed_time: chrono::Duration::seconds(1).num_nanoseconds().unwrap(),
                            payload: vec![1, 2, 3, 4].into(),
                        }],
                    }],
                },
                ..Default::default()
            },
            state: Arc::new(State {
                data_ids: AliasState::default(),
                upstreams_info: AliasState {
                    map: Mutex::new(
                        [(
                            1,
                            msg::UpstreamInfo {
                                source_node_id: "source_node_id".to_string(),
                                stream_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111")
                                    .unwrap(),
                                session_id: "session".to_string(),
                            },
                        )]
                        .iter()
                        .cloned()
                        .collect(),
                    ),
                    ..Default::default()
                },
                ..Default::default()
            }),
            want: DownstreamChunk::default(),
            want_result: Err(Error::FailedMessage {
                code: msg::ResultCode::ProtocolError,
                detail: "".into(),
            }),
        });

        cases.into_iter().for_each(|case| case.test());
    }

    #[test]
    fn send_ack() {
        struct Case {
            state: Arc<State>,
            want: Option<msg::DownstreamChunkAck>,
        }
        impl Case {
            async fn test(&self) {
                let mut mock = wire::MockMockConnection::new();
                if let Some(want) = self.want.clone() {
                    mock.expect_downstream_chunk_ack()
                        .return_once(move |got_ack| {
                            assert_eq!(got_ack, want);
                            Ok(())
                        });
                }

                let mut down = new_downstream_with_wire_conn(mock);
                down.state = self.state.clone();
                down.send_ack().await.unwrap();
                assert!(down.state.ack_buf.lock().unwrap().is_empty());
            }
        }
        let cases = vec![
            Case {
                state: Arc::new(State {
                    stream_id_alias: 1,
                    ack_buf: Mutex::new(vec![msg::DownstreamChunkResult {
                        stream_id_of_upstream: Uuid::parse_str(
                            "11111111-1111-1111-1111-111111111111",
                        )
                        .unwrap(),
                        sequence_number_in_upstream: 100,
                        result_code: msg::ResultCode::Succeeded,
                        result_string: "OK".to_string(),
                    }]),
                    ..Default::default()
                }),
                want: Some(msg::DownstreamChunkAck {
                    stream_id_alias: 1,
                    results: vec![msg::DownstreamChunkResult {
                        stream_id_of_upstream: Uuid::parse_str(
                            "11111111-1111-1111-1111-111111111111",
                        )
                        .unwrap(),
                        sequence_number_in_upstream: 100,
                        result_code: msg::ResultCode::Succeeded,
                        result_string: "OK".to_string(),
                    }],
                    ..Default::default()
                }),
            },
            Case {
                state: Arc::default(),
                want: None,
            },
        ];
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        for case in cases.into_iter() {
            rt.block_on(async { case.test().await });
        }
    }

    #[test]
    fn receive_invalid_alias() {
        struct Case {
            input: msg::DownstreamChunk,
            want_disconnect: bool,
        }
        impl Case {
            async fn test(&self) {
                let mut mock = wire::MockMockConnection::new();
                if self.want_disconnect {
                    mock.expect_disconnect().return_once(|_| Ok(()));
                }
                let (_notify, r) = broadcast::channel(1);

                mock.expect_subscribe_disconnect_notify()
                    .return_once(|| DisconnectNotificationReceiver::new(r));

                let (sender, data_points_subscriber) = mpsc::channel(1);
                let (_meta_sender, metadata_subscriber) = broadcast::channel(1);
                let down = Downstream::new(
                    Arc::new(mock),
                    Arc::new(DownstreamConfig {
                        qos: QoS::Unreliable,
                        ..Default::default()
                    }),
                    HashMap::default(),
                    DownstreamParam {
                        stream_id: Uuid::nil(),
                        stream_id_alias: 1,
                        data_points_subscriber,
                        metadata_subscriber,
                        source_node_ids: vec![],
                        repository: Arc::new(super::super::InMemStreamRepository::new()),
                        server_time: <Utc as chrono::TimeZone>::timestamp_opt(
                            &Utc,
                            123_456_789,
                            123_456_789,
                        )
                        .unwrap(),
                    },
                );

                let down_c = down.clone();
                tokio::spawn(async move { down_c.read_data_points().await });
                sender.send(self.input.clone().into()).await.unwrap();
            }
        }
        let cases = vec![
            Case {
                input: msg::DownstreamChunk {
                    upstream_or_alias: 100.into(),
                    ..Default::default()
                },
                want_disconnect: true,
            },
            Case {
                input: msg::DownstreamChunk {
                    upstream_or_alias: msg::UpstreamInfo::default().into(),
                    stream_chunk: msg::StreamChunk {
                        sequence_number: 1,
                        data_point_groups: vec![msg::DataPointGroup {
                            data_points: vec![],
                            data_id_or_alias: 100.into(),
                        }],
                    },
                    ..Default::default()
                },
                want_disconnect: true,
            },
            Case {
                input: msg::DownstreamChunk {
                    upstream_or_alias: msg::UpstreamInfo::default().into(),
                    stream_chunk: msg::StreamChunk {
                        sequence_number: 1,
                        data_point_groups: vec![msg::DataPointGroup {
                            data_points: vec![],
                            data_id_or_alias: msg::DataId::new("name", "type").into(),
                        }],
                    },
                    ..Default::default()
                },
                want_disconnect: false,
            },
        ];

        let rt = tokio::runtime::Runtime::new().unwrap();

        for case in cases.into_iter() {
            rt.block_on(case.test());
        }
    }
}
