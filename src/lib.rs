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
//! use std::env;
//! use std::sync::Arc;
//!
//! #[derive(Clone)]
//! struct TokenSource {
//!     access_token: String,
//! }
//!
//! #[async_trait::async_trait]
//! impl iscp::TokenSource for TokenSource {
//!     async fn token(&self) -> iscp::Result<String> {
//!         Ok(self.access_token.clone())
//!     }
//! }
//!
//! let host = env::var("EXAMPLE_HOST").unwrap_or_else(|_| "xxx.xxx.jp".to_string());
//! let port = env::var("EXAMPLE_PORT")
//!     .unwrap_or_else(|_| "11443".to_string())
//!     .parse::<i32>()
//!     .unwrap();
//! let api_token = env::var("EXAMPLE_TOKEN").unwrap_or_else(|_| {
//!     "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string()
//! });
//! let node_id = env::var("EXAMPLE_NODE_ID")
//!     .unwrap_or_else(|_| "11111111-1111-1111-1111-111111111111".to_string());
//!
//! let addr = format!("{}:{}", host, port);
//!
//! let token_source = Arc::new(TokenSource {
//!     // TODO: アクセストークンはintdash-apiからoauth2で動的に取得してください。
//!     access_token: api_token,
//! });
//!
//! let builder = iscp::ConnBuilder::new(&addr, iscp::TransportKind::Quic)
//!     .quic_config(Some(iscp::tr::QuicConfig {
//!         host, // `host` は、サーバー証明書の検証に使用されます。
//!         mtu: 1000,
//!         ..Default::default()
//!     }))
//!     .encoding(iscp::enc::EncodingKind::Proto)
//!     .token_source(Some(token_source))
//!     .node_id(node_id);
//!
//! tokio::runtime::Runtime::new().unwrap().block_on(async {
//!     let conn = builder.connect().await.unwrap();
//!     conn.close().await.unwrap();
//! });
//! ```
//!
//! ## Start Upstream
//!
//! アップストリームの送信サンプルです。このサンプルでは、コネクションからアップストリームを開き、基準時刻のメタデータと、文字列型のデータポイントをiSCPサーバーへ送信しています。
//!
//! ```
//! use std::env;
//! use std::sync::Arc;
//!
//! #[derive(Clone)]
//! struct TokenSource {
//!     access_token: String,
//! }
//!
//! #[async_trait::async_trait]
//! impl iscp::TokenSource for TokenSource {
//!     async fn token(&self) -> iscp::Result<String> {
//!         Ok(self.access_token.clone())
//!     }
//! }
//!
//! let host = env::var("EXAMPLE_HOST").unwrap_or_else(|_| "xxx.xxx.jp".to_string());
//! let port = env::var("EXAMPLE_PORT")
//!     .unwrap_or_else(|_| "11443".to_string())
//!     .parse::<i32>()
//!     .unwrap();
//! let api_token = env::var("EXAMPLE_TOKEN").unwrap_or_else(|_| {
//!     "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string()
//! });
//! let node_id = env::var("EXAMPLE_NODE_ID")
//!     .unwrap_or_else(|_| "11111111-1111-1111-1111-111111111111".to_string());
//!
//! let addr = format!("{}:{}", host, port);
//!
//! let token_source = Arc::new(TokenSource {
//!     // TODO: アクセストークンはintdash-apiからoauth2で動的に取得してください。
//!     access_token: api_token,
//! });
//!
//! let builder = iscp::ConnBuilder::new(&addr, iscp::TransportKind::Quic)
//!     .quic_config(Some(iscp::tr::QuicConfig {
//!         host, // `host` は、サーバー証明書の検証に使用されます。
//!         mtu: 1000,
//!         ..Default::default()
//!     }))
//!     .encoding(iscp::enc::EncodingKind::Proto)
//!     .token_source(Some(token_source))
//!     .node_id(node_id);
//!
//! tokio::runtime::Runtime::new().unwrap().block_on(async {
//!     let conn = builder.connect().await.unwrap();
//!
//!     let session_id = uuid::Uuid::new_v4().to_string(); // セッションIDを払い出します。
//!     let base_time = chrono::Utc::now();
//!
//!     let up = conn
//!         .upstream_builder(&session_id)
//!         .flush_policy(iscp::FlushPolicy::IntervalOnly {
//!             interval: std::time::Duration::from_millis(5),
//!         })
//!         .ack_interval(chrono::Duration::milliseconds(1000))
//!         .persist(true)
//!         .build()
//!         .await
//!         .unwrap();
//!
//!     // 基準時刻をiSCPサーバーへ送信します。
//!     conn.send_base_time(
//!         iscp::msg::BaseTime {
//!             elapsed_time: chrono::Duration::zero(),
//!             name: "edge_rtc".to_string(),
//!             base_time,
//!             priority: 20,
//!             session_id,
//!         },
//!         iscp::SendMetadataOptions {
//!             persist: true,
//!         },
//!     )
//!     .await
//!     .unwrap();
//!
//!     tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
//!
//!     // データポイントをiSCPサーバーへ送信します。
//!     up.write_data_points(iscp::DataPointGroup {
//!         id: iscp::DataId::new("greeting", "string"),
//!         data_points: vec![iscp::DataPoint {
//!             payload: "hello".into(),
//!             elapsed_time: chrono::Utc::now() - base_time,
//!         }],
//!     })
//!     .await
//!     .unwrap();
//!
//!     up.close(Some(iscp::UpstreamCloseOptions {
//!         close_session: true,
//!     }))
//!     .await
//!     .unwrap();
//!     conn.close().await.unwrap();
//! });
//! ```
//!
//! ## Start Downstream
//!
//! 前述のアップストリームで送信されたデータをダウンストリームで受信するサンプルです。
//! このサンプルでは、アップストリーム開始のメタデータ、基準時刻のメタデータ、 文字列型のデータポイントを受信しています。
//!
//! ```
//! use std::env;
//! use std::sync::Arc;
//!
//! #[derive(Clone)]
//! struct TokenSource {
//!     access_token: String,
//! }
//!
//! #[async_trait::async_trait]
//! impl iscp::TokenSource for TokenSource {
//!     async fn token(&self) -> iscp::Result<String> {
//!         Ok(self.access_token.clone())
//!     }
//! }
//!
//! let host = env::var("EXAMPLE_HOST").unwrap_or_else(|_| "xxx.xxx.jp".to_string());
//! let port = env::var("EXAMPLE_PORT")
//!     .unwrap_or_else(|_| "11443".to_string())
//!     .parse::<i32>()
//!     .unwrap();
//! let api_token = env::var("EXAMPLE_TOKEN").unwrap_or_else(|_| {
//!     "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string()
//! });
//! let node_id = env::var("EXAMPLE_NODE_ID")
//!     .unwrap_or_else(|_| "11111111-1111-1111-1111-111111111111".to_string());
//! let source_node_id = env::var("EXAMPLE_SRC_NODE_ID")
//!     .unwrap_or_else(|_| "22222222-2222-2222-2222-222222222222".to_string());
//!
//! let addr = format!("{}:{}", host, port);
//!
//! let token_source = Arc::new(TokenSource {
//!     // TODO: アクセストークンはintdash-apiからoauth2で動的に取得してください。
//!     access_token: api_token,
//! });
//!
//! let builder = iscp::ConnBuilder::new(&addr, iscp::TransportKind::Quic)
//!     .quic_config(Some(iscp::tr::QuicConfig {
//!         host, // `host` は、サーバー証明書の検証に使用されます。
//!         mtu: 1000,
//!         ..Default::default()
//!     }))
//!     .encoding(iscp::enc::EncodingKind::Proto)
//!     .token_source(Some(token_source))
//!     .node_id(node_id);
//!
//! tokio::runtime::Runtime::new().unwrap().block_on(async {
//!     let conn = builder.connect().await.unwrap();
//!
//!     let mut disconnect_notified = conn.subscribe_disconnect();
//!
//!     let down = conn
//!         .downstream_builder(
//!             vec![iscp::DownstreamFilter {
//!                 source_node_id, // 送信元のノードIDを指定します。
//!                 data_filters: vec![iscp::DataId::new("#", "#").into()], // 受信したいデータを名称と型で指定します。この例では、ワイルドカード `#` を使用して全てのデータを取得します。
//!             }],
//!         )
//!         .build()
//!         .await
//!         .unwrap();
//!
//!     // ダウンストリーム開始のメタデータの受信
//!     let metadata = down.read_metadata().await;
//!     println!("{:?}", metadata.unwrap());
//!
//!     // アップストリーム開始のメタデータの受信
//!     let metadata = down.read_metadata().await;
//!     println!("{:?}", metadata.unwrap());
//!
//!     // 基準時刻のメタデータの受信
//!     let metadata = down.read_metadata().await;
//!     println!("{:?}", metadata.unwrap());
//!
//!     // 文字列型のデータポイントの受信
//!     let down_chunk = down.read_data_points().await.unwrap();
//!     println!("{:?}", down_chunk);
//!
//!     down.close().await.unwrap();
//!     tokio::spawn(async move { conn.close().await.unwrap() });
//!
//!     disconnect_notified.recv().await;
//! });
//! ```

#[macro_use]
mod internal;

pub mod encoding;
pub mod error;
pub mod iscp;
pub mod message;
pub mod token_source;
pub mod transport;
pub mod wire;

pub use crate::error::*;
pub use crate::iscp::*;
pub use crate::token_source::*;

pub use crate::encoding as enc;
pub use crate::message as msg;
pub use crate::transport as tr;

mod sync;
pub(crate) use crate::sync::*;

/// プロトコルのバージョンです。
pub const ISCP_VERSION: &str = "2.0.0";
