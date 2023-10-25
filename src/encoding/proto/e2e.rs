use crate::{encoding::internal::autogen, message as msg};

impl From<msg::DownstreamCall> for autogen::DownstreamCall {
    fn from(c: msg::DownstreamCall) -> Self {
        Self {
            call_id: c.call_id,
            source_node_id: c.source_node_id,
            name: c.name,
            type_: c.type_,
            request_call_id: c.request_call_id,
            payload: c.payload,
            ..Default::default()
        }
    }
}

impl From<autogen::DownstreamCall> for msg::DownstreamCall {
    fn from(c: autogen::DownstreamCall) -> Self {
        Self {
            call_id: c.call_id,
            source_node_id: c.source_node_id,
            name: c.name,
            type_: c.type_,
            request_call_id: c.request_call_id,
            payload: c.payload,
        }
    }
}

impl From<autogen::DownstreamCall> for msg::Message {
    fn from(c: autogen::DownstreamCall) -> Self {
        Self::DownstreamCall(c.into())
    }
}

impl From<msg::UpstreamCall> for autogen::UpstreamCall {
    fn from(c: msg::UpstreamCall) -> Self {
        Self {
            call_id: c.call_id,
            name: c.name,
            destination_node_id: c.destination_node_id,
            type_: c.type_,
            request_call_id: c.request_call_id,
            payload: c.payload,
            ..Default::default()
        }
    }
}

impl From<autogen::UpstreamCall> for msg::UpstreamCall {
    fn from(c: autogen::UpstreamCall) -> Self {
        Self {
            call_id: c.call_id,
            destination_node_id: c.destination_node_id,
            name: c.name,
            type_: c.type_,
            request_call_id: c.request_call_id,
            payload: c.payload,
        }
    }
}

impl From<autogen::UpstreamCall> for msg::Message {
    fn from(c: autogen::UpstreamCall) -> Self {
        Self::UpstreamCall(c.into())
    }
}

impl From<autogen::UpstreamCallAck> for msg::UpstreamCallAck {
    fn from(a: autogen::UpstreamCallAck) -> Self {
        Self {
            call_id: a.call_id,
            result_code: a.result_code.into(),
            result_string: a.result_string,
        }
    }
}

impl From<msg::UpstreamCallAck> for autogen::UpstreamCallAck {
    fn from(a: msg::UpstreamCallAck) -> Self {
        Self {
            call_id: a.call_id,
            result_code: a.result_code.into(),
            result_string: a.result_string,
            ..Default::default()
        }
    }
}

impl From<autogen::UpstreamCallAck> for msg::Message {
    fn from(c: autogen::UpstreamCallAck) -> Self {
        Self::UpstreamCallAck(c.into())
    }
}
