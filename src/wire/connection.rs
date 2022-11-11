use crate::{
    error::{Error, Result},
    message::{self as msg, Disconnect, Message, RequestId},
    sync::{Cancel, Waiter},
};

use super::{BoxedUnreliableTransport, CloseNotificationReceiver, Connection, Transport};

use log::*;
use std::collections::{hash_map::Entry, HashMap};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::{broadcast, mpsc, oneshot};

type ReplySender = oneshot::Sender<Message>;

static DEFAULT_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug)]
struct State {
    connected: AtomicBool,
    request_id: Mutex<RequestId>,
    waiting_reply: Mutex<HashMap<RequestId, ReplySender>>,
    waiting_e2e_ack: Mutex<HashMap<String, ReplySender>>,
    upstream_subscribers: Mutex<Subscribers<Message>>,
    upstream_qos: Mutex<HashMap<u32, msg::QoS>>,
    upstream_alias: Mutex<HashMap<uuid::Uuid, u32>>,
    downstream_subscribers: Mutex<Subscribers<Message>>,
    unreliable_downstream_subscribers: Mutex<Subscribers<Message>>,
    broadcast_downstream_meta: broadcast::Sender<msg::DownstreamMetadata>,
    broadcast_downstream_call: broadcast::Sender<msg::DownstreamCall>,
}

#[derive(Debug)]
struct Subscribers<T>(HashMap<u32, mpsc::Sender<T>>);

impl<T> Default for Subscribers<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Subscribers<T> {
    pub fn new() -> Self {
        Self(HashMap::new())
    }
    pub fn add(&mut self, alias: u32) -> Result<mpsc::Receiver<T>> {
        match self.0.entry(alias) {
            Entry::Occupied(_) => Err(Error::unexpected("subscriber already exist")),
            Entry::Vacant(e) => {
                let (sender, receiver) = mpsc::channel(1);
                e.insert(sender);
                Ok(receiver)
            }
        }
    }
    pub fn find(&mut self, alias: u32) -> Option<mpsc::Sender<T>> {
        match self.0.entry(alias) {
            Entry::Vacant(_) => None,
            Entry::Occupied(e) => Some(e.get().clone()),
        }
    }
    pub fn remove(&mut self, alias: u32) -> Result<()> {
        match self.0.entry(alias) {
            Entry::Occupied(e) => {
                e.remove();
                Ok(())
            }
            Entry::Vacant(_) => Err(Error::unexpected("NotFound")),
        }
    }
    pub fn clear(&mut self) {
        self.0.clear()
    }
}

impl State {
    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }
    fn check_connected(&self) -> Result<()> {
        if !self.is_connected() {
            return Err(Error::ConnectionClosed("".into()));
        }
        Ok(())
    }
    fn next_request_id(&self) -> RequestId {
        let mut id = self.request_id.lock().unwrap();
        let res = id.value().into();
        id.increment();
        res
    }
    fn add_waiting_reply(&self, id: &RequestId, sender: ReplySender) {
        let mut waiting = self.waiting_reply.lock().unwrap();
        waiting.insert(*id, sender);
    }
    fn add_waiting_e2e_ack(&self, call_id: &str, sender: ReplySender) {
        let mut waiting = self.waiting_e2e_ack.lock().unwrap();
        waiting.insert(call_id.to_string(), sender);
    }

    fn remove_waiting_reply(&self, id: &RequestId) -> Option<ReplySender> {
        let mut waiting = self.waiting_reply.lock().unwrap();
        waiting.remove(id)
    }

    fn remove_waiting_e2e_ack(&self, call_id: &str) -> Option<ReplySender> {
        let mut waiting = self.waiting_e2e_ack.lock().unwrap();
        waiting.remove(call_id)
    }

    pub fn add_upstream_subscriber(&self, alias: u32) -> Result<mpsc::Receiver<Message>> {
        self.upstream_subscribers.lock().unwrap().add(alias)
    }
    pub fn find_upstream_subscriber(&self, alias: u32) -> Option<mpsc::Sender<Message>> {
        self.upstream_subscribers.lock().unwrap().find(alias)
    }
    pub fn remove_upstream_subscriber(&self, alias: u32) -> Result<()> {
        self.upstream_subscribers.lock().unwrap().remove(alias)
    }

    pub fn add_downstream_subscriber(&self, alias: u32) -> Result<mpsc::Receiver<Message>> {
        self.downstream_subscribers.lock().unwrap().add(alias)
    }
    pub fn find_downstream_subscriber(&self, alias: u32) -> Option<mpsc::Sender<Message>> {
        self.downstream_subscribers.lock().unwrap().find(alias)
    }
    pub fn remove_downstream_subscriber(&self, alias: u32) -> Result<()> {
        self.downstream_subscribers.lock().unwrap().remove(alias)
    }

    pub fn add_unreliable_downstream_subscriber(
        &self,
        alias: u32,
    ) -> Result<mpsc::Receiver<Message>> {
        self.unreliable_downstream_subscribers
            .lock()
            .unwrap()
            .add(alias)
    }
    pub fn find_unreliable_downstream_subscriber(
        &self,
        alias: u32,
    ) -> Option<mpsc::Sender<Message>> {
        self.unreliable_downstream_subscribers
            .lock()
            .unwrap()
            .find(alias)
    }
    pub fn remove_unreliable_downstream_subscriber(&self, alias: u32) -> Result<()> {
        self.unreliable_downstream_subscribers
            .lock()
            .unwrap()
            .remove(alias)
    }
    pub fn remove_all_subscriber(&self) {
        self.upstream_subscribers.lock().unwrap().clear();
        self.downstream_subscribers.lock().unwrap().clear();
        self.unreliable_downstream_subscribers
            .lock()
            .unwrap()
            .clear();
    }

    pub fn register_upstream_alias(&self, uuid: uuid::Uuid, alias: u32) -> Result<()> {
        match self.upstream_alias.lock().unwrap().entry(uuid) {
            Entry::Occupied(_) => Err(Error::unexpected("already exist")),
            Entry::Vacant(e) => {
                e.insert(alias);
                Ok(())
            }
        }
    }
    pub fn deregister_upstream_alias(&self, uuid: &uuid::Uuid) -> Option<u32> {
        self.upstream_alias.lock().unwrap().remove(uuid)
    }

    pub fn register_upstream_qos(&self, alias: u32, qos: msg::QoS) -> Result<()> {
        match self.upstream_qos.lock().unwrap().entry(alias) {
            Entry::Occupied(_) => Err(Error::unexpected("already exist")),
            Entry::Vacant(e) => {
                e.insert(qos);
                Ok(())
            }
        }
    }

    pub fn find_upstream_qos(&self, alias: &u32) -> Result<msg::QoS> {
        match self.upstream_qos.lock().unwrap().get(alias) {
            Some(q) => Ok(*q),
            None => Err(Error::unexpected("not found")),
        }
    }

    pub fn deregister_upstream_qos(&self, alias: &u32) -> Option<msg::QoS> {
        self.upstream_qos.lock().unwrap().remove(alias)
    }
}

pub struct Conn<W: Transport> {
    cancel: Cancel,
    state: Arc<State>,
    tr: Arc<W>,
    unreliable_tr: Option<Arc<BoxedUnreliableTransport>>,
    request_timeout: Duration,
    close_notify: broadcast::Sender<()>,
}

impl<T> Clone for Conn<T>
where
    T: Transport,
{
    fn clone(&self) -> Self {
        Self {
            cancel: self.cancel.clone(),
            state: self.state.clone(),
            tr: self.tr.clone(),
            unreliable_tr: self.unreliable_tr.clone(),
            request_timeout: self.request_timeout,
            close_notify: self.close_notify.clone(),
        }
    }
}

#[async_trait]
impl<T> Connection for Conn<T>
where
    T: Transport + 'static,
{
    fn subscribe_close_notify(&self) -> CloseNotificationReceiver {
        self.subscribe_close_notify()
    }
    async fn close(&self) -> Result<()> {
        self.close().await
    }
    async fn open_request(&self, msg: msg::ConnectRequest) -> Result<msg::ConnectResponse> {
        self.open_request(msg).await
    }
    async fn disconnect(&self, msg: msg::Disconnect) -> Result<()> {
        self.disconnect(msg).await
    }

    async fn upstream_open_request(
        &self,
        msg: msg::UpstreamOpenRequest,
    ) -> Result<msg::UpstreamOpenResponse> {
        self.upstream_open_request(msg).await
    }

    async fn upstream_resume_request(
        &self,
        msg: msg::UpstreamResumeRequest,
        qos: msg::QoS,
    ) -> Result<msg::UpstreamResumeResponse> {
        self.upstream_resume_request(msg, qos).await
    }

    async fn upstream_metadata(
        &self,
        msg: msg::UpstreamMetadata,
    ) -> Result<msg::UpstreamMetadataAck> {
        self.upstream_metadata(msg).await
    }

    async fn upstream_chunk(&self, msg: msg::UpstreamChunk) -> Result<()> {
        self.upstream_data_points(msg).await
    }

    async fn upstream_close_request(
        &self,
        msg: msg::UpstreamCloseRequest,
    ) -> Result<msg::UpstreamCloseResponse> {
        self.upstream_close_request(msg).await
    }

    async fn downstream_open_request(
        &self,
        msg: msg::DownstreamOpenRequest,
    ) -> Result<msg::DownstreamOpenResponse> {
        self.downstream_open_request(msg).await
    }
    async fn downstream_resume_request(
        &self,
        msg: msg::DownstreamResumeRequest,
    ) -> Result<msg::DownstreamResumeResponse> {
        self.downstream_resume_request(msg).await
    }
    async fn downstream_close_request(
        &self,
        msg: msg::DownstreamCloseRequest,
    ) -> Result<msg::DownstreamCloseResponse> {
        self.downstream_close_request(msg).await
    }
    async fn downstream_chunk_ack(&self, msg: msg::DownstreamChunkAck) -> Result<()> {
        self.downstream_data_points_ack(msg).await
    }

    async fn upstream_call(&self, msg: msg::UpstreamCall) -> Result<msg::UpstreamCallAck> {
        self.upstream_call(msg).await
    }

    fn subscribe_upstream(&self, stream_id_alias: u32) -> Result<mpsc::Receiver<msg::Message>> {
        self.subscribe_upstream(stream_id_alias)
    }
    fn unsubscribe_upstream(&self, stream_id_alias: u32) -> Result<()> {
        self.unsubscribe_upstream(stream_id_alias)
    }

    fn subscribe_downstream(
        &self,
        stream_id_alias: u32,
        qos: msg::QoS,
    ) -> Result<mpsc::Receiver<msg::Message>> {
        self.subscribe_downstream(stream_id_alias, qos)
    }
    fn unsubscribe_downstream(&self, stream_id_alias: u32) -> Result<()> {
        self.unsubscribe_downstream(stream_id_alias)
    }
    fn subscribe_downstream_meta(&self) -> broadcast::Receiver<msg::DownstreamMetadata> {
        self.subscribe_downstream_meta()
    }
    fn unsubscribe_downstream_meta(&self) {
        self.unsubscribe_downstream_meta()
    }

    fn subscribe_downstream_call(&self) -> broadcast::Receiver<msg::DownstreamCall> {
        self.subscribe_downstream_call()
    }

    fn unsubscribe_downstream_call(&self) {
        // do nothing
    }
}

pub async fn connect<T>(
    tr: T,
    unreliable_tr: Option<BoxedUnreliableTransport>,
    request_timeout: Option<Duration>,
) -> Result<Conn<T>>
where
    T: Transport + 'static,
{
    let (waiter, wg) = crate::sync::WaitGroup::new();
    let (close_notify, _) = broadcast::channel(1);
    let (broadcast_downstream_meta, _) = broadcast::channel(1);
    let (broadcast_downstream_call, _) = broadcast::channel(1);

    let utr = unreliable_tr.map(Arc::new);

    let conn = Conn {
        cancel: Cancel::new(),
        tr: Arc::new(tr),
        unreliable_tr: utr,
        request_timeout: request_timeout.unwrap_or(DEFAULT_TIMEOUT),
        state: Arc::new(State {
            connected: AtomicBool::new(true),
            request_id: Mutex::new(RequestId::new_as_edge()),
            waiting_reply: Mutex::default(),
            upstream_qos: Mutex::default(),
            upstream_subscribers: Mutex::default(),
            upstream_alias: Mutex::default(),
            downstream_subscribers: Mutex::default(),
            unreliable_downstream_subscribers: Mutex::default(),
            waiting_e2e_ack: Mutex::default(),
            broadcast_downstream_meta,
            broadcast_downstream_call,
        }),
        close_notify,
    };

    let conn_c = conn.clone();
    let wg_c = wg.clone();
    tokio::spawn(async move {
        log_err!(error, conn_c.read_loop(wg_c).await);
        trace!("exit wire conn read loop")
    });

    let conn_c = conn.clone();
    tokio::spawn(async move {
        log_err!(error, conn_c.unreliable_read_loop(wg).await);
        trace!("exit wire conn unreliable read loop")
    });

    let conn_c = conn.clone();
    tokio::spawn(async move { conn_c.wait_close(waiter).await });

    Ok(conn)
}

// public method
impl<T> Conn<T>
where
    T: Transport + 'static,
{
    pub async fn close(&self) -> Result<()> {
        if !self.state.is_connected() {
            return Ok(());
        }
        debug!("close connection");
        let mut close_notified = self.close_notify.subscribe();

        log_err!(trace, self.cancel.notify());
        log_err!(trace, close_notified.recv().await);
        log_err!(warn, self.tr.close().await);
        Ok(())
    }

    pub fn subscribe_close_notify(&self) -> CloseNotificationReceiver {
        CloseNotificationReceiver::new(self.close_notify.subscribe())
    }

    pub async fn open_request(&self, mut msg: msg::ConnectRequest) -> Result<msg::ConnectResponse> {
        msg.request_id = self.state.next_request_id();
        debug!("open: {:?}", msg);
        let timeout = msg.ping_timeout.clone().to_std()?;

        if timeout < Duration::from_secs(1) {
            return Err(Error::invalid_value(
                "ping_timeout must be grater then 1 sec",
            ));
        }

        let interval = msg.ping_interval.clone().to_std()?;
        if interval < Duration::from_secs(1) {
            return Err(Error::invalid_value(
                "ping_interval must be grater then 1 sec",
            ));
        }

        let msg = match self.write_message_need_reply(msg.into()).await {
            Ok(msg) => msg,
            Err(e) => {
                if Error::TimeOut == e {
                    log_err!(
                        warn,
                        self.disconnect(msg::Disconnect {
                            result_code: msg::ResultCode::ConnectTimeout,
                            result_string: "connect timeout".to_string(),
                        })
                        .await
                    );
                }
                return Err(e);
            }
        };

        let resp: msg::ConnectResponse = msg.try_into()?;

        match resp.result_code {
            msg::ResultCode::Succeeded | msg::ResultCode::IncompatibleVersion => {
                let conn = self.clone();
                tokio::spawn(async move { conn.keep_alive(interval, timeout).await });
            }
            _ => {}
        }

        Ok(resp)
    }

    pub async fn disconnect(&self, msg: msg::Disconnect) -> Result<()> {
        debug!("disconnect : {:?}", msg);
        tokio::select! {
            result = self.write_message(msg.into()) => {
                result?;
                trace!("sent disconnect message");
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                warn!("cannot send disconnect message");
            }
        };

        self.close().await
    }

    pub async fn upstream_open_request(
        &self,
        mut msg: msg::UpstreamOpenRequest,
    ) -> Result<msg::UpstreamOpenResponse> {
        self.state.check_connected()?;

        msg.request_id = self.state.next_request_id();
        debug!("open upstream: {:?}", msg);

        let qos = msg.qos;

        let msg = self.write_message_need_reply(msg.into()).await?;
        let res = msg::UpstreamOpenResponse::try_from(msg)?;
        self.state
            .register_upstream_qos(res.assigned_stream_id_alias, qos)?;

        self.state
            .register_upstream_alias(res.assigned_stream_id, res.assigned_stream_id_alias)?;

        Ok(res)
    }

    pub async fn upstream_resume_request(
        &self,
        mut msg: msg::UpstreamResumeRequest,
        qos: msg::QoS,
    ) -> Result<msg::UpstreamResumeResponse> {
        self.state.check_connected()?;

        msg.request_id = self.state.next_request_id();
        debug!("resume upstream: {:?}", msg);

        let uuid = msg.stream_id;

        let msg = self.write_message_need_reply(msg.into()).await?;
        let res = msg::UpstreamResumeResponse::try_from(msg)?;
        self.state
            .register_upstream_qos(res.assigned_stream_id_alias, qos)?; // todo: qos is wanted to be included in response

        self.state
            .register_upstream_alias(uuid, res.assigned_stream_id_alias)?;

        Ok(res)
    }
    pub async fn upstream_close_request(
        &self,
        mut msg: msg::UpstreamCloseRequest,
    ) -> Result<msg::UpstreamCloseResponse> {
        msg.request_id = self.state.next_request_id();
        debug!("close upstream: {:?}", msg);
        let uuid = msg.stream_id;

        let msg = self.write_message_need_reply(msg.into()).await?;
        let res = msg::UpstreamCloseResponse::try_from(msg)?;

        if res.result_code != msg::ResultCode::Succeeded {
            return Ok(res);
        }
        let alias = match self.state.deregister_upstream_alias(&uuid) {
            Some(a) => a,
            None => return Ok(res),
        };

        self.state.deregister_upstream_qos(&alias);

        Ok(res)
    }

    pub async fn upstream_metadata(
        &self,
        mut msg: msg::UpstreamMetadata,
    ) -> Result<msg::UpstreamMetadataAck> {
        msg.request_id = self.state.next_request_id();
        let resp = self.write_message_need_reply(msg.into()).await?;
        resp.try_into()
    }

    pub async fn upstream_data_points(&self, msg: msg::UpstreamChunk) -> Result<()> {
        match self.unreliable_tr {
            Some(_) => match self.state.find_upstream_qos(&msg.stream_id_alias)? {
                msg::QoS::Reliable => self.write_message(msg.into()).await,
                msg::QoS::Unreliable => self.unreliable_write_message(msg.into()).await,
                msg::QoS::Partial => self.write_message(msg.into()).await,
            },
            None => self.write_message(msg.into()).await,
        }
    }

    pub async fn downstream_data_points_ack(&self, msg: msg::DownstreamChunkAck) -> Result<()> {
        self.state.check_connected()?;
        self.write_message(msg.into()).await
    }

    pub async fn downstream_open_request(
        &self,
        mut msg: msg::DownstreamOpenRequest,
    ) -> Result<msg::DownstreamOpenResponse> {
        msg.request_id = self.state.next_request_id();
        debug!("open downstream: {:?}", msg);

        let msg = self.write_message_need_reply(msg.into()).await?;
        msg.try_into()
    }

    pub async fn downstream_resume_request(
        &self,
        mut msg: msg::DownstreamResumeRequest,
    ) -> Result<msg::DownstreamResumeResponse> {
        self.state.check_connected()?;

        msg.request_id = self.state.next_request_id();
        debug!("resume downstream: {:?}", msg);

        let msg = self.write_message_need_reply(msg.into()).await?;

        msg.try_into()
    }

    pub async fn downstream_close_request(
        &self,
        mut msg: msg::DownstreamCloseRequest,
    ) -> Result<msg::DownstreamCloseResponse> {
        msg.request_id = self.state.next_request_id();
        debug!("close downstream: {:?}", msg);

        let msg = self.write_message_need_reply(msg.into()).await?;
        msg.try_into()
    }

    pub fn subscribe_upstream(&self, stream_id_alias: u32) -> Result<mpsc::Receiver<msg::Message>> {
        self.state.add_upstream_subscriber(stream_id_alias)
    }

    pub fn unsubscribe_upstream(&self, stream_id_alias: u32) -> Result<()> {
        self.state.remove_upstream_subscriber(stream_id_alias)
    }

    pub fn subscribe_downstream(
        &self,
        stream_id_alias: u32,
        qos: msg::QoS,
    ) -> Result<mpsc::Receiver<msg::Message>> {
        match self.unreliable_tr {
            Some(_) => match qos {
                msg::QoS::Reliable => self.state.add_downstream_subscriber(stream_id_alias),
                msg::QoS::Unreliable => self
                    .state
                    .add_unreliable_downstream_subscriber(stream_id_alias),
                msg::QoS::Partial => todo!(),
            },
            None => self.state.add_downstream_subscriber(stream_id_alias),
        }
    }

    pub fn unsubscribe_downstream(&self, stream_id_alias: u32) -> Result<()> {
        if self
            .state
            .remove_downstream_subscriber(stream_id_alias)
            .is_ok()
        {
            return Ok(());
        }

        self.state
            .remove_unreliable_downstream_subscriber(stream_id_alias)
    }

    pub fn subscribe_downstream_meta(&self) -> broadcast::Receiver<msg::DownstreamMetadata> {
        self.state.broadcast_downstream_meta.subscribe()
    }

    pub fn unsubscribe_downstream_meta(&self) {
        // do nothing
    }

    pub fn subscribe_downstream_call(&self) -> broadcast::Receiver<msg::DownstreamCall> {
        self.state.broadcast_downstream_call.subscribe()
    }

    pub async fn upstream_call(&self, msg: msg::UpstreamCall) -> Result<msg::UpstreamCallAck> {
        debug!("{:?}", msg);
        let (s, r) = oneshot::channel();
        self.state.add_waiting_e2e_ack(msg.call_id.as_str(), s);

        self.tr.write_message(msg.into()).await?;

        let res = tokio::select! {
            resp = r => resp.map_err(Error::unexpected),
            _ = tokio::time::sleep(self.request_timeout) => Err(Error::TimeOut),
            _ = self.cancel.notified() => Err(Error::ConnectionClosed("".into())),
        };
        let msg = res?;
        msg::UpstreamCallAck::try_from(msg)
    }

    pub async fn ping(&self) -> Result<msg::Pong> {
        let ping = msg::Ping {
            request_id: self.state.next_request_id(),
        };
        let msg = self.write_message_need_reply(ping.into()).await?;
        msg.try_into()
    }
}

// private methods
impl<T> Conn<T>
where
    T: Transport,
{
    async fn write_message_need_reply(&self, msg: Message) -> Result<Message> {
        self.state.check_connected()?;
        let id = msg
            .request_id()
            .ok_or_else(|| Error::unexpected("unmatched message received"))?;

        let (sender, receiver) = oneshot::channel();
        self.state.add_waiting_reply(&id, sender);

        let res = self.write_message(msg).await;

        if res.is_err() {
            self.state.remove_waiting_reply(&id);
            return Err(res.unwrap_err());
        }

        tokio::select! {
            resp = receiver => resp.map_err(Error::unexpected),
            _ = tokio::time::sleep(self.request_timeout) => Err(Error::TimeOut),
            _ = self.cancel.notified() => Err(Error::ConnectionClosed("".into())),
        }
    }
    async fn write_message(&self, msg: Message) -> Result<()> {
        self.state.check_connected()?;
        self.tr.write_message(msg).await
    }
    async fn unreliable_write_message(&self, msg: Message) -> Result<()> {
        match &self.unreliable_tr {
            None => unreachable!("must be some in case of calling this method"),
            Some(tr) => tr.unreliable_write_message(msg).await,
        }
    }
}

// background tasks
impl<T> Conn<T>
where
    T: Transport + 'static,
{
    async fn ping_pong(
        &self,
        interval: std::time::Duration,
        timeout: std::time::Duration,
    ) -> Result<()> {
        // let mut timeout = tokio::time::interval(timeout);
        let mut interval = tokio::time::interval(interval);
        loop {
            tokio::select! {
                res = tokio::time::timeout(timeout, self.ping()) => res??,
                _ = self.cancel.notified() => return Ok(()),
            };

            tokio::select! {
                _ = interval.tick() => {},
                _ = self.cancel.notified() => return Ok(()),
            }
        }
    }

    async fn keep_alive(&self, interval: std::time::Duration, timeout: std::time::Duration) {
        let res = self.ping_pong(interval, timeout).await;
        if let Err(e) = res {
            if Error::TimeOut == e {
                log_err!(
                    warn,
                    self.disconnect(msg::Disconnect {
                        result_code: msg::ResultCode::PingTimeout,
                        result_string: "ping timeout".to_string(),
                    })
                    .await
                );
            } else {
                log_err!(
                    warn,
                    self.disconnect(msg::Disconnect {
                        result_code: msg::ResultCode::UnspecifiedError,
                        result_string: e.to_string(),
                    })
                    .await
                );
            }
            log_err!(warn, self.close().await);
        }
    }

    async fn wait_close(&self, mut waiter: Waiter) {
        waiter.wait().await;
        self.state.remove_all_subscriber();

        self.state.connected.store(false, Ordering::Release);
        log_err!(trace, self.close_notify.send(()));
    }

    // read loop despatches incoming messages to subscribers
    async fn unreliable_read_loop(&self, _wg: crate::sync::WaitGroup) -> Result<()> {
        let tr = match &self.unreliable_tr {
            Some(tr) => tr,
            None => {
                debug!("no unreliable transport");
                return Ok(());
            }
        };

        let (downstream_s, downstream_r) = mpsc::channel(8);
        let conn = self.clone();
        tokio::task::spawn(async move { conn.read_downstream_unreliable_loop(downstream_r).await });

        loop {
            let msg = tokio::select! {
                _ = self.cancel.notified() => break,
                res = tr.unreliable_read_message() => res,
            };

            // Explicitly disconnect if malformed message error
            if matches!(msg, Err(Error::MalformedMessage(_))) {
                self.disconnect(Disconnect {
                    result_code: msg::ResultCode::MalformedMessage,
                    result_string: "message decode error".into(),
                })
                .await?;
            }
            let msg = msg?;

            trace!("got message: {:?}", msg);
            match msg {
                Message::DownstreamChunk(dps) => {
                    let alias = dps.stream_id_alias;
                    log_err!(warn, downstream_s.send((dps, alias)).await);
                }
                _ => {
                    error!("receive invalid message");
                    log_err!(trace, self.cancel.notify());
                    break;
                }
            }
        }
        Ok(())
    }

    async fn read_disconnect_loop(&self, mut recv: mpsc::Receiver<msg::Disconnect>) {
        while let Some(d) = recv.recv().await {
            info!("receive disconnect: {:?}", d);
            log_err!(debug, self.close().await);
        }
    }

    async fn read_ping_loop(&self, mut ping: mpsc::Receiver<msg::Ping>) {
        while let Some(p) = ping.recv().await {
            let msg = msg::Message::from(msg::Pong::from(p));
            log_err!(warn, self.tr.write_message(msg).await);
        }
    }

    async fn read_downstream_meta_loop(&self, mut meta_r: mpsc::Receiver<msg::DownstreamMetadata>) {
        while let Some(meta) = meta_r.recv().await {
            let request_id = meta.request_id;

            let msg: msg::Message = match self.state.broadcast_downstream_meta.send(meta) {
                Ok(_) => msg::DownstreamMetadataAck {
                    request_id,
                    result_code: msg::ResultCode::Succeeded,
                    result_string: "ok".to_string(),
                }
                .into(),
                Err(e) => {
                    warn!("{}", e);
                    continue;
                }
            };
            log_err!(warn, self.tr.write_message(msg).await);
        }
    }

    async fn read_downstream_call_loop(&self, mut call_r: mpsc::Receiver<msg::DownstreamCall>) {
        while let Some(call) = call_r.recv().await {
            log_err!(warn, self.state.broadcast_downstream_call.send(call));
        }
    }

    async fn read_upstream_call_ack_loop(&self, mut ack_r: mpsc::Receiver<msg::UpstreamCallAck>) {
        while let Some(ack) = ack_r.recv().await {
            match self.state.remove_waiting_e2e_ack(ack.call_id.as_str()) {
                Some(s) => debug_log_err!(warn, s.send(ack.into())),
                None => {
                    warn!("no subscriber");
                    continue;
                }
            }
        }
    }

    async fn read_request_loop(&self, mut recv: mpsc::Receiver<(msg::Message, msg::RequestId)>) {
        while let Some((m, id)) = recv.recv().await {
            match self.state.remove_waiting_reply(&id) {
                Some(s) => {
                    if let Err(e) = s.send(m) {
                        warn!("cannot dispatch message: {:?}", e);
                        continue;
                    };
                }
                None => {
                    warn!("no subscriber");
                    continue;
                }
            };
        }
    }

    async fn read_upstream_loop(&self, mut recv: mpsc::Receiver<(msg::Message, u32)>) {
        while let Some((m, alias)) = recv.recv().await {
            if let Some(s) = self.state.find_upstream_subscriber(alias) {
                if s.send(m).await.is_err() {
                    error!("channel is closed for stream_alias: {}", alias);
                    log_err!(warn, self.state.remove_upstream_subscriber(alias));
                }
            }
        }
    }

    async fn read_downstream_loop(&self, mut recv: mpsc::Receiver<(msg::Message, u32)>) {
        while let Some((m, alias)) = recv.recv().await {
            if let Some(s) = self.state.find_downstream_subscriber(alias) {
                if s.send(m).await.is_err() {
                    error!("channel is closed for stream_alias: {}", alias);
                    log_err!(warn, self.state.remove_downstream_subscriber(alias));
                }
            }
        }
    }

    async fn read_downstream_unreliable_loop(
        &self,
        mut recv: mpsc::Receiver<(msg::DownstreamChunk, u32)>,
    ) {
        while let Some((m, alias)) = recv.recv().await {
            if let Some(s) = self.state.find_unreliable_downstream_subscriber(alias) {
                if s.send(m.into()).await.is_err() {
                    error!("channel is closed for stream_alias: {}", alias);
                    log_err!(
                        warn,
                        self.state.remove_unreliable_downstream_subscriber(alias)
                    );
                }
            }
        }
    }
    // read loop despatches incoming messages to subscribers
    async fn read_loop(&self, _wg: crate::sync::WaitGroup) -> Result<()> {
        let buffer_size = 8;

        let (disconnect_s, disconnect_r) = mpsc::channel(buffer_size);
        let conn = self.clone();
        tokio::task::spawn(async move { conn.read_disconnect_loop(disconnect_r).await });

        let (ping_s, ping_r) = mpsc::channel(buffer_size);
        let conn = self.clone();
        tokio::task::spawn(async move { conn.read_ping_loop(ping_r).await });

        let (downstream_meta_s, downstream_meta_r) = mpsc::channel(buffer_size);
        let conn = self.clone();
        tokio::task::spawn(async move { conn.read_downstream_meta_loop(downstream_meta_r).await });

        let (downstream_call_s, downstream_call_r) = mpsc::channel(buffer_size);
        let conn = self.clone();
        tokio::task::spawn(async move { conn.read_downstream_call_loop(downstream_call_r).await });

        let (upstream_call_ack_s, upstream_call_ack_r) = mpsc::channel(buffer_size);
        let conn = self.clone();
        tokio::task::spawn(
            async move { conn.read_upstream_call_ack_loop(upstream_call_ack_r).await },
        );

        let (request_s, request_r) = mpsc::channel(buffer_size);
        let conn = self.clone();
        tokio::task::spawn(async move { conn.read_request_loop(request_r).await });

        let (upstream_s, upstream_r) = mpsc::channel(buffer_size);
        let conn = self.clone();
        tokio::task::spawn(async move { conn.read_upstream_loop(upstream_r).await });

        let (downstream_s, downstream_r) = mpsc::channel(buffer_size);
        let conn = self.clone();
        tokio::task::spawn(async move { conn.read_downstream_loop(downstream_r).await });

        loop {
            let msg = tokio::select! {
                res = self.tr.read_message() => res,
                _ = self.cancel.notified() => break,
            };

            // Explicitly disconnect if malformed message error
            if matches!(msg, Err(Error::MalformedMessage(_))) {
                self.disconnect(Disconnect {
                    result_code: msg::ResultCode::MalformedMessage,
                    result_string: "message decode error".into(),
                })
                .await?;
            }
            let msg = msg?;

            trace!("got message: {:?}", msg);
            let msg = match msg {
                Message::Ping(p) => {
                    log_err!(warn, ping_s.send(p).await);
                    continue;
                }
                Message::Disconnect(d) => {
                    log_err!(warn, disconnect_s.send(d).await);
                    continue;
                }
                Message::DownstreamMetadata(meta) => {
                    log_err!(warn, downstream_meta_s.send(meta).await);
                    continue;
                }
                Message::UpstreamCallAck(ack) => {
                    log_err!(warn, upstream_call_ack_s.send(ack).await);
                    continue;
                }
                Message::DownstreamCall(call) => {
                    log_err!(warn, downstream_call_s.send(call).await);
                    continue;
                }
                _ => msg,
            };

            // dispatch control-message
            if let Some(id) = msg.request_id() {
                log_err!(warn, request_s.send((msg, id)).await);
            } else if let Some(alias) = msg.upstream_id_alias() {
                log_err!(warn, upstream_s.send((msg, alias)).await);
            } else if let Some(alias) = msg.downstream_id_alias() {
                log_err!(warn, downstream_s.send((msg, alias)).await);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {

    use std::{
        convert::TryFrom,
        sync::atomic::{AtomicBool, Ordering},
    };

    use tokio::sync::broadcast;

    use super::*;

    fn init_logger() {
        let _ = env_logger::builder()
            .filter_level(log::LevelFilter::Trace)
            .try_init();
    }

    #[test]
    fn next_request_id() {
        let (broadcast_downstream_meta, _) = broadcast::channel(1);
        let (broadcast_downstream_call, _) = broadcast::channel(1);
        let shared = State {
            connected: AtomicBool::new(true),
            request_id: Mutex::new(RequestId::new_as_edge()),
            waiting_reply: Mutex::default(),
            upstream_alias: Mutex::default(),
            upstream_qos: Mutex::default(),
            upstream_subscribers: Mutex::default(),
            downstream_subscribers: Mutex::default(),
            unreliable_downstream_subscribers: Mutex::default(),
            waiting_e2e_ack: Mutex::default(),
            broadcast_downstream_meta,
            broadcast_downstream_call,
        };
        assert_eq!(shared.next_request_id().value(), 0);
        assert_eq!(shared.next_request_id().value(), 2);
    }

    #[tokio::test]
    async fn connect_reply_timeout() {
        init_logger();

        #[derive(Default, Debug)]
        struct MockTrans {
            disconnect_called: AtomicBool,
            close_called: AtomicBool,
        }
        impl Drop for MockTrans {
            fn drop(&mut self) {
                assert!(self.disconnect_called.load(Ordering::Acquire));
                assert!(self.close_called.load(Ordering::Acquire));
            }
        }

        #[async_trait]
        impl Transport for MockTrans {
            async fn read_message(&self) -> Result<msg::Message> {
                tokio::time::sleep(Duration::from_secs(1)).await;
                Ok(msg::ConnectResponse::default().into())
            }

            async fn write_message(&self, _msg: msg::Message) -> Result<()> {
                if let msg::Message::Disconnect(m) = _msg {
                    assert!(matches!(m.result_code, msg::ResultCode::ConnectTimeout));
                    self.disconnect_called.store(true, Ordering::Release);
                }
                Ok(())
            }

            async fn close(&self) -> Result<()> {
                self.close_called.store(true, Ordering::Release);
                Ok(())
            }
        }

        let conn = connect(MockTrans::default(), None, Some(Duration::from_millis(1)))
            .await
            .unwrap();
        assert!(conn
            .open_request(msg::ConnectRequest::default())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn connect_read_loop_closed() {
        struct MockTrans;
        #[async_trait]
        impl Transport for MockTrans {
            async fn read_message(&self) -> Result<msg::Message> {
                Err(Error::unexpected("test"))
            }
            async fn write_message(&self, _msg: msg::Message) -> Result<()> {
                Ok(())
            }
            async fn close(&self) -> Result<()> {
                Ok(())
            }
        }
        let conn = connect(MockTrans, None, None).await.unwrap();

        conn.close().await.unwrap();
        let err = conn
            .write_message_need_reply(msg::Ping::default().into())
            .await
            .expect_err("want error");
        assert!(matches!(err, Error::ConnectionClosed(_)))
    }

    #[tokio::test]
    async fn dispatch_upstream_message() {
        struct MockTrans;
        #[async_trait]
        impl Transport for MockTrans {
            async fn read_message(&self) -> Result<msg::Message> {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                Ok(msg::UpstreamChunkAck {
                    stream_id_alias: 1,
                    ..Default::default()
                }
                .into())
            }
            async fn write_message(&self, _msg: msg::Message) -> Result<()> {
                Ok(())
            }
            async fn close(&self) -> Result<()> {
                Ok(())
            }
        }
        let conn = connect(MockTrans, None, None).await.unwrap();
        let mut r = conn.subscribe_upstream(1).unwrap();
        let _ = conn.subscribe_upstream(1).expect_err("not allowed twice");
        let got = r.recv().await.unwrap();
        let got: msg::UpstreamChunkAck = got.try_into().unwrap();

        assert_eq!(
            got,
            msg::UpstreamChunkAck {
                stream_id_alias: 1,
                ..Default::default()
            }
        );

        conn.unsubscribe_upstream(1).unwrap();
        let recv = r.recv().await;
        assert!(recv.is_none());
    }

    #[tokio::test]
    async fn dispatch_downstream_message() {
        struct MockTrans;
        #[async_trait]
        impl Transport for MockTrans {
            async fn read_message(&self) -> Result<msg::Message> {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                Ok(msg::DownstreamChunk {
                    stream_id_alias: 1,
                    ..Default::default()
                }
                .into())
            }
            async fn write_message(&self, _msg: msg::Message) -> Result<()> {
                Ok(())
            }
            async fn close(&self) -> Result<()> {
                Ok(())
            }
        }
        let conn = connect(MockTrans, None, None).await.unwrap();
        let mut r = conn.subscribe_downstream(1, msg::QoS::Unreliable).unwrap();
        let _ = conn
            .subscribe_downstream(1, msg::QoS::Unreliable)
            .expect_err("not allowed twice");
        let got = r.recv().await.unwrap();
        let got: msg::DownstreamChunk = got.try_into().unwrap();

        assert_eq!(
            got,
            msg::DownstreamChunk {
                stream_id_alias: 1,
                ..Default::default()
            }
        );

        conn.unsubscribe_downstream(1).unwrap();
        let recv = r.recv().await;
        assert!(recv.is_none());
    }

    #[tokio::test]
    async fn connect_read_loop_response_ping() {
        struct MockTrans {
            ok: AtomicBool,
        }
        impl MockTrans {
            fn new() -> Self {
                Self {
                    ok: AtomicBool::new(false),
                }
            }
        }

        #[async_trait]
        impl Transport for MockTrans {
            async fn read_message(&self) -> Result<msg::Message> {
                tokio::time::sleep(Duration::from_millis(1)).await;
                Ok(msg::Ping::default().into())
            }
            async fn write_message(&self, msg: msg::Message) -> Result<()> {
                if msg::Pong::try_from(msg).is_ok() {
                    self.ok.store(true, Ordering::Release);
                }

                Ok(())
            }
            async fn close(&self) -> Result<()> {
                if !self.ok.load(Ordering::Acquire) {
                    return Err(Error::unexpected("not ok"));
                }
                Ok(())
            }
        }
        let conn = connect(MockTrans::new(), None, None).await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        conn.close().await.unwrap();
    }

    #[tokio::test]
    async fn connect_open_request() {
        struct MockTrans {
            s: broadcast::Sender<msg::Message>,
        }
        impl MockTrans {
            fn new() -> Self {
                let (s, _) = broadcast::channel(1);
                Self { s }
            }
        }
        #[async_trait]
        impl Transport for MockTrans {
            async fn read_message(&self) -> Result<msg::Message> {
                let m = self.s.subscribe().recv().await.unwrap();
                Ok(m)
            }
            async fn write_message(&self, msg: msg::Message) -> Result<()> {
                let input = match msg::ConnectRequest::try_from(msg) {
                    Ok(m) => m,
                    Err(_) => return Ok(()),
                };

                let expect = msg::ConnectRequest {
                    request_id: input.request_id,
                    ping_interval: chrono::Duration::seconds(10),
                    ping_timeout: chrono::Duration::seconds(4),
                    protocol_version: crate::ISCP_VERSION.to_string(),
                    node_id: "test_id".to_string(),
                    access_token: Some(msg::AccessToken::new("access_token")),
                    project_uuid: None,
                };
                assert_eq!(expect, input);
                let resp = msg::ConnectResponse {
                    request_id: input.request_id,
                    result_code: msg::ResultCode::Succeeded,
                    ..Default::default()
                };
                let _ = self.s.send(resp.into()).unwrap();
                Ok(())
            }
            async fn close(&self) -> Result<()> {
                Ok(())
            }
        }
        let conn = connect(MockTrans::new(), None, None).await.unwrap();
        tokio::time::sleep(Duration::from_millis(1)).await;

        let _resp = conn
            .open_request(msg::ConnectRequest {
                node_id: "test_id".to_string(),
                protocol_version: crate::ISCP_VERSION.to_string(),
                ping_timeout: chrono::Duration::seconds(4),
                access_token: Some(msg::AccessToken::new("access_token")),
                ..Default::default()
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn upstream_open_request() {
        struct MockTrans {
            s: broadcast::Sender<msg::Message>,
        }
        impl MockTrans {
            fn new() -> Self {
                let (s, _) = broadcast::channel(1);
                Self { s }
            }
        }
        #[async_trait]
        impl Transport for MockTrans {
            async fn read_message(&self) -> Result<msg::Message> {
                let m = self.s.subscribe().recv().await.unwrap();
                Ok(m)
            }
            async fn write_message(&self, msg: msg::Message) -> Result<()> {
                let input: msg::UpstreamOpenRequest =
                    msg.try_into().expect("upstream open request");
                let resp = msg::UpstreamOpenResponse {
                    request_id: input.request_id,
                    result_code: msg::ResultCode::Succeeded,
                    ..Default::default()
                };
                let _ = self.s.send(resp.into()).unwrap();
                Ok(())
            }
            async fn close(&self) -> Result<()> {
                Ok(())
            }
        }
        let conn = connect(MockTrans::new(), None, None).await.unwrap();
        tokio::time::sleep(Duration::from_millis(1)).await;

        let _resp = conn
            .upstream_open_request(msg::UpstreamOpenRequest::default())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn upstream_resume_request() {
        struct MockTrans {
            s: broadcast::Sender<msg::Message>,
        }
        impl MockTrans {
            fn new() -> Self {
                let (s, _) = broadcast::channel(1);
                Self { s }
            }
        }
        #[async_trait]
        impl Transport for MockTrans {
            async fn read_message(&self) -> Result<msg::Message> {
                let m = self.s.subscribe().recv().await.unwrap();
                Ok(m)
            }
            async fn write_message(&self, msg: msg::Message) -> Result<()> {
                let input: msg::UpstreamResumeRequest =
                    msg.try_into().expect("upstream open request");
                let resp = msg::UpstreamResumeResponse {
                    request_id: input.request_id,
                    result_code: msg::ResultCode::Succeeded,
                    ..Default::default()
                };
                let _ = self.s.send(resp.into()).unwrap();
                Ok(())
            }
            async fn close(&self) -> Result<()> {
                Ok(())
            }
        }
        let conn = connect(MockTrans::new(), None, None).await.unwrap();
        tokio::time::sleep(Duration::from_millis(1)).await;

        let _resp = conn
            .upstream_resume_request(
                msg::UpstreamResumeRequest {
                    stream_id: uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111")
                        .unwrap(),
                    ..Default::default()
                },
                msg::QoS::Unreliable,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn dispatch_downstream_meta() {
        struct MockTrans {
            s: broadcast::Sender<msg::Message>,
        }

        #[async_trait]
        impl Transport for MockTrans {
            async fn read_message(&self) -> Result<msg::Message> {
                let m = self.s.subscribe().recv().await.unwrap();
                Ok(m)
            }
            async fn write_message(&self, msg: msg::Message) -> Result<()> {
                let input: msg::DownstreamMetadataAck =
                    msg.try_into().expect("downstream metadata ack");
                assert_eq!(input.request_id.value(), 100);
                Ok(())
            }
            async fn close(&self) -> Result<()> {
                Ok(())
            }
        }
        let (s, _) = broadcast::channel(1);

        let conn = connect(MockTrans { s: s.clone() }, None, None)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(1)).await;

        let mut recv_meta = conn.subscribe_downstream_meta();
        let want_msg = msg::DownstreamMetadata {
            request_id: 100.into(),
            ..Default::default()
        };
        s.send(want_msg.clone().into()).unwrap();

        assert_eq!(recv_meta.recv().await.unwrap(), want_msg);
    }

    #[tokio::test]
    async fn upstream_call() {
        struct MockTrans {
            s: broadcast::Sender<msg::Message>,
        }

        #[async_trait]
        impl Transport for MockTrans {
            async fn read_message(&self) -> Result<msg::Message> {
                let m = self.s.subscribe().recv().await.unwrap();
                Ok(m)
            }
            async fn write_message(&self, msg: msg::Message) -> Result<()> {
                let input: msg::UpstreamCall = msg.try_into().expect("upstream call");
                assert_eq!(input.call_id.as_str(), "call_id");

                self.s
                    .send(
                        msg::UpstreamCallAck {
                            call_id: input.call_id,
                            result_code: msg::ResultCode::Succeeded,
                            result_string: "OK".to_string(),
                        }
                        .into(),
                    )
                    .map_err(Error::unexpected)?;

                Ok(())
            }
            async fn close(&self) -> Result<()> {
                Ok(())
            }
        }
        let (s, _) = broadcast::channel(1);

        let conn = connect(MockTrans { s: s.clone() }, None, None)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(1)).await;
        let ack = conn
            .upstream_call(msg::UpstreamCall {
                call_id: "call_id".to_string(),
                destination_node_id: "node_id".to_string(),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(
            ack,
            msg::UpstreamCallAck {
                call_id: "call_id".to_string(),
                result_code: msg::ResultCode::Succeeded,
                result_string: "OK".to_string(),
            }
        )
    }

    #[tokio::test]
    async fn dispatch_downstream_call() {
        struct MockTrans {
            s: broadcast::Sender<msg::Message>,
        }

        #[async_trait]
        impl Transport for MockTrans {
            async fn read_message(&self) -> Result<msg::Message> {
                let m = self.s.subscribe().recv().await.unwrap();
                Ok(m)
            }
            async fn write_message(&self, _msg: msg::Message) -> Result<()> {
                unreachable!();
            }
            async fn close(&self) -> Result<()> {
                Ok(())
            }
        }
        let (s, _) = broadcast::channel(1);

        let conn = connect(MockTrans { s: s.clone() }, None, None)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(1)).await;

        let mut recv_call = conn.subscribe_downstream_call();
        let want_msg = msg::DownstreamCall {
            call_id: "call_id".to_string(),
            ..Default::default()
        };
        s.send(want_msg.clone().into()).unwrap();

        assert_eq!(recv_call.recv().await.unwrap(), want_msg);
    }

    #[tokio::test]
    async fn unreliable_read() {
        init_logger();
        struct Case {
            msg: msg::Message,
            want_err: bool,
        }

        impl Case {
            async fn test(&self) {
                let mut mock_tr = crate::wire::MockMockTransport::new();
                mock_tr
                    .expect_read_message()
                    .returning(|| Err(Error::malformed_message("reliable")));

                mock_tr.expect_close().returning(|| Ok(()));

                let msg_c = self.msg.clone();
                let mut mock_utr = crate::wire::MockMockUnreliableTransport::new();
                let mut called = false;
                mock_utr
                    .expect_unreliable_read_message()
                    .returning(move || {
                        if !called {
                            called = true;
                        }
                        Ok(msg_c.clone())
                    });

                let mock_utr = Box::new(mock_utr) as super::BoxedUnreliableTransport;

                let conn = connect(mock_tr, Some(mock_utr), None).await.unwrap();

                let mut down_recv = conn.subscribe_downstream(1, msg::QoS::Unreliable).unwrap();
                let mut up_recv = conn.subscribe_upstream(1).unwrap();

                let res = tokio::select! {
                    Some(recv) = down_recv.recv() => recv,
                    Some(recv) = up_recv.recv() => recv,
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {
                        if !self.want_err {
                            panic!("timeout error");
                        }
                        return;
                    }
                };

                assert_eq!(res, self.msg);
            }
        }
        let cases = vec![Case {
            msg: msg::DownstreamChunk {
                stream_id_alias: 1,
                ..Default::default()
            }
            .into(),
            want_err: false,
        }];

        for case in cases.into_iter() {
            case.test().await;
        }
    }

    #[tokio::test]
    async fn unreliable_write_message() {
        let mock_tr = crate::wire::MockMockTransport::new();
        let mut mock_utr = crate::wire::MockMockUnreliableTransport::new();
        mock_utr
            .expect_unreliable_write_message()
            .returning(|msg| match msg {
                Message::DownstreamChunkAck(_) | Message::UpstreamChunk(_) => Ok(()),
                _ => Err(Error::unexpected("unmatched message")),
            });

        let mock_utr = Box::new(mock_utr) as super::BoxedUnreliableTransport;

        let (close_notify, _) = broadcast::channel(1);
        let (broadcast_downstream_meta, _) = broadcast::channel(1);
        let (broadcast_downstream_call, _) = broadcast::channel(1);
        let conn = Conn {
            cancel: Cancel::new(),
            close_notify,
            state: Arc::new(State {
                broadcast_downstream_meta,
                broadcast_downstream_call,
                connected: AtomicBool::new(true),
                downstream_subscribers: Mutex::default(),
                unreliable_downstream_subscribers: Mutex::default(),
                upstream_alias: Mutex::default(),
                upstream_qos: Mutex::default(),
                upstream_subscribers: Mutex::default(),
                waiting_reply: Mutex::default(),
                waiting_e2e_ack: Mutex::default(),
                request_id: Mutex::default(),
            }),
            request_timeout: Duration::from_millis(1),
            tr: Arc::new(mock_tr),
            unreliable_tr: Some(Arc::new(mock_utr)),
        };

        conn.state
            .register_upstream_qos(0, msg::QoS::Unreliable)
            .unwrap();

        conn.upstream_data_points(msg::UpstreamChunk::default())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn ping_pong_timeout() {
        init_logger();

        #[derive(Default, Debug)]
        struct MockTrans {
            disconnect_called: AtomicBool,
            ping_called: AtomicBool,
            close_called: AtomicBool,
        }
        impl Drop for MockTrans {
            fn drop(&mut self) {
                assert!(self.disconnect_called.load(Ordering::Acquire));
                assert!(self.ping_called.load(Ordering::Acquire));
                assert!(self.close_called.load(Ordering::Acquire));
            }
        }

        #[async_trait]
        impl Transport for MockTrans {
            async fn read_message(&self) -> Result<msg::Message> {
                tokio::time::sleep(Duration::from_secs(2)).await;
                Ok(msg::Pong::default().into())
            }

            async fn write_message(&self, msg: msg::Message) -> Result<()> {
                match msg {
                    msg::Message::Disconnect(m) => {
                        assert!(matches!(m.result_code, msg::ResultCode::PingTimeout));
                        self.disconnect_called.store(true, Ordering::Release);
                    }
                    msg::Message::Ping(_m) => {
                        self.ping_called.store(true, Ordering::Release);
                    }
                    _ => {}
                }
                Ok(())
            }

            async fn close(&self) -> Result<()> {
                self.close_called.store(true, Ordering::Release);
                Ok(())
            }
        }

        let conn = connect(MockTrans::default(), None, Some(Duration::from_secs(3)))
            .await
            .unwrap();
        let _res = conn
            .ping_pong(Duration::from_secs(10), Duration::from_secs(1))
            .await;
    }

    #[tokio::test]
    async fn recv_disconnect() {
        init_logger();

        #[derive(Default, Debug)]
        struct MockTrans;

        #[async_trait]
        impl Transport for MockTrans {
            async fn read_message(&self) -> Result<msg::Message> {
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                Ok(msg::Disconnect::default().into())
            }

            async fn write_message(&self, _msg: msg::Message) -> Result<()> {
                Ok(())
            }

            async fn close(&self) -> Result<()> {
                Ok(())
            }
        }

        let conn = connect(MockTrans::default(), None, Some(Duration::from_secs(3)))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(3)).await;

        conn.write_message(msg::Ping::default().into())
            .await
            .unwrap_err();
    }
}
