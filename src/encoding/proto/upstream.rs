use crate::encoding::internal::autogen::extensions::{
    UpstreamCloseRequestExtensionFields, UpstreamOpenRequestExtensionFields,
};
use crate::encoding::internal::autogen::{
    UpstreamCloseRequest, UpstreamCloseResponse, UpstreamOpenRequest, UpstreamOpenResponse,
    UpstreamResumeRequest, UpstreamResumeResponse,
};
use crate::message as msg;

impl From<msg::UpstreamOpenRequest> for UpstreamOpenRequest {
    fn from(r: msg::UpstreamOpenRequest) -> Self {
        let mut res = Self {
            request_id: r.request_id.value(),
            session_id: r.session_id,
            ack_interval: r.ack_interval.num_milliseconds() as u32,
            expiry_interval: r.expiry_interval.num_seconds() as u32,
            data_ids: r.data_ids,
            qos: r.qos.into(),
            ..Default::default()
        };

        if let Some(persist) = r.persist {
            let ext = UpstreamOpenRequestExtensionFields { persist };
            res.extension_fields = Some(ext);
        }

        res
    }
}

impl From<UpstreamOpenRequest> for msg::UpstreamOpenRequest {
    fn from(r: UpstreamOpenRequest) -> Self {
        let persist = r.extension_fields.map(|ext| ext.persist);

        Self {
            request_id: r.request_id.into(),
            session_id: r.session_id,
            ack_interval: chrono::Duration::milliseconds(r.ack_interval as i64),
            expiry_interval: chrono::Duration::seconds(r.expiry_interval as i64),
            data_ids: r.data_ids,
            qos: r.qos.into(),
            persist,
        }
    }
}

impl From<UpstreamOpenRequest> for msg::Message {
    fn from(r: UpstreamOpenRequest) -> Self {
        Self::UpstreamOpenRequest(r.into())
    }
}

impl From<msg::UpstreamOpenResponse> for UpstreamOpenResponse {
    fn from(r: msg::UpstreamOpenResponse) -> Self {
        Self {
            request_id: r.request_id.value(),
            assigned_stream_id: r.assigned_stream_id.as_bytes().to_vec().into(),
            assigned_stream_id_alias: r.assigned_stream_id_alias,
            server_time: r.server_time.timestamp_nanos_opt().unwrap_or_else(|| {
                log::warn!("server time overflow");
                Default::default()
            }),
            result_code: r.result_code.into(),
            result_string: r.result_string,
            data_id_aliases: r.data_id_aliases,
            ..Default::default()
        }
    }
}

impl From<UpstreamOpenResponse> for msg::UpstreamOpenResponse {
    fn from(r: UpstreamOpenResponse) -> Self {
        let secs = r.server_time / 1_000_000_000;
        let nsecs = (r.server_time % 1_000_000_000) as u32;
        let server_time = chrono::DateTime::from_timestamp(secs, nsecs).unwrap_or_else(|| {
            log::warn!("server_time overflow");
            chrono::DateTime::default()
        });
        Self {
            request_id: r.request_id.into(),
            assigned_stream_id: uuid::Builder::from_slice(&r.assigned_stream_id)
                .unwrap_or_else(|_| uuid::Builder::nil())
                .into_uuid(),
            assigned_stream_id_alias: r.assigned_stream_id_alias,
            server_time,
            result_code: r.result_code.into(),
            result_string: r.result_string,
            data_id_aliases: r.data_id_aliases,
        }
    }
}

impl From<UpstreamOpenResponse> for msg::Message {
    fn from(r: UpstreamOpenResponse) -> Self {
        Self::UpstreamOpenResponse(r.into())
    }
}

impl From<msg::UpstreamResumeRequest> for UpstreamResumeRequest {
    fn from(r: msg::UpstreamResumeRequest) -> Self {
        Self {
            request_id: r.request_id.value(),
            stream_id: r.stream_id.as_bytes().to_vec().into(),
            ..Default::default()
        }
    }
}

impl From<UpstreamResumeRequest> for msg::UpstreamResumeRequest {
    fn from(r: UpstreamResumeRequest) -> Self {
        Self {
            request_id: r.request_id.into(),
            stream_id: uuid::Builder::from_slice(&r.stream_id)
                .unwrap_or_else(|_| uuid::Builder::nil())
                .into_uuid(),
        }
    }
}

impl From<UpstreamResumeRequest> for msg::Message {
    fn from(r: UpstreamResumeRequest) -> Self {
        Self::UpstreamResumeRequest(r.into())
    }
}

impl From<msg::UpstreamResumeResponse> for UpstreamResumeResponse {
    fn from(r: msg::UpstreamResumeResponse) -> Self {
        Self {
            request_id: r.request_id.value(),
            assigned_stream_id_alias: r.assigned_stream_id_alias,
            result_code: r.result_code.into(),
            result_string: r.result_string,
            ..Default::default()
        }
    }
}

impl From<UpstreamResumeResponse> for msg::UpstreamResumeResponse {
    fn from(r: UpstreamResumeResponse) -> Self {
        Self {
            request_id: r.request_id.into(),
            result_code: r.result_code.into(),
            assigned_stream_id_alias: r.assigned_stream_id_alias,
            result_string: r.result_string,
        }
    }
}

impl From<UpstreamResumeResponse> for msg::Message {
    fn from(r: UpstreamResumeResponse) -> Self {
        Self::UpstreamResumeResponse(r.into())
    }
}

impl From<msg::UpstreamCloseRequest> for UpstreamCloseRequest {
    fn from(r: msg::UpstreamCloseRequest) -> Self {
        let ext = r
            .extension_fields
            .map(|ext| -> UpstreamCloseRequestExtensionFields {
                UpstreamCloseRequestExtensionFields {
                    close_session: ext.close_session,
                }
            });

        Self {
            request_id: r.request_id.value(),
            stream_id: r.stream_id.as_bytes().to_vec().into(),
            total_data_points: r.total_data_points,
            final_sequence_number: r.final_sequence_number,
            extension_fields: ext,
        }
    }
}

impl From<UpstreamCloseRequest> for msg::UpstreamCloseRequest {
    fn from(r: UpstreamCloseRequest) -> Self {
        let ext = r
            .extension_fields
            .map(|ext| -> msg::UpstreamCloseRequestExtensionFields {
                msg::UpstreamCloseRequestExtensionFields {
                    close_session: ext.close_session,
                }
            });

        Self {
            request_id: r.request_id.into(),
            stream_id: uuid::Builder::from_slice(&r.stream_id)
                .unwrap_or_else(|_| uuid::Builder::nil())
                .into_uuid(),
            total_data_points: r.total_data_points,
            final_sequence_number: r.final_sequence_number,
            extension_fields: ext,
        }
    }
}

impl From<UpstreamCloseRequest> for msg::Message {
    fn from(r: UpstreamCloseRequest) -> Self {
        Self::UpstreamCloseRequest(r.into())
    }
}

impl From<msg::UpstreamCloseResponse> for UpstreamCloseResponse {
    fn from(r: msg::UpstreamCloseResponse) -> Self {
        Self {
            request_id: r.request_id.value(),
            result_code: r.result_code.into(),
            result_string: r.result_string,
            ..Default::default()
        }
    }
}

impl From<UpstreamCloseResponse> for msg::UpstreamCloseResponse {
    fn from(r: UpstreamCloseResponse) -> Self {
        Self {
            request_id: r.request_id.into(),
            result_code: r.result_code.into(),
            result_string: r.result_string,
        }
    }
}

impl From<UpstreamCloseResponse> for msg::Message {
    fn from(r: UpstreamCloseResponse) -> Self {
        Self::UpstreamCloseResponse(r.into())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    macro_rules! invalid_uuid {
        ($msg:ident, $id:ident) => {
            let testee = $msg {
                $id: vec![0, 159, 146, 150].into(),
                ..Default::default()
            };

            let want = msg::$msg::default();
            assert_eq!(want, msg::$msg::from(testee));
        };
    }

    #[test]
    fn invalid_uuid() {
        invalid_uuid!(UpstreamOpenResponse, assigned_stream_id);
        invalid_uuid!(UpstreamResumeRequest, stream_id);
        invalid_uuid!(UpstreamCloseRequest, stream_id);
    }
}
