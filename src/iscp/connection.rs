//! コネクションに関するモジュールです。

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use log::*;
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::{
    msg, wire, BoxedDownstreamOption, BoxedUpstreamOption, Cancel, ConnConfig, Downstream,
    DownstreamCall, DownstreamConfig, DownstreamFilter, DownstreamReplyCall, Error, Result,
    SendMetadataConfig, TokenSource, Upstream, UpstreamCall, UpstreamConfig, UpstreamReplyCall,
    WaitGroup, Waiter,
};

use super::{new_call_id, new_internal_call_id, CallId};

/// iSCPのコネクションです。
#[derive(Clone)] // TODO: unpublish clone trait
pub struct Conn<T>
where
    T: TokenSource + Clone,
{
    pub(crate) wire_conn: Arc<dyn wire::Connection>,
    #[allow(dead_code)]
    pub(crate) upstream_repository: Arc<dyn super::UpstreamRepository>,
    #[allow(dead_code)]
    pub(crate) downstream_repository: Arc<dyn super::DownstreamRepository>,
    #[allow(dead_code)]
    pub(crate) sent_storage: Arc<dyn super::SentStorage>,

    state: Arc<super::State>,
    notify_close: broadcast::Sender<()>,
    cmd_sender: mpsc::Sender<oneshot::Sender<msg::DownstreamCall>>,
    cancel: Cancel,

    config: ConnConfig<T>,
}

// public methods
impl<T> Conn<T>
where
    T: TokenSource + Clone + 'static,
{
    pub(super) fn new<W>(
        wire_conn: W,
        sent_storage: Arc<dyn super::SentStorage>,
        upstream_repository: Arc<dyn super::UpstreamRepository>,
        downstream_repository: Arc<dyn super::DownstreamRepository>,
        config: ConnConfig<T>,
    ) -> Self
    where
        W: crate::wire::Connection + 'static,
    {
        let (notify_close, _) = broadcast::channel(1);

        let (cmd_sender, cmd_receiver) = mpsc::channel(1);
        let conn = Conn {
            cancel: Cancel::new(),
            wire_conn: Arc::new(wire_conn),
            sent_storage,
            upstream_repository,
            downstream_repository,
            state: Arc::new(super::State::new(AtomicBool::new(false))),
            notify_close,
            cmd_sender,
            config,
        };

        let (waiter, wg) = WaitGroup::new();

        let conn_c = conn.clone();
        tokio::spawn(async move { conn_c.read_loop(wg, cmd_receiver).await });

        let conn_c = conn.clone();
        tokio::spawn(async move { conn_c.close_waiter(waiter).await });

        conn
    }

    /// 接続中かどうかを判定します。
    pub fn is_connected(&self) -> bool {
        !self.is_close()
    }

    /// コネクションを切断します。
    pub async fn close(&self) -> Result<()> {
        self.check_opened()?;
        let mut close_notified = self.notify_close.subscribe();

        self.wire_conn.unsubscribe_downstream_call();
        log_err!(debug, self.cancel.notify());

        let res =
            tokio::time::timeout(std::time::Duration::from_secs(3), close_notified.recv()).await;
        if res.is_err() {
            warn!("background tasks did not closed successfully")
        }

        self.wire_conn
            .disconnect(msg::Disconnect {
                result_code: msg::ResultCode::NORMAL_CLOSURE,
                ..Default::default()
            })
            .await?;
        self.wire_conn.close().await?;

        Ok(())
    }

    /// 基準時刻を送信します。
    ///
    /// - `t` - 基準時刻
    /// - `opts` - メタデータ送信時のオプション
    pub async fn send_base_time(&self, t: msg::BaseTime, opts: SendMetadataConfig) -> Result<()> {
        self.send_metadata(msg::SendableMetadata::BaseTime(t), opts)
            .await
    }

    /// メタデータを送信します。
    ///
    /// - `m` - メタデータ
    /// - `opts` - メタデータ送信時のオプション
    pub async fn send_metadata(
        &self,
        m: msg::SendableMetadata,
        opts: SendMetadataConfig,
    ) -> Result<()> {
        self.check_opened()?;
        let req = msg::UpstreamMetadata {
            metadata: m,
            persist: opts.persist,
            ..Default::default()
        };
        self.wire_conn.upstream_metadata(req).await?;
        Ok(())
    }

    /// アップストリームを開きます。
    ///
    /// - `session_id` - セッションID
    /// - `opts` - アップストリームのオプション（[`crate::up_opts`]）
    pub async fn open_upstream(
        &self,
        session_id: &str,
        opts: Vec<BoxedUpstreamOption>,
    ) -> Result<Upstream> {
        self.check_opened()?;

        let cfg = UpstreamConfig::new_with(session_id, opts);
        let req = &cfg;

        let resp = self.wire_conn.upstream_open_request(req.into()).await?;
        if !resp.result_code.is_succeeded() {
            return Err(Error::from(resp));
        }
        debug!(
            "open upstream: alias = {}, stream_id = {}",
            resp.assigned_stream_id_alias, resp.assigned_stream_id
        );

        let res = Upstream::new(
            self.wire_conn.clone(),
            cfg,
            super::UpstreamParam {
                stream_id: resp.assigned_stream_id,
                stream_id_alias: resp.assigned_stream_id_alias,
                sent_strage: Arc::new(super::InMemSentStorageNoPayload::default()),
                repository: self.upstream_repository.clone(),
                server_time: resp.server_time,
            },
        )?;

        Ok(res)
    }

    /// ダウンストリームを開きます
    ///
    /// - `filters` - セッションID
    /// - `opts` - ダウンストリームのオプション（[`crate::down_opts`]）
    pub async fn open_downstream(
        &self,
        filters: Vec<DownstreamFilter>,
        opts: Vec<BoxedDownstreamOption>,
    ) -> Result<Downstream> {
        self.check_opened()?;

        let cfg = DownstreamConfig::new_with(filters, opts);

        let stream_id_alias = self.state.next_downstream_alias();
        let data_points_subscriber = self
            .wire_conn
            .subscribe_downstream(stream_id_alias, cfg.qos)?;
        let metadata_subscriber = self.wire_conn.subscribe_downstream_meta();

        let mut req = msg::DownstreamOpenRequest::from(cfg.clone());
        req.desired_stream_id_alias = stream_id_alias;

        let resp = self.wire_conn.downstream_open_request(req).await?;
        if !resp.result_code.is_succeeded() {
            return Err(Error::from(resp));
        }
        debug!(
            "open downstream: alias = {}, stream_id = {}",
            stream_id_alias, resp.assigned_stream_id
        );

        let source_node_ids = cfg
            .filters
            .iter()
            .map(|f| f.source_node_id.clone())
            .collect();

        let down = Downstream::new(
            self.wire_conn.clone(),
            cfg,
            super::DownstreamParam {
                stream_id: resp.assigned_stream_id,
                stream_id_alias,
                data_points_subscriber,
                metadata_subscriber,
                source_node_ids,
                repository: self.downstream_repository.clone(),
                server_time: resp.server_time,
            },
        );

        Ok(down)
    }

    /// コネクションの切断をサブスクライブします。
    pub fn subscribe_close(&self) -> wire::CloseNotificationReceiver {
        self.wire_conn.subscribe_close_notify()
    }

    /// コネクションの生成に使用したパラメーターを返します。
    pub fn to_config(&self) -> ConnConfig<T> {
        self.config.clone()
    }
}

// e2e
impl<T> Conn<T>
where
    T: TokenSource + Clone + 'static,
{
    async fn read_reply_call_loop(&self, mut recv: mpsc::Receiver<msg::DownstreamCall>) {
        while let Some(call) = recv.recv().await {
            let sender = match self.state.take_waiting_reply(&call.request_call_id) {
                Some(s) => s,
                None => continue,
            };
            debug_log_err!(warn, sender.send(call));
        }
    }

    async fn read_loop(
        &self,
        _wg: WaitGroup,
        cmd_recv: mpsc::Receiver<oneshot::Sender<msg::DownstreamCall>>,
    ) {
        let mut recv = self.wire_conn.subscribe_downstream_call();

        let (down_s, down_r) = mpsc::channel(1024);
        let conn = self.clone();
        tokio::task::spawn(async move { conn.cmd_loop(cmd_recv, down_r).await });

        let (reply_s, reply_r) = mpsc::channel(1);
        let conn = self.clone();
        tokio::task::spawn(async move { conn.read_reply_call_loop(reply_r).await });

        loop {
            let res = tokio::select! {
                _ = self.cancel.notified() => break,
                res = recv.recv() => res,
            };
            let down = match res {
                Ok(down) => down,
                Err(e) => {
                    debug!("{}", e);
                    break;
                }
            };
            if down.is_reply() {
                log_err!(warn, reply_s.send(down).await);
            } else {
                log_err!(warn, down_s.try_send(down))
            }
        }
    }

    async fn cmd_loop(
        &self,
        mut cmd_recv: mpsc::Receiver<oneshot::Sender<msg::DownstreamCall>>,
        mut recv: mpsc::Receiver<msg::DownstreamCall>,
    ) {
        while let Some(sender) = cmd_recv.recv().await {
            let down = match recv.recv().await {
                Some(d) => d,
                None => break,
            };
            debug_log_err!(warn, sender.send(down));
        }
    }

    async fn upstream_call(&self, call: msg::UpstreamCall) -> Result<()> {
        let ack = self.wire_conn.upstream_call(call).await?;

        match ack.result_code {
            msg::ResultCode::Succeeded => Ok(()),
            _ => Err(Error::failed_message(ack.result_code, ack.result_string)),
        }
    }

    /// E2Eコールを送信し、コールIDを返します。
    pub async fn send_call(&self, call: UpstreamCall) -> Result<String> {
        self.state.check_open()?;
        let id = new_call_id();
        self.upstream_call(msg::UpstreamCall {
            call_id: id.clone(),
            request_call_id: "".to_string(),
            destination_node_id: call.destination_node_id,
            name: call.data.id.name,
            f_type: call.data.id.r#type,
            payload: call.data.payload,
        })
        .await?;
        Ok(id)
    }

    /// リプライコールを送信し、コールIDを返します。
    pub async fn send_reply_call(&self, call: UpstreamReplyCall) -> Result<String> {
        self.state.check_open()?;
        let id = new_call_id();
        self.upstream_call(msg::UpstreamCall {
            call_id: id.clone(),
            request_call_id: call.request_call_id,
            destination_node_id: call.destination_node_id,
            name: call.data.id.name,
            f_type: call.data.id.r#type,
            payload: call.data.payload,
        })
        .await?;
        Ok(id)
    }

    /// E2Eコールを送信し、それに対応するリプライコールを受信します。
    ///
    /// このメソッドはリプライコールを受信できるまで処理をブロックします。
    pub async fn send_call_and_wait_reply_call(
        &self,
        call: UpstreamCall,
    ) -> Result<DownstreamReplyCall> {
        self.state.check_open()?;

        let call_id = new_internal_call_id();
        let future = self.recv_reply_call_with_call_id(call_id.clone());

        self.upstream_call(msg::UpstreamCall {
            call_id,
            request_call_id: "".to_string(),
            destination_node_id: call.destination_node_id,
            name: call.data.id.name,
            f_type: call.data.id.r#type,
            payload: call.data.payload,
        })
        .await?;

        future.await
    }

    /// リプライコールでないコールを受信します。
    pub async fn recv_call(&self) -> Result<DownstreamCall> {
        self.state.check_open()?;

        let (s, r) = oneshot::channel();
        self.cmd_sender.send(s).await?;

        let res = r.await.map_err(|_| Error::unexpected("drop message"));

        Ok(res?.into())
    }

    /// リプライコールを受信します。
    pub async fn recv_reply_call(&self) -> Result<DownstreamReplyCall> {
        self.state.check_open()?;

        let (s, r) = oneshot::channel();
        if self.state.register_waiting_reply(s).is_none() {
            return Err(Error::invalid_value("already in use this call id"));
        }

        let res = r.await.map_err(|_| Error::unexpected("drop message"));

        Ok(res?.into())
    }
}

// private methods
impl<T> Conn<T>
where
    T: TokenSource + Clone,
{
    async fn close_waiter(&self, mut waiter: Waiter) {
        let mut wire_close_notified = self.wire_conn.subscribe_close_notify();
        tokio::select! {
            _ = wire_close_notified.recv() => {},
            _ = waiter.wait() => {},
        };
        self.state.close.store(true, Ordering::Release);
        log_err!(trace, self.notify_close.send(()));
    }
    fn is_close(&self) -> bool {
        self.state.close.load(Ordering::Acquire)
    }
    fn check_opened(&self) -> Result<()> {
        if self.is_close() {
            return Err(Error::ConnectionClosed("".into()));
        }
        Ok(())
    }

    async fn recv_reply_call_with_call_id(
        &self,
        request_call_id: CallId,
    ) -> Result<DownstreamReplyCall> {
        self.state.check_open()?;

        let (s, r) = oneshot::channel();
        if !self
            .state
            .register_waiting_reply_with_call_id(request_call_id, s)
        {
            return Err(Error::invalid_value("already in use this call id"));
        }

        let res = r.await.map_err(|_| Error::unexpected("drop message"));

        Ok(res?.into())
    }
}

#[cfg(test)]
pub mod test {

    use std::{str::FromStr, time::Duration};

    use crate::*;

    use super::*;
    use async_trait::async_trait;
    use tokio::sync::mpsc;
    use uuid::Uuid;

    pub fn new_mock_conn(_id: &str, mut mock: wire::MockMockConnection) -> Conn<StaticTokenSource> {
        let (notify_close, _) = broadcast::channel(1);
        let r = notify_close.subscribe();
        mock.expect_subscribe_close_notify()
            .return_once(|| wire::CloseNotificationReceiver::new(r));

        let (cmd_sender, _cmd_receiver) = mpsc::channel(1);
        let repo = super::super::InMemStreamRepository::new();
        let repo = Arc::new(repo);
        Conn {
            wire_conn: Arc::new(mock),
            upstream_repository: repo.clone(),
            downstream_repository: repo,
            sent_storage: Arc::new(super::super::InMemSentStorageNoPayload::default()),
            state: Arc::new(super::super::State::new(AtomicBool::new(false))),
            notify_close,
            cmd_sender,
            cancel: Cancel::new(),
            config: ConnConfig::default(),
        }
    }

    #[tokio::test]
    async fn upstream_open_request() {
        struct MockConnector;
        #[async_trait]
        impl wire::Connector for MockConnector {
            async fn connect(&self, _timeout: Option<Duration>) -> Result<wire::BoxedConnection> {
                let mut mock_conn = wire::MockMockConnection::new();
                mock_conn.expect_subscribe_upstream().return_once(|_| {
                    let (_s, r) = mpsc::channel(1);
                    Ok(r)
                });

                mock_conn
                    .expect_open_request()
                    .return_once(move |_| Ok(msg::ConnectResponse::default()));
                mock_conn.expect_disconnect().returning(move |_| Ok(()));

                mock_conn
                    .expect_subscribe_downstream_call()
                    .return_once(|| {
                        let (_, r) = broadcast::channel(1);
                        r
                    });

                let (close_s, _) = broadcast::channel(1);
                let close_s_c = close_s.clone();
                mock_conn.expect_close().return_once(move || {
                    let _ = close_s_c.send(());
                    Ok(())
                });
                mock_conn
                    .expect_subscribe_close_notify()
                    .returning(move || wire::CloseNotificationReceiver::new(close_s.subscribe()));

                mock_conn.expect_upstream_open_request().returning(|req| {
                    if req.session_id == "OK" {
                        return Ok(msg::UpstreamOpenResponse {
                            assigned_stream_id_alias: 1,
                            result_code: msg::ResultCode::Succeeded,
                            ..Default::default()
                        });
                    } else if req.session_id == "TIMEOUT" {
                        return Err(Error::TimeOut);
                    }
                    Ok(msg::UpstreamOpenResponse {
                        result_code: msg::ResultCode::UnspecifiedError,
                        ..Default::default()
                    })
                });
                mock_conn
                    .expect_unsubscribe_downstream_call()
                    .return_once(|| ());

                let res = Box::new(mock_conn);
                Ok(res)
            }
        }
        let repo = super::super::InMemStreamRepository::new();
        let repo = Arc::new(repo);
        let cli = ConnBuilder {
            config: ConnConfig::<StaticTokenSource> {
                node_id: "test_edge".to_string(),
                project_uuid: None,
                ping_interval: chrono::Duration::seconds(10),
                ping_timeout: chrono::Duration::seconds(1),
                token_source: None,
                ..Default::default()
            },
            sent_storage: Arc::new(super::super::InMemSentStorageNoPayload::default()),
            downstream_repository: repo.clone(),
            upstream_repository: repo.clone(),
        };
        let conn = cli
            .connect_with_connector(Box::new(MockConnector {}))
            .await
            .expect("no error");

        // OK
        let _up = conn.open_upstream("OK", vec![]).await.unwrap();

        // NG Result code
        let res = conn.open_upstream("NG", vec![]).await;
        assert!(res.is_err());

        // Error
        let res = conn.open_upstream("TIMEOUT", vec![]).await;
        assert!(res.is_err());

        // closed
        conn.close().await.unwrap();
        let res = conn.open_upstream("OK", vec![]).await;
        assert!(res.is_err())
    }

    #[ignore] // TODO: enable test
    #[tokio::test]
    async fn downstream_open_request() {
        struct MockConnector;
        #[async_trait]
        impl wire::Connector for MockConnector {
            async fn connect(&self, _timeout: Option<Duration>) -> Result<wire::BoxedConnection> {
                let mut mock_conn = wire::MockMockConnection::new();

                mock_conn
                    .expect_open_request()
                    .return_once(move |_| Ok(msg::ConnectResponse::default()));
                mock_conn.expect_disconnect().returning(move |_| Ok(()));
                mock_conn.expect_close().returning(move || Ok(()));

                mock_conn
                    .expect_subscribe_downstream_call()
                    .return_once(|| {
                        let (_, r) = broadcast::channel(1);
                        r
                    });

                let (close_s, _) = broadcast::channel(1);
                let close_s_c = close_s.clone();
                mock_conn.expect_close().return_once(move || {
                    let _ = close_s_c.send(());
                    Ok(())
                });
                mock_conn
                    .expect_subscribe_close_notify()
                    .returning(move || wire::CloseNotificationReceiver::new(close_s.subscribe()));

                mock_conn.expect_subscribe_downstream().returning(|_, _| {
                    let (_s, r) = mpsc::channel(1);
                    Ok(r)
                });

                mock_conn.expect_subscribe_downstream_meta().returning(|| {
                    let (_s, r) = broadcast::channel(1);
                    r
                });

                mock_conn.expect_downstream_open_request().returning(|req| {
                    let id = req
                        .downstream_filters
                        .first()
                        .unwrap()
                        .source_node_id
                        .clone();

                    if id == "OK" {
                        return Ok(msg::DownstreamOpenResponse {
                            assigned_stream_id: Uuid::from_str(
                                "11111111-1111-1111-1111-111111111111",
                            )
                            .unwrap(),
                            result_code: msg::ResultCode::Succeeded,
                            ..Default::default()
                        });
                    } else if id == "TIMEOUT" {
                        return Err(Error::TimeOut);
                    }
                    Ok(msg::DownstreamOpenResponse {
                        result_code: msg::ResultCode::UnspecifiedError,
                        ..Default::default()
                    })
                });
                mock_conn
                    .expect_unsubscribe_downstream_call()
                    .return_once(|| ());

                let res = Box::new(mock_conn);
                Ok(res)
            }
        }
        let repo = super::super::InMemStreamRepository::new();
        let repo = Arc::new(repo);
        let cli = ConnBuilder {
            config: ConnConfig::<StaticTokenSource> {
                node_id: "test_edge".to_string(),
                project_uuid: None,
                ping_interval: chrono::Duration::seconds(10),
                ping_timeout: chrono::Duration::seconds(1),
                token_source: None,
                ..Default::default()
            },
            sent_storage: Arc::new(super::super::InMemSentStorageNoPayload::default()),
            downstream_repository: repo.clone(),
            upstream_repository: repo.clone(),
        };
        let conn = cli
            .connect_with_connector(Box::new(MockConnector {}))
            .await
            .expect("no error");

        // OK
        let _up = conn
            .open_downstream(
                vec![msg::DownstreamFilter {
                    source_node_id: "OK".to_string(),
                    ..Default::default()
                }],
                vec![],
            )
            .await
            .unwrap();

        // NG Result code
        let res = conn
            .open_downstream(
                vec![msg::DownstreamFilter {
                    source_node_id: "NG".to_string(),
                    ..Default::default()
                }],
                vec![],
            )
            .await;
        assert!(res.is_err());

        // Error
        let res = conn
            .open_downstream(
                vec![msg::DownstreamFilter {
                    source_node_id: "TIMEOUT".to_string(),
                    ..Default::default()
                }],
                vec![],
            )
            .await;
        assert!(res.is_err());

        // closed
        conn.close().await.unwrap();
        let res = conn
            .open_downstream(
                vec![msg::DownstreamFilter {
                    source_node_id: "OK".to_string(),
                    ..Default::default()
                }],
                vec![],
            )
            .await;
        assert!(res.is_err())
    }
}
