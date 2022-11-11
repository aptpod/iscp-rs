//! iSCPのライブラリを提供します。

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::{
    collections::HashMap,
    fmt::Debug,
    net::{SocketAddr, ToSocketAddrs},
    sync::Arc,
};
use url::Url;
use uuid::Uuid;

use crate::{enc, msg, tr, wire, Error, Result};

#[cfg(test)]
use mockall::predicate::*;

mod connection;
mod data;
mod downstream;
pub mod downstream_options;
mod flush_policy;
mod hooker;
mod metadata;
mod state;
mod storage;
mod upstream;
pub mod upstream_options;
pub use connection::*;
pub use data::*;
pub use downstream::*;
pub use downstream_options as down_opts;
pub use flush_policy::*;
pub use hooker::*;
pub use metadata::*;
pub(self) use state::*;
pub(self) use storage::*;
pub use upstream::*;
pub use upstream_options as up_opts;

pub type DownstreamFilter = msg::DownstreamFilter;
pub type QoS = msg::QoS;

/// 認証トークンを取得するためのインターフェースです。
#[async_trait]
pub trait TokenSource: Sync + Send {
    /// トークンを取得します。
    ///
    ///  iSCPコネクションを開く度に、このメソッドをコールしトークンを取得します。
    async fn token(&self) -> Result<String>;
}

/// 静的に認証トークンを指定するTokenSourceです。
///
/// APIトークンなどを使用するための実装です。クライアントシークレットには使用できません。
#[derive(Clone, Debug, Default)]
pub struct StaticTokenSource {
    token: msg::AccessToken,
}

impl StaticTokenSource {
    /// StaticTokenSource を生成します。
    pub fn new<T: ToString>(token: T) -> Self {
        Self {
            token: msg::AccessToken::new(token),
        }
    }
}

#[async_trait]
impl TokenSource for StaticTokenSource {
    /// トークンを取得します。
    ///
    /// 常に同じトークンを返却します。
    async fn token(&self) -> Result<String> {
        Ok(self.token.clone().into())
    }
}

#[async_trait]
impl<T: TokenSource + Sync + Send + Clone> TokenSource for Box<T> {
    async fn token(&self) -> Result<String> {
        (**self).token().await
    }
}

/// トランスポートです。
#[derive(Clone, Debug)]
pub enum Transport {
    /// QUICトランスポート
    Quic,
    /// WebSocketトランスポート
    Ws,
}

/// [`Conn`]のビルダーです。
pub struct ConnBuilder<T>
where
    T: TokenSource + Clone,
{
    config: ConnConfig<T>,
    sent_storage: Arc<dyn SentStorage>,
    upstream_repository: Arc<dyn UpstreamRepository>,
    downstream_repository: Arc<dyn DownstreamRepository>,
}

/// [`Conn`]の生成に使用するパラメーターです。
#[derive(Clone)]
pub struct ConnConfig<T>
where
    T: TokenSource + Clone,
{
    pub address: String,
    pub transport: Transport,
    pub websocket_config: Option<tr::WebSocketConfig>,
    pub quic_config: Option<tr::QuicConfig>,
    pub encoding: enc::Encoding,
    pub node_id: String,
    pub project_uuid: Option<String>,
    pub ping_interval: chrono::Duration,
    pub ping_timeout: chrono::Duration,
    pub token_source: Option<T>,
}

impl<T> Default for ConnConfig<T>
where
    T: TokenSource + Clone,
{
    fn default() -> Self {
        Self {
            address: "localhost:8080".to_string(),
            transport: Transport::Quic,
            websocket_config: None,
            quic_config: None,
            encoding: enc::Encoding::Proto,
            node_id: String::new(),
            project_uuid: None,
            ping_interval: chrono::Duration::seconds(10),
            ping_timeout: chrono::Duration::seconds(1),
            token_source: None,
        }
    }
}

impl<T> ConnBuilder<T>
where
    T: TokenSource + Clone + 'static,
{
    pub fn new(address: &str, transport: Transport) -> Self {
        Self::new_with_conn_config(ConnConfig {
            address: address.to_string(),
            transport,
            ..Default::default()
        })
    }

    pub fn new_with_conn_config(c: ConnConfig<T>) -> Self {
        Self {
            config: c,
            upstream_repository: Arc::new(InMemStreamRepository::new()),
            downstream_repository: Arc::new(InMemStreamRepository::new()),
            sent_storage: Arc::new(InMemSentStorage::new()),
        }
    }

    pub fn websocket_config(mut self, websocket_config: Option<tr::WebSocketConfig>) -> Self {
        self.config.websocket_config = websocket_config;
        self
    }

    pub fn quic_config(mut self, quic_config: Option<tr::QuicConfig>) -> Self {
        self.config.quic_config = quic_config;
        self
    }

    pub fn encoding(mut self, encoding: enc::Encoding) -> Self {
        self.config.encoding = encoding;
        self
    }

    pub fn node_id<S: ToString>(mut self, node_id: S) -> Self {
        self.config.node_id = node_id.to_string();
        self
    }

    pub fn project_uuid<S: ToString>(mut self, project_uuid: S) -> Self {
        self.config.project_uuid = Some(project_uuid.to_string());
        self
    }

    pub fn ping_interval(mut self, d: chrono::Duration) -> Self {
        self.config.ping_interval = d;
        self
    }

    pub fn ping_timeout(mut self, d: chrono::Duration) -> Self {
        self.config.ping_timeout = d;
        self
    }

    pub fn token_source(mut self, token_source: Option<T>) -> Self {
        self.config.token_source = token_source;
        self
    }

    #[allow(dead_code)]
    pub(crate) fn upstream_repository<U>(mut self, upstream_repository: U) -> Self
    where
        U: UpstreamRepository + 'static,
    {
        self.upstream_repository = Arc::new(upstream_repository);
        self
    }

    #[allow(dead_code)]
    pub(crate) fn downstream_repository<D>(mut self, downstream_repository: D) -> Self
    where
        D: DownstreamRepository + 'static,
    {
        self.downstream_repository = Arc::new(downstream_repository);
        self
    }

    #[allow(dead_code)]
    pub(crate) fn sent_storage<S>(mut self, sent_storage: S) -> Self
    where
        S: SentStorage + 'static,
    {
        self.sent_storage = Arc::new(sent_storage);
        self
    }

    pub async fn connect(self) -> Result<Conn<T>> {
        let connector = self.connect_wire().await?;
        self.connect_with_connector(connector).await
    }

    async fn connect_wire(&self) -> Result<wire::BoxedConnector> {
        let tr_connector: tr::Connector = match self.config.transport {
            Transport::Ws => {
                let cfg = self.config.websocket_config.clone().unwrap_or_default();

                let scheme = if cfg.enable_tls { "https" } else { "http" };
                let (host, port) = parse_host_and_port(scheme, &self.config.address)?;

                tr::WebSocketConnector::new(host, Some(port), cfg).into()
            }
            Transport::Quic => {
                let cfg = self.config.quic_config.clone().unwrap_or_default();

                let (host, port) = parse_host_and_port("https", &self.config.address)?;
                let sock_addr = format!("{}:{}", host, port)
                    .as_str()
                    .to_socket_addrs()
                    .unwrap()
                    .find(|x| matches!(x, SocketAddr::V4(_)))
                    .unwrap();
                tr::QuicConnector::new(sock_addr, cfg).into()
            }
        };

        Ok(Box::new(wire::new_connector(
            tr_connector,
            self.config.encoding,
        )))
    }

    async fn connect_with_connector(self, connector: wire::BoxedConnector) -> Result<Conn<T>> {
        // Validate
        if self.config.node_id.is_empty() {
            return Err(Error::invalid_value("need any edge id"));
        }
        if self.config.ping_interval.num_seconds() < 1i64 {
            return Err(Error::invalid_value("must ping interval >= 1 sec"));
        }
        if self.config.ping_timeout.num_seconds() < 1i64 {
            return Err(Error::invalid_value("must ping timeout >= 1 sec"));
        }

        // TODO: config timeout for this connect()
        let wire_conn = connector.connect(None).await?;

        let token = if let Some(ts) = &self.config.token_source {
            Some(ts.token().await?)
        } else {
            None
        };

        let resp = wire_conn
            .open_request(msg::ConnectRequest {
                node_id: self.config.node_id.clone(),
                protocol_version: crate::ISCP_VERSION.to_string(),
                access_token: token.map(msg::AccessToken::new),
                ping_interval: self.config.ping_interval,
                ping_timeout: self.config.ping_timeout,
                project_uuid: self.config.project_uuid.clone(),
                ..Default::default()
            })
            .await?;
        verify_connect_response(resp)?;

        let conn = Conn::new(
            wire_conn,
            self.sent_storage.clone(),
            self.upstream_repository.clone(),
            self.downstream_repository.clone(),
            self.config,
        );

        Ok(conn)
    }
}

fn parse_host_and_port(scheme: &str, address: &str) -> Result<(String, u16)> {
    if let Ok(url) = Url::parse(&format!("{}://{}", scheme, address)) {
        let port = url
            .port_or_known_default()
            .expect("scheme must be 'http' or 'https'");
        if let Some(host) = url.host_str() {
            return Ok((host.to_string(), port));
        }
    }

    Err(Error::invalid_value("invalid address"))
}

fn verify_connect_response(resp: msg::ConnectResponse) -> Result<()> {
    match resp.result_code {
        msg::ResultCode::Succeeded => Ok(()),
        msg::ResultCode::IncompatibleVersion => Ok(()),
        _ => Err(resp.into()),
    }
}

#[doc(hidden)]
/// アップストリームの情報です。
#[derive(Clone, Default, Debug)]
pub struct UpstreamInfo {
    pub session_id: String,
    pub stream_id: Uuid,
    pub stream_id_alias: u32,
    pub id_alias_map: IdAliasMap,
    pub flush_policy: FlushPolicy,
    pub sequence_number: u32,
    pub data_point_count: u64,
    pub qos: msg::QoS,
    pub server_time: DateTime<Utc>,
}

#[doc(hidden)]
/// アップストリーム永続化用のインターフェースです。
pub trait UpstreamRepository: Sync + Send {
    /// アップストリームを永続化します。
    fn save_upstream(&self, info: &UpstreamInfo) -> Result<()>;
    /// アップストリームを取得します。
    fn find_upstream_by_id(&self, uuid: Uuid) -> Result<UpstreamInfo>;
    /// アップストリームを削除します。
    fn remove_upstream_by_id(&self, uuid: Uuid) -> Result<()>;
}

#[doc(hidden)]
pub type BoxedUpstreamRepository = Box<dyn UpstreamRepository>;

impl<R: UpstreamRepository> UpstreamRepository for Box<R> {
    fn save_upstream(&self, info: &UpstreamInfo) -> Result<()> {
        (**self).save_upstream(info)
    }
    fn find_upstream_by_id(&self, uuid: Uuid) -> Result<UpstreamInfo> {
        (**self).find_upstream_by_id(uuid)
    }
    fn remove_upstream_by_id(&self, uuid: Uuid) -> Result<()> {
        (**self).remove_upstream_by_id(uuid)
    }
}

impl<R: UpstreamRepository> UpstreamRepository for Arc<R> {
    fn save_upstream(&self, info: &UpstreamInfo) -> Result<()> {
        (**self).save_upstream(info)
    }
    fn find_upstream_by_id(&self, uuid: Uuid) -> Result<UpstreamInfo> {
        (**self).find_upstream_by_id(uuid)
    }
    fn remove_upstream_by_id(&self, uuid: Uuid) -> Result<()> {
        (**self).remove_upstream_by_id(uuid)
    }
}

#[doc(hidden)]
/// ダウンストリームの情報です。
#[allow(dead_code)] // TODO: remove me
#[derive(Clone, Default, Debug)]
pub struct DownstreamInfo {
    ack_id: u32,
    stream_id: Uuid,
    upstreams_info: HashMap<u32, msg::UpstreamInfo>,
    data_ids: HashMap<u32, msg::DataId>,
    source_node_ids: Vec<String>,
    qos: msg::QoS,
    last_recv_sequences: HashMap<msg::UpstreamInfo, u32>,
    server_time: chrono::DateTime<chrono::Utc>,
}

#[doc(hidden)]
/// ダウンストリーム永続化用のインターフェースです。
pub trait DownstreamRepository: Sync + Send {
    /// ダウンストリームを永続化します。
    fn save_downstream(&self, info: &DownstreamInfo) -> Result<()>;
    /// ダウンストリームを取得します。
    fn find_downstream_by_id(&self, uuid: Uuid) -> Result<DownstreamInfo>;
    /// ダウンストリームを削除します。
    fn remove_downstream_by_id(&self, uuid: Uuid) -> Result<()>;
}

#[doc(hidden)]
pub type BoxedDownstreamRepository = Box<dyn DownstreamRepository>;

impl<R: DownstreamRepository> DownstreamRepository for Box<R> {
    fn save_downstream(&self, info: &DownstreamInfo) -> Result<()> {
        (**self).save_downstream(info)
    }
    fn find_downstream_by_id(&self, uuid: Uuid) -> Result<DownstreamInfo> {
        (**self).find_downstream_by_id(uuid)
    }
    fn remove_downstream_by_id(&self, uuid: Uuid) -> Result<()> {
        (**self).remove_downstream_by_id(uuid)
    }
}

impl<R: DownstreamRepository> DownstreamRepository for Arc<R> {
    fn save_downstream(&self, info: &DownstreamInfo) -> Result<()> {
        (**self).save_downstream(info)
    }
    fn find_downstream_by_id(&self, uuid: Uuid) -> Result<DownstreamInfo> {
        (**self).find_downstream_by_id(uuid)
    }
    fn remove_downstream_by_id(&self, uuid: Uuid) -> Result<()> {
        (**self).remove_downstream_by_id(uuid)
    }
}
