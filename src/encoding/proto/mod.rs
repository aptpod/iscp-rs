//! Protocol Buffers を使用したエンコーダーを提供するモジュールです。

use super::internal::autogen;
use crate::error::{Error, Result};
use crate::message as msg;
use prost::Message as _;

mod connect;
mod data;
mod downstream;
mod e2e;
mod filter;
mod metadata;
mod ping_pong;
mod upstream;
pub use connect::*;
pub use data::*;
pub use downstream::*;
pub use e2e::*;
pub use filter::*;
pub use metadata::*;
pub use ping_pong::*;
pub use upstream::*;

/// Protocol Buffers 用エンコーダーです。
#[derive(Clone, Copy, Debug)]
pub struct Encoder;

impl Encoder {
    #[doc(hidden)]
    pub fn encode(&self, msg: msg::Message) -> Result<Vec<u8>> {
        let msg = autogen::Message::from(msg);
        Ok(msg.encode_to_vec())
    }

    #[doc(hidden)]
    pub fn decode(&self, bin: &[u8]) -> Result<msg::Message> {
        let msg = autogen::Message::decode(bin).map_err(Error::malformed_message)?;
        msg.try_into()
    }
}

impl super::Encoder for Encoder {
    fn encode(&self, msg: msg::Message) -> Result<Vec<u8>> {
        self.encode(msg)
    }

    fn decode(&self, bin: &[u8]) -> Result<crate::message::Message> {
        self.decode(bin)
    }
}

impl From<msg::Message> for autogen::Message {
    fn from(msg: msg::Message) -> Self {
        use autogen::message::Message as InnerMsg;

        let inner_msg = match msg {
            // connect
            msg::Message::ConnectRequest(req) => InnerMsg::ConnectRequest(req.into()),
            msg::Message::ConnectResponse(resp) => InnerMsg::ConnectResponse(resp.into()),
            msg::Message::Disconnect(c) => InnerMsg::Disconnect(c.into()),

            // upstream
            msg::Message::UpstreamOpenRequest(r) => InnerMsg::UpstreamOpenRequest(r.into()),
            msg::Message::UpstreamResumeRequest(r) => InnerMsg::UpstreamResumeRequest(r.into()),
            msg::Message::UpstreamCloseRequest(r) => InnerMsg::UpstreamCloseRequest(r.into()),
            msg::Message::UpstreamOpenResponse(r) => InnerMsg::UpstreamOpenResponse(r.into()),
            msg::Message::UpstreamResumeResponse(r) => InnerMsg::UpstreamResumeResponse(r.into()),
            msg::Message::UpstreamCloseResponse(r) => InnerMsg::UpstreamCloseResponse(r.into()),

            // downstream
            msg::Message::DownstreamOpenRequest(r) => InnerMsg::DownstreamOpenRequest(r.into()),
            msg::Message::DownstreamResumeRequest(r) => InnerMsg::DownstreamResumeRequest(r.into()),
            msg::Message::DownstreamCloseRequest(r) => InnerMsg::DownstreamCloseRequest(r.into()),
            msg::Message::DownstreamOpenResponse(r) => InnerMsg::DownstreamOpenResponse(r.into()),
            msg::Message::DownstreamResumeResponse(r) => {
                InnerMsg::DownstreamResumeResponse(r.into())
            }
            msg::Message::DownstreamCloseResponse(r) => InnerMsg::DownstreamCloseResponse(r.into()),

            // stream
            msg::Message::UpstreamChunk(ps) => InnerMsg::UpstreamChunk(ps.into()),
            msg::Message::UpstreamChunkAck(ack) => InnerMsg::UpstreamChunkAck(ack.into()),
            msg::Message::DownstreamChunk(ps) => InnerMsg::DownstreamChunk(ps.into()),
            msg::Message::DownstreamChunkAck(ack) => InnerMsg::DownstreamChunkAck(ack.into()),
            msg::Message::DownstreamChunkAckComplete(ack) => {
                InnerMsg::DownstreamChunkAckComplete(ack.into())
            }

            // ping pong
            msg::Message::Ping(p) => InnerMsg::Ping(p.into()),
            msg::Message::Pong(p) => InnerMsg::Pong(p.into()),

            // e2e
            msg::Message::UpstreamCall(c) => InnerMsg::UpstreamCall(c.into()),
            msg::Message::UpstreamCallAck(c) => InnerMsg::UpstreamCallAck(c.into()),
            msg::Message::DownstreamCall(c) => InnerMsg::DownstreamCall(c.into()),

            // metadata
            msg::Message::UpstreamMetadata(m) => InnerMsg::UpstreamMetadata(m.into()),
            msg::Message::UpstreamMetadataAck(m) => InnerMsg::UpstreamMetadataAck(m.into()),
            msg::Message::DownstreamMetadata(m) => InnerMsg::DownstreamMetadata(m.into()),
            msg::Message::DownstreamMetadataAck(m) => InnerMsg::DownstreamMetadataAck(m.into()),
        };
        Self {
            message: Some(inner_msg),
        }
    }
}

impl TryFrom<autogen::Message> for msg::Message {
    type Error = Error;
    fn try_from(msg: autogen::Message) -> Result<Self, Self::Error> {
        use autogen::message::Message as InnerMsg;

        let inner_msg = if let Some(inner_msg) = msg.message {
            inner_msg
        } else {
            return Err(Error::unexpected("empty message"));
        };

        let msg = match inner_msg {
            // connect
            InnerMsg::ConnectRequest(r) => msg::Message::ConnectRequest(r.into()),
            InnerMsg::ConnectResponse(r) => msg::Message::ConnectResponse(r.into()),
            InnerMsg::Disconnect(r) => msg::Message::Disconnect(r.into()),

            // upstream
            InnerMsg::UpstreamOpenRequest(r) => msg::Message::UpstreamOpenRequest(r.into()),
            InnerMsg::UpstreamOpenResponse(r) => msg::Message::UpstreamOpenResponse(r.into()),
            InnerMsg::UpstreamResumeRequest(r) => msg::Message::UpstreamResumeRequest(r.into()),
            InnerMsg::UpstreamResumeResponse(r) => msg::Message::UpstreamResumeResponse(r.into()),
            InnerMsg::UpstreamCloseRequest(r) => msg::Message::UpstreamCloseRequest(r.into()),
            InnerMsg::UpstreamCloseResponse(r) => msg::Message::UpstreamCloseResponse(r.into()),

            // downstream
            InnerMsg::DownstreamOpenRequest(r) => msg::Message::DownstreamOpenRequest(r.into()),
            InnerMsg::DownstreamOpenResponse(r) => msg::Message::DownstreamOpenResponse(r.into()),
            InnerMsg::DownstreamResumeRequest(r) => msg::Message::DownstreamResumeRequest(r.into()),
            InnerMsg::DownstreamResumeResponse(r) => {
                msg::Message::DownstreamResumeResponse(r.into())
            }
            InnerMsg::DownstreamCloseRequest(r) => msg::Message::DownstreamCloseRequest(r.into()),
            InnerMsg::DownstreamCloseResponse(r) => msg::Message::DownstreamCloseResponse(r.into()),

            // stream
            InnerMsg::UpstreamChunk(r) => msg::Message::UpstreamChunk(r.try_into()?),
            InnerMsg::UpstreamChunkAck(r) => msg::Message::UpstreamChunkAck(r.into()),

            InnerMsg::DownstreamChunk(r) => msg::Message::DownstreamChunk(r.try_into()?),
            InnerMsg::DownstreamChunkAck(r) => msg::Message::DownstreamChunkAck(r.into()),
            InnerMsg::DownstreamChunkAckComplete(r) => {
                msg::Message::DownstreamChunkAckComplete(r.into())
            }

            // ping pong
            InnerMsg::Ping(r) => msg::Message::Ping(r.into()),
            InnerMsg::Pong(r) => msg::Message::Pong(r.into()),

            // e2e
            InnerMsg::UpstreamCall(r) => msg::Message::UpstreamCall(r.into()),
            InnerMsg::UpstreamCallAck(r) => msg::Message::UpstreamCallAck(r.into()),
            InnerMsg::DownstreamCall(r) => msg::Message::DownstreamCall(r.into()),

            // metadata
            InnerMsg::UpstreamMetadata(r) => msg::Message::UpstreamMetadata(r.try_into()?),
            InnerMsg::UpstreamMetadataAck(r) => msg::Message::UpstreamMetadataAck(r.into()),
            InnerMsg::DownstreamMetadata(r) => msg::Message::DownstreamMetadata(r.try_into()?),
            InnerMsg::DownstreamMetadataAck(r) => msg::Message::DownstreamMetadataAck(r.into()),
        };
        Ok(msg)
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use std::{collections::HashMap, str::FromStr};
    use uuid::Uuid;

    #[allow(clippy::vec_init_then_push, clippy::redundant_clone)]
    #[test]
    fn test_message_from_to_proto() {
        use chrono::TimeZone;

        let bt = msg::BaseTime {
            session_id: "session_id".to_string(),
            name: String::from("name"),
            priority: 1,
            elapsed_time: chrono::Duration::seconds(2),
            base_time: chrono::Utc.with_ymd_and_hms(2000, 1, 2, 3, 4, 5).unwrap(),
        };

        let data_id = msg::DataId::parse_str("test_id_type:test_id").unwrap();
        let id_alias: HashMap<u32, msg::DataId> = [(1, data_id.clone())].iter().cloned().collect();
        let autogen_id_alias: HashMap<u32, autogen::DataId> =
            [(1, data_id.clone())].iter().cloned().collect();

        let data_filter = msg::DataFilter {
            name: "test".to_string(),
            type_: "test_field_type".to_string(),
        };
        let down_filter = msg::DownstreamFilter {
            source_node_id: "8aa70edc-3daf-4ae5-ace9-20e3c09426d3".to_string(),
            data_filters: vec![data_filter],
        };

        let upstream_aliases = [(
            1_u32,
            msg::UpstreamInfo {
                source_node_id: "source_node_id".to_string(),
                session_id: "session_id".to_string(),
                stream_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            },
        )]
        .iter()
        .cloned()
        .collect::<HashMap<_, _>>();

        let point = msg::DataPoint {
            elapsed_time: chrono::Duration::seconds(100).num_nanoseconds().unwrap(),
            payload: vec![1, 2, 3, 4].into(),
        };

        let data_points = vec![point];
        let data_point_group = msg::DataPointGroup {
            data_points,
            data_id_or_alias: msg::DataIdOrAlias::DataIdAlias(9),
        };

        let stream_chunk = msg::StreamChunk {
            sequence_number: 0,
            data_point_groups: vec![data_point_group.clone()],
        };

        let upstream_data_points_result = msg::UpstreamChunkResult {
            sequence_number: 1,
            result_code: msg::ResultCode::Succeeded,
            result_string: "ok".to_string(),
        };

        let downstream_data_points_result = msg::DownstreamChunkResult {
            sequence_number_in_upstream: 1,
            stream_id_of_upstream: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            result_code: msg::ResultCode::Succeeded,
            result_string: "ok".to_string(),
        };

        struct Case {
            msg: msg::Message,
            proto: autogen::Message,
        }
        impl Case {
            pub fn test(&self) {
                let p2m = msg::Message::try_from(self.proto.clone()).unwrap();
                assert_eq!(p2m, self.msg.clone());
                let m2p = autogen::Message::try_from(self.msg.clone()).unwrap();
                assert_eq!(m2p, self.proto.clone());

                let encoded = Encoder.encode(self.msg.clone()).unwrap();
                let decoded = Encoder.decode(&encoded).unwrap();
                assert_eq!(decoded, self.msg);
            }
        }

        let mut cases = Vec::new();
        // connect
        cases.push(Case {
            msg: msg::ConnectRequest {
                request_id: 1.into(),
                protocol_version: "v1.0.0".to_string(),
                node_id: "2dda7df9-cfd8-4aba-a577-f05dec10b1d9".to_string(),
                ping_interval: chrono::Duration::seconds(10),
                ping_timeout: chrono::Duration::seconds(1),
                access_token: Some(msg::AccessToken::new("access_token")),
                project_uuid: None,
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::ConnectRequest(
                    autogen::ConnectRequest {
                        request_id: 1,
                        protocol_version: "v1.0.0".to_string(),
                        node_id: "2dda7df9-cfd8-4aba-a577-f05dec10b1d9".to_string(),
                        ping_interval: 10u32,
                        ping_timeout: 1u32,
                        extension_fields: Some(autogen::ConnectRequestExtensionFields {
                            access_token: "access_token".into(),
                            intdash: None,
                        }),
                    },
                )),
            },
        });
        cases.push(Case {
            msg: msg::ConnectResponse {
                request_id: 1.into(),
                protocol_version: "v1.0.0".to_string(),
                result_code: msg::ResultCode::UnspecifiedError,
                result_string: "error".to_string(),
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::ConnectResponse(
                    autogen::ConnectResponse {
                        request_id: 1,
                        protocol_version: "v1.0.0".to_string(),
                        result_code: autogen::ResultCode::UnspecifiedError.into(),
                        result_string: "error".to_string(),
                        ..Default::default()
                    },
                )),
            },
        });
        cases.push(Case {
            msg: msg::Disconnect {
                result_code: msg::ResultCode::UnspecifiedError,
                result_string: "error".to_string(),
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::Disconnect(autogen::Disconnect {
                    result_code: autogen::ResultCode::UnspecifiedError.into(),
                    result_string: "error".to_string(),
                    ..Default::default()
                })),
            },
        });
        // upstream
        cases.push(Case {
            msg: msg::UpstreamOpenRequest {
                request_id: 1.into(),
                session_id: "session".to_string(),
                ack_interval: chrono::Duration::seconds(1),
                expiry_interval: chrono::Duration::seconds(1),
                data_ids: vec![data_id.clone()],
                qos: msg::QoS::Reliable,
                persist: Some(true),
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::UpstreamOpenRequest(
                    autogen::UpstreamOpenRequest {
                        request_id: 1,
                        session_id: "session".to_string(),
                        ack_interval: chrono::Duration::seconds(1).num_milliseconds() as u32,
                        expiry_interval: chrono::Duration::seconds(1).num_seconds() as u32,
                        data_ids: vec![data_id],
                        qos: autogen::QoS::Reliable.into(),
                        extension_fields: Some(autogen::UpstreamOpenRequestExtensionFields {
                            persist: true,
                        }),
                    },
                )),
            },
        });
        cases.push(Case {
            msg: msg::UpstreamOpenResponse {
                request_id: 1.into(),
                result_code: msg::ResultCode::Succeeded,
                result_string: "ok".to_string(),
                assigned_stream_id: uuid::Uuid::from_str("b6ac8145-0dbb-4660-a99a-d891f6b74db5")
                    .unwrap(),
                assigned_stream_id_alias: 2,
                data_id_aliases: id_alias.clone(),
                server_time: chrono::Utc.timestamp_opt(1, 420_000_000).unwrap(),
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::UpstreamOpenResponse(
                    autogen::UpstreamOpenResponse {
                        request_id: 1,
                        result_code: autogen::ResultCode::Succeeded.into(),
                        result_string: "ok".to_string(),
                        assigned_stream_id: uuid::Uuid::parse_str(
                            "b6ac8145-0dbb-4660-a99a-d891f6b74db5",
                        )
                        .unwrap()
                        .as_bytes()
                        .to_vec()
                        .into(),
                        assigned_stream_id_alias: 2,
                        data_id_aliases: autogen_id_alias.clone(),
                        server_time: 1_420_000_000,
                        ..Default::default()
                    },
                )),
            },
        });
        cases.push(Case {
            msg: msg::UpstreamResumeRequest {
                request_id: 1.into(),
                stream_id: uuid::Uuid::from_str("b6ac8145-0dbb-4660-a99a-d891f6b74db5").unwrap(),
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::UpstreamResumeRequest(
                    autogen::UpstreamResumeRequest {
                        request_id: 1,
                        stream_id: uuid::Uuid::from_str("b6ac8145-0dbb-4660-a99a-d891f6b74db5")
                            .unwrap()
                            .as_bytes()
                            .to_vec()
                            .into(),
                        extension_fields: None,
                    },
                )),
            },
        });
        cases.push(Case {
            msg: msg::UpstreamResumeResponse {
                request_id: 1.into(),
                assigned_stream_id_alias: 2,
                result_code: msg::ResultCode::Succeeded,
                result_string: "ok".to_string(),
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::UpstreamResumeResponse(
                    autogen::UpstreamResumeResponse {
                        request_id: 1,
                        assigned_stream_id_alias: 2,
                        result_code: autogen::ResultCode::Succeeded.into(),
                        result_string: "ok".to_string(),
                        extension_fields: None,
                    },
                )),
            },
        });
        cases.push(Case {
            msg: msg::UpstreamCloseRequest {
                request_id: 1.into(),
                stream_id: uuid::Uuid::from_str("b6ac8145-0dbb-4660-a99a-d891f6b74db5").unwrap(),
                total_data_points: 2,
                final_sequence_number: 3,
                extension_fields: Some(msg::UpstreamCloseRequestExtensionFields {
                    close_session: true,
                }),
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::UpstreamCloseRequest(
                    autogen::UpstreamCloseRequest {
                        request_id: 1,
                        stream_id: uuid::Uuid::from_str("b6ac8145-0dbb-4660-a99a-d891f6b74db5")
                            .unwrap()
                            .as_bytes()
                            .to_vec()
                            .into(),
                        total_data_points: 2,
                        final_sequence_number: 3,
                        extension_fields: Some(autogen::UpstreamCloseRequestExtensionFields {
                            close_session: true,
                        }),
                    },
                )),
            },
        });
        cases.push(Case {
            msg: msg::UpstreamCloseResponse {
                request_id: 1.into(),
                result_code: msg::ResultCode::UnspecifiedError,
                result_string: "error".to_string(),
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::UpstreamCloseResponse(
                    autogen::UpstreamCloseResponse {
                        request_id: 1,
                        result_code: autogen::ResultCode::UnspecifiedError.into(),
                        result_string: "error".to_string(),
                        extension_fields: None,
                    },
                )),
            },
        });
        // downstream
        cases.push(Case {
            msg: msg::DownstreamOpenRequest {
                request_id: 1.into(),
                desired_stream_id_alias: 2,
                downstream_filters: vec![down_filter.clone()],
                expiry_interval: chrono::Duration::seconds(1),
                data_id_aliases: id_alias.clone(),
                qos: msg::QoS::Reliable,
                omit_empty_chunk: false,
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::DownstreamOpenRequest(
                    autogen::DownstreamOpenRequest {
                        request_id: 1,
                        desired_stream_id_alias: 2,
                        expiry_interval: chrono::Duration::seconds(1).num_seconds() as u32,
                        downstream_filters: vec![down_filter.clone()],
                        data_id_aliases: id_alias.clone(),
                        qos: autogen::QoS::Reliable.into(),
                        extension_fields: None,
                        omit_empty_chunk: false,
                    },
                )),
            },
        });
        cases.push(Case {
            msg: msg::DownstreamOpenResponse {
                request_id: 1.into(),
                server_time: chrono::Utc.timestamp_opt(123_456_789, 123_456_789).unwrap(),
                result_code: msg::ResultCode::Succeeded,
                result_string: "ok".to_string(),
                assigned_stream_id: uuid::Uuid::from_str("b6ac8145-0dbb-4660-a99a-d891f6b74db5")
                    .unwrap(),
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::DownstreamOpenResponse(
                    autogen::DownstreamOpenResponse {
                        request_id: 1,
                        server_time: chrono::Utc
                            .timestamp_opt(123_456_789, 123_456_789)
                            .unwrap()
                            .timestamp_nanos_opt()
                            .unwrap_or_else(|| {
                                log::warn!("server time overflow");
                                Default::default()
                            }),
                        result_code: autogen::ResultCode::Succeeded.into(),
                        result_string: "ok".to_string(),
                        assigned_stream_id: uuid::Uuid::parse_str(
                            "b6ac8145-0dbb-4660-a99a-d891f6b74db5",
                        )
                        .unwrap()
                        .as_bytes()
                        .to_vec()
                        .into(),
                        ..Default::default()
                    },
                )),
            },
        });
        cases.push(Case {
            msg: msg::DownstreamResumeRequest {
                request_id: 1.into(),
                desired_stream_id_alias: 2,
                stream_id: uuid::Uuid::from_str("b6ac8145-0dbb-4660-a99a-d891f6b74db5").unwrap(),
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::DownstreamResumeRequest(
                    autogen::DownstreamResumeRequest {
                        request_id: 1,
                        desired_stream_id_alias: 2,
                        stream_id: uuid::Uuid::from_str("b6ac8145-0dbb-4660-a99a-d891f6b74db5")
                            .unwrap()
                            .as_bytes()
                            .to_vec()
                            .into(),
                        ..Default::default()
                    },
                )),
            },
        });
        cases.push(Case {
            msg: msg::DownstreamResumeResponse {
                request_id: 1.into(),
                result_code: msg::ResultCode::Succeeded,
                result_string: "ok".to_string(),
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::DownstreamResumeResponse(
                    autogen::DownstreamResumeResponse {
                        request_id: 1,
                        result_code: autogen::ResultCode::Succeeded.into(),
                        result_string: "ok".to_string(),
                        extension_fields: None,
                    },
                )),
            },
        });
        cases.push(Case {
            msg: msg::DownstreamCloseRequest {
                request_id: 1.into(),
                stream_id: uuid::Uuid::from_str("b6ac8145-0dbb-4660-a99a-d891f6b74db5").unwrap(),
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::DownstreamCloseRequest(
                    autogen::DownstreamCloseRequest {
                        request_id: 1,
                        stream_id: uuid::Uuid::from_str("b6ac8145-0dbb-4660-a99a-d891f6b74db5")
                            .unwrap()
                            .as_bytes()
                            .to_vec()
                            .into(),
                        extension_fields: None,
                    },
                )),
            },
        });
        cases.push(Case {
            msg: msg::DownstreamCloseResponse {
                request_id: 1.into(),
                result_code: msg::ResultCode::UnspecifiedError,
                result_string: "error".to_string(),
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::DownstreamCloseResponse(
                    autogen::DownstreamCloseResponse {
                        request_id: 1,
                        result_code: autogen::ResultCode::UnspecifiedError.into(),
                        result_string: "error".to_string(),
                        extension_fields: None,
                    },
                )),
            },
        });
        // stream
        cases.push(Case {
            msg: msg::UpstreamChunk {
                stream_id_alias: 2,
                stream_chunk: stream_chunk.clone(),
                data_ids: vec![],
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::UpstreamChunk(
                    autogen::UpstreamChunk {
                        stream_id_alias: 2,
                        stream_chunk: Some(stream_chunk.clone().into()),
                        data_ids: vec![],
                        ..Default::default()
                    },
                )),
            },
        });
        cases.push(Case {
            msg: msg::UpstreamChunkAck {
                stream_id_alias: 2,
                results: vec![upstream_data_points_result.clone()],
                data_id_aliases: id_alias.clone(),
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::UpstreamChunkAck(
                    autogen::UpstreamChunkAck {
                        stream_id_alias: 2,
                        results: vec![upstream_data_points_result.into()],
                        data_id_aliases: autogen_id_alias.clone(),
                        ..Default::default()
                    },
                )),
            },
        });
        cases.push(Case {
            msg: msg::DownstreamChunk {
                stream_id_alias: 2,
                stream_chunk: stream_chunk.clone(),
                upstream_or_alias: msg::UpstreamOrAlias::Alias(2),
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::DownstreamChunk(
                    autogen::DownstreamChunk {
                        stream_id_alias: 2,
                        stream_chunk: Some(stream_chunk.clone().into()),
                        upstream_or_alias: Some(
                            autogen::downstream_chunk::UpstreamOrAlias::UpstreamAlias(2),
                        ),
                        ..Default::default()
                    },
                )),
            },
        });
        cases.push(Case {
            msg: msg::DownstreamChunkAck {
                stream_id_alias: 2,
                ack_id: 1,
                results: vec![downstream_data_points_result.clone()],
                upstream_aliases: upstream_aliases.clone(),
                data_id_aliases: id_alias,
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::DownstreamChunkAck(
                    autogen::DownstreamChunkAck {
                        ack_id: 1,
                        stream_id_alias: 2,
                        results: vec![downstream_data_points_result.into()],
                        data_id_aliases: autogen_id_alias,
                        upstream_aliases: upstream_aliases
                            .into_iter()
                            .map(|(a, u)| (a, u.into()))
                            .collect(),
                        ..Default::default()
                    },
                )),
            },
        });
        cases.push(Case {
            msg: msg::DownstreamChunkAckComplete {
                stream_id_alias: 1,
                ack_id: 2,
                result_code: msg::ResultCode::Succeeded,
                result_string: "ok".to_string(),
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::DownstreamChunkAckComplete(
                    autogen::DownstreamChunkAckComplete {
                        stream_id_alias: 1,
                        ack_id: 2,
                        result_code: autogen::ResultCode::Succeeded.into(),
                        result_string: "ok".to_string(),
                        ..Default::default()
                    },
                )),
            },
        });
        // e2e
        cases.push(Case {
            msg: msg::UpstreamCall {
                call_id: "call_id".to_string(),
                request_call_id: "request_call_id".to_string(),
                destination_node_id: "node_id".to_string(),
                name: "name".to_string(),
                type_: "type".to_string(),
                payload: vec![1, 2, 3, 4].into(),
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::UpstreamCall(
                    autogen::UpstreamCall {
                        call_id: "call_id".to_string(),
                        request_call_id: "request_call_id".to_string(),
                        destination_node_id: "node_id".to_string(),
                        name: "name".to_string(),
                        type_: "type".to_string(),
                        payload: vec![1, 2, 3, 4].into(),
                        ..Default::default()
                    },
                )),
            },
        });
        cases.push(Case {
            msg: msg::UpstreamCallAck {
                call_id: "call_id".to_string(),
                result_code: msg::ResultCode::Succeeded,
                result_string: "OK".to_string(),
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::UpstreamCallAck(
                    autogen::UpstreamCallAck {
                        call_id: "call_id".to_string(),
                        result_code: autogen::ResultCode::Succeeded.into(),
                        result_string: "OK".to_string(),
                        ..Default::default()
                    },
                )),
            },
        });
        cases.push(Case {
            msg: msg::DownstreamCall {
                call_id: "call_id".to_string(),
                request_call_id: "request_call_id".to_string(),
                source_node_id: "node_id".to_string(),
                name: "name".to_string(),
                type_: "type".to_string(),
                payload: vec![1, 2, 3, 4].into(),
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::DownstreamCall(
                    autogen::DownstreamCall {
                        call_id: "call_id".to_string(),
                        request_call_id: "request_call_id".to_string(),
                        source_node_id: "node_id".to_string(),
                        name: "name".to_string(),
                        type_: "type".to_string(),
                        payload: vec![1, 2, 3, 4].into(),
                        ..Default::default()
                    },
                )),
            },
        });
        // ping
        cases.push(Case {
            msg: msg::Ping {
                request_id: 1.into(),
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::Ping(autogen::Ping {
                    request_id: 1,
                    ..Default::default()
                })),
            },
        });
        //pong
        cases.push(Case {
            msg: msg::Pong {
                request_id: 1.into(),
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::Pong(autogen::Pong {
                    request_id: 1,
                    ..Default::default()
                })),
            },
        });
        // metadata
        cases.push(Case {
            msg: msg::UpstreamMetadata {
                request_id: 1.into(),
                metadata: msg::SendableMetadata::BaseTime(bt.clone()),
                persist: Some(true),
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::UpstreamMetadata(
                    autogen::UpstreamMetadata {
                        request_id: 1,
                        metadata: Some(autogen::upstream_metadata::Metadata::BaseTime(
                            bt.clone().into(),
                        )),
                        extension_fields: Some(autogen::UpstreamMetadataExtensionFields {
                            persist: true,
                        }),
                    },
                )),
            },
        });
        cases.push(Case {
            msg: msg::UpstreamMetadataAck {
                request_id: 1.into(),
                result_code: msg::ResultCode::Succeeded,
                result_string: "ok".to_string(),
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::UpstreamMetadataAck(
                    autogen::UpstreamMetadataAck {
                        request_id: 1,
                        result_code: autogen::ResultCode::Succeeded.into(),
                        result_string: "ok".to_string(),
                        ..Default::default()
                    },
                )),
            },
        });
        cases.push(Case {
            msg: msg::DownstreamMetadata {
                request_id: 1.into(),
                stream_id_alias: 2,
                source_node_id: "source_node_id".to_string(),
                metadata: msg::ReceivableMetadata::BaseTime(bt.clone()),
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::DownstreamMetadata(
                    autogen::DownstreamMetadata {
                        request_id: 1,
                        stream_id_alias: 2,
                        source_node_id: "source_node_id".to_string(),
                        metadata: Some(autogen::downstream_metadata::Metadata::BaseTime(bt.into())),
                        ..Default::default()
                    },
                )),
            },
        });
        cases.push(Case {
            msg: msg::DownstreamMetadataAck {
                request_id: 1.into(),
                result_code: msg::ResultCode::Succeeded,
                result_string: "ok".to_string(),
            }
            .into(),
            proto: autogen::Message {
                message: Some(autogen::message::Message::DownstreamMetadataAck(
                    autogen::DownstreamMetadataAck {
                        request_id: 1,
                        result_code: autogen::ResultCode::Succeeded.into(),
                        result_string: "ok".to_string(),
                        ..Default::default()
                    },
                )),
            },
        });
        cases.into_iter().for_each(|case| {
            case.test();
        });
    }
}
