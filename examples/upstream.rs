use std::{env, sync::Arc, time::Duration};
use tokio::runtime::Runtime;

#[derive(Clone)]
struct TokenSource {
    access_token: String,
}

#[async_trait::async_trait]
impl iscp::TokenSource for TokenSource {
    async fn token(&self) -> iscp::error::Result<String> {
        Ok(self.access_token.clone())
    }
}

fn main() {
    let host = env::var("EXAMPLE_HOST").unwrap_or_else(|_| "xxx.xxx.jp".to_string());
    let port = env::var("EXAMPLE_PORT")
        .unwrap_or_else(|_| "11443".to_string())
        .parse::<i32>()
        .unwrap();
    let api_token = env::var("EXAMPLE_TOKEN").unwrap_or_else(|_| {
        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".to_string()
    });
    let node_id = env::var("EXAMPLE_NODE_ID")
        .unwrap_or_else(|_| "11111111-1111-1111-1111-111111111111".to_string());

    let addr = format!("{}:{}", host, port);

    let token_source = Arc::new(TokenSource {
        // TODO: アクセストークンはintdash-apiからoauth2で動的に取得してください。
        access_token: api_token,
    });

    let builder = iscp::ConnBuilder::new(&addr, iscp::TransportKind::Quic)
        .quic_config(Some(iscp::tr::QuicConfig {
            host, // `host` は、サーバー証明書の検証に使用されます。
            mtu: 1000,
            ..Default::default()
        }))
        .encoding(iscp::enc::EncodingKind::Proto)
        .token_source(Some(token_source))
        .node_id(node_id);

    Runtime::new().unwrap().block_on(async {
        let conn = builder.connect().await.unwrap();

        let session_id = uuid::Uuid::new_v4().to_string(); // セッションIDを払い出します。
        let base_time = chrono::Utc::now();

        let up = conn
            .upstream_builder(&session_id)
            .flush_policy(iscp::FlushPolicy::IntervalOnly {
                interval: std::time::Duration::from_millis(5),
            })
            .ack_interval(chrono::Duration::milliseconds(1000))
            .persist(true)
            .close_timeout(Some(Duration::new(1, 0)))
            .build()
            .await
            .unwrap();

        // 基準時刻をiSCPサーバーへ送信します。
        conn.send_base_time(
            iscp::message::BaseTime {
                elapsed_time: chrono::Duration::zero(),
                name: "edge_rtc".to_string(),
                base_time,
                priority: 20,
                session_id,
            },
            iscp::SendMetadataOptions { persist: true },
        )
        .await
        .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        // データポイントをiSCPサーバーへ送信します。
        up.write_data_points(iscp::DataPointGroup {
            id: iscp::DataId::new("greeting", "string"),
            data_points: vec![iscp::DataPoint {
                payload: "hello".into(),
                elapsed_time: chrono::Utc::now() - base_time,
            }],
        })
        .await
        .unwrap();

        up.close(Some(iscp::UpstreamCloseOptions {
            close_session: true,
        }))
        .await
        .unwrap();
        conn.close().await.unwrap();
    });
}
