use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let url = std::env::var("EXAMPLE_URL")?;
    let node_id = std::env::var("EXAMPLE_NODE_ID")?;
    let token = std::env::var("EXAMPLE_TOKEN")?;

    let connector = iscp::transport::websocket::WebSocketConnector::new(url);

    let token_source = iscp::StaticTokenSource::new(token);
    let conn = iscp::ConnBuilder::new(connector)
        .node_id(node_id.clone())
        .token_source(token_source)
        .build()
        .await?;

    let filters = vec![iscp::DownstreamFilter {
        source_node_id: node_id,
        data_filters: vec![iscp::DataFilter {
            name: "test".into(),
            type_: "string".into(),
        }],
    }];

    let down = conn.downstream_builder(filters).build().await?;

    tokio::spawn(async move {
        let (mut down, mut metadata_reader) = down;
        loop {
            tokio::select! {
                Ok(chunk) = down.read_chunk() => {
                    println!("received: {:?}", chunk);
                }
                Ok(metadata) = metadata_reader.read() => {
                    println!("received: {:?}", metadata);
                }
                else => break,
            }
        }
    });

    let callback: iscp::ReceiveAckCallback = Arc::new(|_, result| {
        println!("received upstream ack: {:?}", result);
        iscp::CallbackReturnValue::complete()
    });

    let session_id = uuid::Uuid::new_v4();
    let mut up = conn
        .upstream_builder(session_id.to_string())
        .flush_policy(iscp::FlushPolicy::Immediately)
        .receive_ack_callback(callback)
        .build()
        .await?;

    let base_time = SystemTime::now();
    conn.metadata_sender(iscp::metadata::BaseTime {
        base_time,
        elapsed_time: Duration::from_secs(0),
        name: "edge_rtc".into(),
        priority: 0,
        session_id: session_id.to_string(),
    })
    .send()
    .await?;

    for i in 0..10 {
        let elapsed_time = SystemTime::now().duration_since(base_time).unwrap();
        if up
            .write_data_points(
                iscp::DataId::new("test", "string"),
                vec![iscp::DataPoint {
                    payload: format!("seq = {}", i).into_bytes().into(),
                    elapsed_time: elapsed_time.as_nanos().try_into().unwrap(),
                }],
            )
            .await
            .is_err()
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    up.close(iscp::UpstreamCloseOptions {
        close_session: true,
    })
    .await?;
    conn.close().await?;

    Ok(())
}
