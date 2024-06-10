use crate::encoding::internal::autogen::{
    DownstreamCloseRequest, DownstreamCloseResponse, DownstreamOpenRequest, DownstreamOpenResponse,
    DownstreamResumeRequest, DownstreamResumeResponse,
};
use crate::message as msg;

impl From<msg::DownstreamOpenRequest> for DownstreamOpenRequest {
    fn from(req: msg::DownstreamOpenRequest) -> Self {
        Self {
            request_id: req.request_id.value(),
            desired_stream_id_alias: req.desired_stream_id_alias,
            downstream_filters: req.downstream_filters,
            expiry_interval: req.expiry_interval.num_seconds() as u32,
            data_id_aliases: req.data_id_aliases,
            qos: req.qos.into(),
            omit_empty_chunk: req.omit_empty_chunk,
            ..Default::default()
        }
    }
}

impl From<DownstreamOpenRequest> for msg::DownstreamOpenRequest {
    fn from(req: DownstreamOpenRequest) -> Self {
        Self {
            request_id: req.request_id.into(),
            desired_stream_id_alias: req.desired_stream_id_alias,
            downstream_filters: req.downstream_filters,
            expiry_interval: chrono::Duration::seconds(req.expiry_interval as i64),
            data_id_aliases: req.data_id_aliases,
            qos: req.qos.into(),
            omit_empty_chunk: req.omit_empty_chunk,
        }
    }
}

impl From<DownstreamOpenRequest> for msg::Message {
    fn from(r: DownstreamOpenRequest) -> Self {
        Self::DownstreamOpenRequest(r.into())
    }
}

impl From<msg::DownstreamOpenResponse> for DownstreamOpenResponse {
    fn from(r: msg::DownstreamOpenResponse) -> Self {
        Self {
            request_id: r.request_id.value(),
            assigned_stream_id: r.assigned_stream_id.as_bytes().to_vec().into(),
            server_time: r.server_time.timestamp_nanos_opt().unwrap_or_else(|| {
                log::warn!("server time overflow");
                Default::default()
            }),
            result_code: r.result_code.into(),
            result_string: r.result_string,
            ..Default::default()
        }
    }
}

impl From<DownstreamOpenResponse> for msg::DownstreamOpenResponse {
    fn from(r: DownstreamOpenResponse) -> Self {
        Self {
            request_id: r.request_id.into(),
            assigned_stream_id: uuid::Builder::from_slice(&r.assigned_stream_id)
                .unwrap_or_else(|_| uuid::Builder::nil())
                .into_uuid(),
            server_time: chrono::DateTime::from_timestamp(
                r.server_time / 1000000000,
                (r.server_time % 1000000000) as u32,
            )
            .unwrap_or_else(|| {
                log::warn!("server_time overflow");
                Default::default()
            }),
            result_code: r.result_code.into(),
            result_string: r.result_string,
        }
    }
}

impl From<DownstreamOpenResponse> for msg::Message {
    fn from(r: DownstreamOpenResponse) -> Self {
        Self::DownstreamOpenResponse(r.into())
    }
}

impl From<msg::DownstreamResumeRequest> for DownstreamResumeRequest {
    fn from(r: msg::DownstreamResumeRequest) -> Self {
        Self {
            request_id: r.request_id.value(),
            stream_id: r.stream_id.as_bytes().to_vec().into(),
            desired_stream_id_alias: r.desired_stream_id_alias,
            ..Default::default()
        }
    }
}

impl From<DownstreamResumeRequest> for msg::DownstreamResumeRequest {
    fn from(r: DownstreamResumeRequest) -> Self {
        Self {
            request_id: r.request_id.into(),
            stream_id: uuid::Builder::from_slice(&r.stream_id)
                .unwrap_or_else(|_| uuid::Builder::nil())
                .into_uuid(),
            desired_stream_id_alias: r.desired_stream_id_alias,
        }
    }
}

impl From<DownstreamResumeRequest> for msg::Message {
    fn from(r: DownstreamResumeRequest) -> Self {
        Self::DownstreamResumeRequest(r.into())
    }
}

impl From<msg::DownstreamResumeResponse> for DownstreamResumeResponse {
    fn from(r: msg::DownstreamResumeResponse) -> Self {
        Self {
            request_id: r.request_id.value(),
            result_code: r.result_code.into(),
            result_string: r.result_string,
            ..Default::default()
        }
    }
}

impl From<DownstreamResumeResponse> for msg::DownstreamResumeResponse {
    fn from(r: DownstreamResumeResponse) -> Self {
        Self {
            request_id: r.request_id.into(),
            result_code: r.result_code.into(),
            result_string: r.result_string,
        }
    }
}

impl From<DownstreamResumeResponse> for msg::Message {
    fn from(r: DownstreamResumeResponse) -> Self {
        Self::DownstreamResumeResponse(r.into())
    }
}

impl From<msg::DownstreamCloseRequest> for DownstreamCloseRequest {
    fn from(r: msg::DownstreamCloseRequest) -> Self {
        Self {
            request_id: r.request_id.value(),
            stream_id: r.stream_id.as_bytes().to_vec().into(),
            ..Default::default()
        }
    }
}

impl From<DownstreamCloseRequest> for msg::DownstreamCloseRequest {
    fn from(r: DownstreamCloseRequest) -> Self {
        Self {
            request_id: r.request_id.into(),
            stream_id: uuid::Builder::from_slice(&r.stream_id)
                .unwrap_or_else(|_| uuid::Builder::nil())
                .into_uuid(),
        }
    }
}

impl From<DownstreamCloseRequest> for msg::Message {
    fn from(r: DownstreamCloseRequest) -> Self {
        Self::DownstreamCloseRequest(r.into())
    }
}

impl From<msg::DownstreamCloseResponse> for DownstreamCloseResponse {
    fn from(r: msg::DownstreamCloseResponse) -> Self {
        Self {
            request_id: r.request_id.value(),
            result_code: r.result_code.into(),
            result_string: r.result_string,
            ..Default::default()
        }
    }
}

impl From<DownstreamCloseResponse> for msg::DownstreamCloseResponse {
    fn from(r: DownstreamCloseResponse) -> Self {
        Self {
            request_id: r.request_id.into(),
            result_code: r.result_code.into(),
            result_string: r.result_string,
        }
    }
}

impl From<DownstreamCloseResponse> for msg::Message {
    fn from(r: DownstreamCloseResponse) -> Self {
        Self::DownstreamCloseResponse(r.into())
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
        invalid_uuid!(DownstreamOpenResponse, assigned_stream_id);
        invalid_uuid!(DownstreamResumeRequest, stream_id);
        invalid_uuid!(DownstreamCloseRequest, stream_id);
    }
}
