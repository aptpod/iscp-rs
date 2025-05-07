//! iscpクレートは、iSCP version 2を用いたリアルタイムAPIにアクセスするためのクライアントライブラリです。
//!
//! iscpクレートを使用することで、iSCPを利用するクライアントアプリケーションを実装することができます。
//!
//! iSCPで通信を行うためには、コネクション[`iscp::Conn`]を確立した後、ストリームやE2Eコールを使用してデータを送受信してください。
//!
//! # Examples
//!
//! ## Connect To intdash API
//!
//! このサンプルではintdash APIとのコネクションを確立します。
//!
//! ```
//! #[derive(Clone)]
//! struct TokenSource {
//!     access_token: String,
//! }
//!
//! impl iscp::TokenSource for TokenSource {
//!     async fn token(&mut self) -> Result<iscp::AccessToken, iscp::TokenSourceError> {
//!         Ok(iscp::AccessToken::new(&self.access_token))
//!     }
//! }
//!
//! let url = std::env::var("EXAMPLE_URL").unwrap_or_else(|_| "wss://xxx.xxx.jp".to_string());
//! let api_token = std::env::var("EXAMPLE_TOKEN").unwrap_or_else(|_| {
//!     "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string()
//! });
//! let node_id = std::env::var("EXAMPLE_NODE_ID")
//!     .unwrap_or_else(|_| "11111111-1111-1111-1111-111111111111".to_string());
//!
//! let token_source = TokenSource {
//!     // Get an access token by oauth2 from intdash API in actual usage
//!     access_token: api_token,
//! };
//!
//! let connector = iscp::transport::websocket::WebSocketConnector::new(url);
//! let conn = iscp::ConnBuilder::new(connector)
//!     .node_id(node_id)
//!     .compression(iscp::transport::Compression::new().enable(9))
//!     .token_source(token_source)
//!     .build()
//!     .await?;
//!
//! conn.close().await?;
//! ```
//!
//! ## Start Upstream
//!
//! アップストリームの送信サンプルです。このサンプルでは、コネクションからアップストリームを開き、基準時刻のメタデータと、文字列型のデータポイントをiSCPサーバーへ送信しています。
//!
//! ```
//! #[derive(Clone)]
//! struct TokenSource {
//!     access_token: String,
//! }
//!
//! impl iscp::TokenSource for TokenSource {
//!     async fn token(&mut self) -> Result<iscp::AccessToken, iscp::TokenSourceError> {
//!         Ok(iscp::AccessToken::new(&self.access_token))
//!     }
//! }
//!
//! let url = std::env::var("EXAMPLE_URL").unwrap_or_else(|_| "wss://xxx.xxx.jp".to_string());
//! let api_token = std::env::var("EXAMPLE_TOKEN").unwrap_or_else(|_| {
//!     "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string()
//! });
//! let node_id = std::env::var("EXAMPLE_NODE_ID")
//!     .unwrap_or_else(|_| "11111111-1111-1111-1111-111111111111".to_string());
//!
//! let token_source = TokenSource {
//!     // Get an access token by oauth2 from intdash API in actual usage
//!     access_token: api_token,
//! };
//!
//! let connector = iscp::transport::websocket::WebSocketConnector::new(url);
//! let conn = iscp::ConnBuilder::new(connector)
//!     .node_id(node_id)
//!     .compression(iscp::transport::Compression::new().enable(9))
//!     .token_source(token_source)
//!     .build()
//!     .await?;
//!
//! // Open an upstream
//! let session_id = uuid::Uuid::new_v4();
//! let mut up = conn
//!     .upstream_builder(session_id.to_string())
//!     .flush_policy(iscp::FlushPolicy::Immediately)
//!     .expiry_interval(std::time::Duration::from_secs(60))
//!     .persist(true)
//!     .build()
//!     .await?;
//!
//! // Send base time
//! let start_clock = std::time::Instant::now();
//! let base_time = iscp::metadata::BaseTime {
//!     base_time: std::time::SystemTime::now(),
//!     elapsed_time: std::time::Duration::from_secs(0),
//!     name: "edge_rtc".into(),
//!     priority: 0,
//!     session_id: session_id.to_string(),
//! };
//! conn.metadata_sender(base_time).persist(true).send().await?;
//!
//! // Send a data point
//! let data_point = iscp::DataPoint {
//!     payload: b"hello, world".to_vec().into(),
//!     elapsed_time: (std::time::Instant::now() - start_clock).as_nanos() as _,
//! };
//! up.write_data_points(
//!     iscp::DataId::new("greeting", "string"),
//!     vec![data_point],
//! )
//! .await?;
//!
//! conn.close().await?;
//! ```
//!
//! ## Start Downstream
//!
//! 前述のアップストリームで送信されたデータをダウンストリームで受信するサンプルです。 このサンプルでは、アップストリーム開始のメタデータ、基準時刻のメタデータ、 文字列型のデータポイントを受信しています。
//!
//! ```
//! #[derive(Clone)]
//! struct TokenSource {
//!     access_token: String,
//! }
//!
//! impl iscp::TokenSource for TokenSource {
//!     async fn token(&mut self) -> Result<iscp::AccessToken, iscp::TokenSourceError> {
//!         Ok(iscp::AccessToken::new(&self.access_token))
//!     }
//! }
//!
//! let url = std::env::var("EXAMPLE_URL").unwrap_or_else(|_| "wss://xxx.xxx.jp".to_string());
//! let api_token = std::env::var("EXAMPLE_TOKEN").unwrap_or_else(|_| {
//!     "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string()
//! });
//! let node_id = std::env::var("EXAMPLE_NODE_ID")
//!     .unwrap_or_else(|_| "11111111-1111-1111-1111-111111111111".to_string());
//! let src_node_id = std::env::var("EXAMPLE_SRC_NODE_ID")
//!     .unwrap_or_else(|_| "22222222-2222-2222-2222-222222222222".to_string());
//!
//! let token_source = TokenSource {
//!     // Get an access token by oauth2 from intdash API in actual usage
//!     access_token: api_token,
//! };
//!
//! let connector = iscp::transport::websocket::WebSocketConnector::new(url);
//! let conn = iscp::ConnBuilder::new(connector)
//!     .node_id(node_id)
//!     .compression(iscp::transport::Compression::new().enable(9))
//!     .token_source(token_source)
//!     .build()
//!     .await?;
//!
//! // Open a downstream
//! let mut config = iscp::DownstreamConfig::default();
//! config.filters = vec![iscp::DownstreamFilter {
//!     source_node_id: src_node_id.into(),
//!     data_filters: vec![iscp::DataFilter {
//!         name: "greeting".into(),
//!         type_: "string".into(),
//!     }],
//! }];
//! let (mut down, mut metadata_reader) = conn.open_downstream_with_config(config).await?;
//!
//! // Read received chunks and metadata
//! for _ in 0..4 {
//!     tokio::select! {
//!         Ok(chunk) = down.read_chunk() => {
//!             println!("received {}: {:?}", chunk.upstream.stream_id, chunk);
//!         }
//!         Ok(metadata) = metadata_reader.read() => {
//!             println!("received metadata {:?}", metadata);
//!         }
//!         else => break,
//!     }
//! }
//!
//! conn.close().await?;
//! ```

#![cfg_attr(docsrs, feature(doc_cfg))]

#[macro_use]
mod internal;

pub mod encoding;
mod error;
mod iscp;
pub mod message;
mod token_source;
pub mod transport;
pub mod wire;

pub use crate::error::*;
pub use crate::iscp::*;
pub use crate::token_source::*;

/// iSCPプロトコルバージョン
pub const ISCP_VERSION: &str = "2.0.0";
