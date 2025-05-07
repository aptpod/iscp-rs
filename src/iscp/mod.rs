mod builder;
mod down_order;
mod down_state;
mod downstream;
mod e2e;
mod flush_policy;
pub mod metadata;
mod misc;
mod storage;
mod types;
mod upstream;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::sync::{watch, Semaphore};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    encoding::{Encoding, EncodingBuilder},
    error::Error,
    internal::{WaitGroup, Waiter},
    token_source::{SharedTokenSource, TokenSource},
    transport::{Compression, CompressionType, Connector, NegotiationParams},
    wire::Conn as WireConn,
};

pub use builder::*;
pub use down_state::DownstreamState;
pub use downstream::{
    Downstream, DownstreamConfig, DownstreamMetadataReader, DownstreamReordering,
};
pub use flush_policy::FlushPolicy;
pub use misc::CallbackReturnValue;
pub use types::*;
pub use upstream::{
    ReceiveAckCallback, SendDataPointsCallback, Upstream, UpstreamCloseOptions, UpstreamConfig,
    UpstreamResumedCallback, UpstreamState,
};

/// [Conn] のビルダー型
#[derive(Clone)]
pub struct ConnBuilder<C> {
    connector: C,
    encoding: Encoding,
    compression: Compression,
    node_id: String,
    project_uuid: Option<Uuid>,
    ping_interval: Duration,
    ping_timeout: Duration,
    response_message_timeout: Duration,
    token_source: Option<SharedTokenSource>,
    channel_size: usize,
}

impl<C: Connector> ConnBuilder<C> {
    /// ビルダーを作成
    pub fn new(connector: C) -> Self {
        Self {
            connector,
            encoding: EncodingBuilder::default().build(),
            compression: Compression::new(),
            node_id: String::new(),
            project_uuid: None,
            ping_interval: Duration::from_secs(10),
            ping_timeout: Duration::from_secs(1),
            response_message_timeout: Duration::from_secs(3),
            token_source: None,
            channel_size: 1024,
        }
    }

    /// 接続開始
    pub async fn build(mut self) -> Result<Conn, Error> {
        if rustls::crypto::CryptoProvider::get_default().is_none() {
            let _ = rustls::crypto::ring::default_provider().install_default();
        }
        if self.compression.window_bits().is_some() && !C::_compat_with_context_takeover() {
            return Err(Error::Unexpected(
                "using window bits for unsupported transport".into(),
            ));
        }

        let wire_conn = self.connect().await?;

        Ok(Conn {
            inner: Arc::new(ConnInner::new(wire_conn, self)),
        })
    }

    /// エンコード方法を設定
    pub fn encoding(mut self, encoding: Encoding) -> Self {
        self.encoding = encoding;
        self
    }

    /// 圧縮方法を設定
    pub fn compression(mut self, compression: Compression) -> Self {
        self.compression = compression;
        self
    }

    /// ノードIDを設定
    pub fn node_id<S: ToString>(mut self, node_id: S) -> Self {
        self.node_id = node_id.to_string();
        self
    }

    /// プロジェクトUUIDを設定
    pub fn project_uuid<T: Into<Option<Uuid>>>(mut self, project_uuid: T) -> Self {
        self.project_uuid = project_uuid.into();
        self
    }

    /// Ping間隔を設定
    pub fn ping_interval(mut self, ping_interval: Duration) -> Self {
        self.ping_interval = ping_interval;
        self
    }

    /// Pingタイムアウトを設定
    pub fn ping_timeout(mut self, ping_timeout: Duration) -> Self {
        self.ping_timeout = ping_timeout;
        self
    }

    /// 返信タイムアウトを設定
    pub fn response_message_timeout(mut self, response_message_timeout: Duration) -> Self {
        self.response_message_timeout = response_message_timeout;
        self
    }

    /// トークンソースを設定
    pub fn token_source<T: Into<Option<TS>>, TS: TokenSource>(mut self, token_source: T) -> Self {
        self.token_source = token_source
            .into()
            .map(|token_source| SharedTokenSource::new(token_source));
        self
    }

    async fn connect(&mut self) -> Result<WireConn, Error> {
        let transport = self.connector.connect(self.negotiation_params()).await?;
        let conn = WireConn::new::<C>(
            transport,
            self.encoding.clone(),
            self.compression.clone(),
            self.channel_size,
            self.response_message_timeout,
            self.ping_interval,
            self.ping_timeout,
        );

        let Ok(ping_interval) = self.ping_interval.as_secs().try_into() else {
            return Err(Error::invalid_value("invalid ping_interval"));
        };
        if ping_interval == 0 {
            return Err(Error::invalid_value("invalid ping_interval"));
        }
        let Ok(ping_timeout) = self.ping_timeout.as_secs().try_into() else {
            return Err(Error::invalid_value("invalid ping_timeout"));
        };
        if ping_timeout == 0 {
            return Err(Error::invalid_value("invalid ping_timeout"));
        }

        let intdash_extension_fields =
            self.project_uuid
                .map(|uuid| crate::message::extensions::IntdashExtensionFields {
                    project_uuid: uuid.to_string(),
                });
        let extension_fields = if let Some(token_source) = &self.token_source {
            let access_token =
                tokio::time::timeout(self.response_message_timeout, token_source.token())
                    .await
                    .map_err(|_| Error::timeout("token source"))??;

            Some(crate::message::extensions::ConnectRequestExtensionFields {
                access_token: access_token.0,
                intdash: intdash_extension_fields,
            })
        } else if intdash_extension_fields.is_some() {
            Some(crate::message::extensions::ConnectRequestExtensionFields {
                access_token: "".into(),
                intdash: intdash_extension_fields,
            })
        } else {
            None
        };

        let connect_request = crate::message::ConnectRequest {
            protocol_version: crate::ISCP_VERSION.into(),
            node_id: self.node_id.clone(),
            ping_interval,
            ping_timeout,
            extension_fields,
            ..Default::default()
        };
        let _response = conn.request_message_need_response(connect_request).await?;

        log::info!("successfully received connect response message");

        Ok(conn)
    }

    fn negotiation_params(&self) -> NegotiationParams {
        let encoding_name = self.encoding.name().into();

        if !self.compression.enabled() {
            NegotiationParams {
                encoding_name,
                compression_type: None,
                compression_level: None,
                compression_window_bits: None,
            }
        } else {
            NegotiationParams {
                encoding_name,
                compression_type: if self.compression.window_bits().is_none() {
                    Some(CompressionType::PerMessage)
                } else {
                    Some(CompressionType::ContextTakeover)
                },
                compression_level: self.compression.level(),
                compression_window_bits: self.compression.window_bits(),
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum ConnectionState {
    Connected,
    Reconnecting,
    Closing,
    Closed,
}

/// iSCP接続
#[derive(Clone)]
pub struct Conn {
    inner: Arc<ConnInner>,
}

impl std::fmt::Debug for Conn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Conn").finish()
    }
}

impl Conn {
    pub async fn open_upstream_with_config<T: Into<Arc<UpstreamConfig>>>(
        &self,
        upstream_config: T,
    ) -> Result<Upstream, Error> {
        self.check_close()?;

        let (upstream, ct) = Upstream::new(
            upstream_config.into(),
            self.inner.shared_wire_conn.clone(),
            self.inner.stream_wait_group(),
        )
        .await?;
        self.inner.push_stream_ct(ct);
        Ok(upstream)
    }

    pub fn upstream_builder<S: ToString>(&self, session_id: S) -> UpstreamBuilder {
        let config = UpstreamConfig {
            session_id: session_id.to_string(),
            ..Default::default()
        };
        UpstreamBuilder {
            conn: self.clone(),
            config,
        }
    }

    pub async fn open_downstream_with_config<T: Into<Arc<DownstreamConfig>>>(
        &self,
        downstream_config: T,
    ) -> Result<(Downstream, DownstreamMetadataReader), Error> {
        self.check_close()?;

        let (downstream, metadata_receiver, ct) = Downstream::new(
            downstream_config.into(),
            self.inner.shared_wire_conn.clone(),
            self.inner.stream_wait_group(),
            self.inner.channel_size,
        )
        .await?;
        self.inner.push_stream_ct(ct);
        Ok((downstream, metadata_receiver))
    }

    pub fn downstream_builder(&self, filters: Vec<DownstreamFilter>) -> DownstreamBuilder {
        let config = DownstreamConfig {
            filters,
            ..Default::default()
        };
        DownstreamBuilder {
            conn: self.clone(),
            config,
        }
    }

    pub async fn close(&self) -> Result<(), Error> {
        let Ok(_permit) = self.inner.close_semaphore.acquire().await else {
            return Ok(());
        };
        let _ = self.inner.tx_state.send(ConnectionState::Closing);

        // Wait for close of streams
        let (stream_ct, stream_waiter) = self.inner.take_stream_waiter();
        for ct in stream_ct.into_iter() {
            ct.cancel();
        }
        stream_waiter.wait().await;

        // Wait for reconnection loop exit
        self.inner.ct.cancel();
        self.inner.waiter.wait().await;
        self.inner.close_semaphore.close();
        Ok(())
    }

    /// Wait for the given connection state.
    async fn wait_for_state(&self, state: ConnectionState) -> Result<(), Error> {
        let mut rx = self.inner.rx_state.clone();
        let s = rx
            .wait_for(|s| *s == state || *s == ConnectionState::Closed)
            .await
            .map_err(|_| Error::ConnectionClosed)?;
        if *s == state {
            Ok(())
        } else {
            // Returns error if closed while waiting states other than Closed state.
            Err(Error::ConnectionClosed)
        }
    }

    fn check_close(&self) -> Result<(), Error> {
        if self.inner.close_semaphore.available_permits() == 0 || self.inner.ct.is_cancelled() {
            Err(Error::ConnectionClosed)
        } else {
            Ok(())
        }
    }
}

struct ConnInner {
    shared_wire_conn: SharedWireConn,
    ct: CancellationToken,
    waiter: Waiter,
    stream_ct: Mutex<Vec<CancellationToken>>,
    stream_close_waiter: Mutex<Option<(Waiter, WaitGroup)>>,
    close_semaphore: Semaphore,
    tx_state: watch::Sender<ConnectionState>,
    rx_state: watch::Receiver<ConnectionState>,
    channel_size: usize,
}

impl ConnInner {
    fn new<C: Connector>(wire_conn: WireConn, builder: ConnBuilder<C>) -> Self {
        let ct = CancellationToken::new();
        let (waiter, wg) = Waiter::new();
        let (tx_wire_conn, rx_wire_conn) = watch::channel(wire_conn.clone());
        let (tx_state, rx_state) = watch::channel(ConnectionState::Connected);
        let channel_size = builder.channel_size;

        let ct_clone = ct.clone();
        let tx_state_clone = tx_state.clone();
        tokio::spawn(async move {
            reconnection_loop(wire_conn, tx_wire_conn, &tx_state_clone, builder, ct_clone).await;
            log::debug!("exit reconnection loop");
            let _ = tx_state_clone.send(ConnectionState::Closed);
            std::mem::drop(wg);
        });

        ConnInner {
            shared_wire_conn: SharedWireConn(rx_wire_conn),
            ct,
            waiter,
            stream_ct: Mutex::new(Vec::new()),
            stream_close_waiter: Mutex::new(Some(Waiter::new())),
            close_semaphore: Semaphore::new(1),
            tx_state,
            rx_state,
            channel_size,
        }
    }

    fn wire_conn(&self) -> Result<WireConn, Error> {
        let wire_conn = self.shared_wire_conn.get();
        if !wire_conn.is_connected() {
            return Err(Error::ConnectionClosed);
        }
        Ok(wire_conn)
    }

    fn push_stream_ct(&self, ct: CancellationToken) {
        self.stream_ct.lock().unwrap().push(ct);
    }

    fn stream_wait_group(&self) -> WaitGroup {
        let guard = self.stream_close_waiter.lock().unwrap();
        guard.as_ref().unwrap().1.clone()
    }

    fn take_stream_waiter(&self) -> (Vec<CancellationToken>, Waiter) {
        (
            std::mem::take(&mut *self.stream_ct.lock().unwrap()),
            self.stream_close_waiter.lock().unwrap().take().unwrap().0,
        )
    }
}

async fn reconnection_loop<C: Connector>(
    mut wire_conn: WireConn,
    tx_wire_conn: watch::Sender<WireConn>,
    tx_state: &watch::Sender<ConnectionState>,
    mut builder: ConnBuilder<C>,
    ct: CancellationToken,
) {
    loop {
        let _ = tx_state.send(ConnectionState::Connected);

        tokio::select! {
            _ = wire_conn.cancelled() => (),
            _ = ct.cancelled() => {
                let msg = crate::message::Disconnect {
                    result_code: crate::message::ResultCode::Succeeded.into(),
                    result_string: "OK".into(),
                    ..Default::default()
                };
                if let Err(e) = wire_conn.send_message(msg).await {
                    log::warn!("cannot send disconnect message: {}", e);
                }
                if let Err(e) = wire_conn.close().await {
                    log::error!("close error: {}", e);
                }
                return;
            }
        }

        let mut waiter = misc::ReconnectWaiter::new();
        let _ = tx_state.send(ConnectionState::Reconnecting);

        loop {
            waiter.wait().await;

            let result = tokio::select! {
                result = builder.connect() => result,
                _ = ct.cancelled() => {
                    return;
                }
            };
            match result {
                Ok(c) => {
                    wire_conn = c.clone();
                    if tx_wire_conn.send(c).is_err() {
                        return;
                    }
                    break;
                }
                Err(e) => {
                    log::info!("reconnection failed: {}", e);
                }
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct SharedWireConn(watch::Receiver<WireConn>);

impl SharedWireConn {
    pub fn get(&self) -> WireConn {
        self.0.borrow().clone()
    }

    pub async fn get_updated(&mut self) -> Result<WireConn, Error> {
        if self.0.changed().await.is_err() {
            return Err(Error::ConnectionClosed);
        }
        Ok(self.0.borrow().clone())
    }

    // pub async fn get_connected(mut self) -> Result<WireConn, Error> {
    //     self.0
    //         .wait_for(|wire_conn| wire_conn.is_connected())
    //         .await
    //         .map(|wire_conn| wire_conn.clone())
    //         .map_err(|_| Error::ConnectionClosed)
    // }
}
