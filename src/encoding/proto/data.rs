use crate::encoding::internal::autogen::{
    downstream_chunk::UpstreamOrAlias, DataPointGroup, DownstreamChunk, DownstreamChunkAck,
    DownstreamChunkAckComplete, DownstreamChunkResult, StreamChunk, UpstreamChunk,
    UpstreamChunkAck, UpstreamChunkResult, UpstreamInfo,
};

use crate::message as msg;
use uuid::Uuid;

impl From<UpstreamChunkResult> for msg::UpstreamChunkResult {
    fn from(r: UpstreamChunkResult) -> Self {
        Self {
            sequence_number: r.sequence_number,
            result_code: r.result_code.into(),
            result_string: r.result_string,
        }
    }
}

impl From<msg::UpstreamChunkResult> for UpstreamChunkResult {
    fn from(r: msg::UpstreamChunkResult) -> Self {
        Self {
            sequence_number: r.sequence_number,
            result_code: r.result_code.into(),
            result_string: r.result_string,
            ..Default::default()
        }
    }
}

impl From<DownstreamChunkResult> for msg::DownstreamChunkResult {
    fn from(r: DownstreamChunkResult) -> Self {
        Self {
            stream_id_of_upstream: Uuid::from_slice(&r.stream_id_of_upstream).unwrap_or_default(),
            sequence_number_in_upstream: r.sequence_number_in_upstream,
            result_code: r.result_code.into(),
            result_string: r.result_string,
        }
    }
}

impl From<msg::DownstreamChunkResult> for DownstreamChunkResult {
    fn from(r: msg::DownstreamChunkResult) -> Self {
        Self {
            stream_id_of_upstream: r.stream_id_of_upstream.as_bytes().to_vec().into(),
            sequence_number_in_upstream: r.sequence_number_in_upstream,
            result_code: r.result_code.into(),
            result_string: r.result_string,
            ..Default::default()
        }
    }
}

impl From<msg::DataPointGroup> for DataPointGroup {
    fn from(d: msg::DataPointGroup) -> Self {
        Self {
            data_points: d.data_points,
            data_id_or_alias: Some(d.data_id_or_alias),
        }
    }
}

impl TryFrom<DataPointGroup> for msg::DataPointGroup {
    type Error = crate::error::Error;
    fn try_from(d: DataPointGroup) -> Result<Self, Self::Error> {
        Ok(Self {
            data_points: d.data_points,
            data_id_or_alias: d
                .data_id_or_alias
                .ok_or_else(|| crate::error::Error::malformed_message("no data_id_or_alias"))?,
        })
    }
}

impl From<msg::StreamChunk> for StreamChunk {
    fn from(c: msg::StreamChunk) -> Self {
        Self {
            sequence_number: c.sequence_number,
            data_point_groups: c
                .data_point_groups
                .into_iter()
                .map(|data_point_group| data_point_group.into())
                .collect(),
        }
    }
}

impl TryFrom<StreamChunk> for msg::StreamChunk {
    type Error = crate::error::Error;
    fn try_from(c: StreamChunk) -> Result<Self, Self::Error> {
        Ok(Self {
            sequence_number: c.sequence_number,
            data_point_groups: c
                .data_point_groups
                .into_iter()
                .map(|data_point_group| data_point_group.try_into())
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

impl From<msg::UpstreamOrAlias> for UpstreamOrAlias {
    fn from(c: msg::UpstreamOrAlias) -> Self {
        match c {
            msg::UpstreamOrAlias::Alias(a) => Self::UpstreamAlias(a),
            msg::UpstreamOrAlias::UpstreamInfo(info) => Self::UpstreamInfo(info.into()),
        }
    }
}

impl TryFrom<UpstreamOrAlias> for msg::UpstreamOrAlias {
    type Error = crate::error::Error;
    fn try_from(c: UpstreamOrAlias) -> Result<Self, Self::Error> {
        Ok(match c {
            UpstreamOrAlias::UpstreamAlias(a) => Self::Alias(a),
            UpstreamOrAlias::UpstreamInfo(info) => Self::UpstreamInfo(info.into()),
        })
    }
}

impl From<msg::UpstreamChunk> for UpstreamChunk {
    fn from(c: msg::UpstreamChunk) -> Self {
        Self {
            stream_id_alias: c.stream_id_alias,
            stream_chunk: Some(c.stream_chunk.into()),
            data_ids: c.data_ids,
            extension_fields: None,
        }
    }
}

impl TryFrom<UpstreamChunk> for msg::UpstreamChunk {
    type Error = crate::error::Error;
    fn try_from(c: UpstreamChunk) -> Result<Self, Self::Error> {
        Ok(Self {
            stream_id_alias: c.stream_id_alias,
            stream_chunk: c
                .stream_chunk
                .ok_or_else(|| Self::Error::malformed_message("no stream_chunk"))?
                .try_into()?,
            data_ids: c.data_ids,
        })
    }
}
impl From<msg::UpstreamInfo> for UpstreamInfo {
    fn from(i: msg::UpstreamInfo) -> Self {
        Self {
            source_node_id: i.source_node_id,
            session_id: i.session_id,
            stream_id: i.stream_id.as_bytes().to_vec().into(),
        }
    }
}

impl From<UpstreamInfo> for msg::UpstreamInfo {
    fn from(i: UpstreamInfo) -> Self {
        Self {
            source_node_id: i.source_node_id,
            session_id: i.session_id,
            stream_id: uuid::Builder::from_slice(&i.stream_id)
                .unwrap_or_else(|_| uuid::Builder::nil())
                .into_uuid(),
        }
    }
}

impl From<msg::DownstreamChunk> for DownstreamChunk {
    fn from(c: msg::DownstreamChunk) -> Self {
        Self {
            stream_id_alias: c.stream_id_alias,
            stream_chunk: Some(c.stream_chunk.into()),
            upstream_or_alias: Some(c.upstream_or_alias.into()),
            ..Default::default()
        }
    }
}

impl TryFrom<DownstreamChunk> for msg::DownstreamChunk {
    type Error = crate::error::Error;
    fn try_from(c: DownstreamChunk) -> Result<Self, Self::Error> {
        Ok(Self {
            stream_id_alias: c.stream_id_alias,
            stream_chunk: c
                .stream_chunk
                .ok_or_else(|| Self::Error::malformed_message("no stream_chunk"))?
                .try_into()?,
            upstream_or_alias: c
                .upstream_or_alias
                .ok_or_else(|| Self::Error::malformed_message("no upstream_or_alias"))?
                .try_into()?,
        })
    }
}

impl TryFrom<DownstreamChunk> for msg::Message {
    type Error = crate::error::Error;
    fn try_from(p: DownstreamChunk) -> Result<Self, Self::Error> {
        Ok(Self::DownstreamChunk(p.try_into()?))
    }
}

impl From<msg::DownstreamChunkAck> for DownstreamChunkAck {
    fn from(ack: msg::DownstreamChunkAck) -> Self {
        Self {
            ack_id: ack.ack_id,
            stream_id_alias: ack.stream_id_alias,
            results: ack.results.into_iter().map(|r| r.into()).collect(),
            data_id_aliases: ack.data_id_aliases,
            upstream_aliases: ack
                .upstream_aliases
                .into_iter()
                .map(|(a, u)| (a, u.into()))
                .collect(),
            ..Default::default()
        }
    }
}

impl From<DownstreamChunkAck> for msg::DownstreamChunkAck {
    fn from(ack: DownstreamChunkAck) -> Self {
        Self {
            ack_id: ack.ack_id,
            stream_id_alias: ack.stream_id_alias,
            results: ack.results.into_iter().map(|r| r.into()).collect(),
            upstream_aliases: ack
                .upstream_aliases
                .into_iter()
                .map(|(a, u)| (a, u.into()))
                .collect(),
            data_id_aliases: ack.data_id_aliases,
        }
    }
}

impl From<DownstreamChunkAck> for msg::Message {
    fn from(p: DownstreamChunkAck) -> Self {
        Self::DownstreamChunkAck(p.into())
    }
}

impl From<msg::DownstreamChunkAckComplete> for DownstreamChunkAckComplete {
    fn from(ack: msg::DownstreamChunkAckComplete) -> Self {
        Self {
            ack_id: ack.ack_id,
            stream_id_alias: ack.stream_id_alias,
            result_code: ack.result_code.into(),
            result_string: ack.result_string,
            ..Default::default()
        }
    }
}

impl From<DownstreamChunkAckComplete> for msg::DownstreamChunkAckComplete {
    fn from(ack: DownstreamChunkAckComplete) -> Self {
        Self {
            ack_id: ack.ack_id,
            stream_id_alias: ack.stream_id_alias,
            result_code: ack.result_code.into(),
            result_string: ack.result_string,
        }
    }
}

impl From<DownstreamChunkAckComplete> for msg::Message {
    fn from(p: DownstreamChunkAckComplete) -> Self {
        Self::DownstreamChunkAckComplete(p.into())
    }
}

impl TryFrom<UpstreamChunk> for msg::Message {
    type Error = crate::error::Error;
    fn try_from(c: UpstreamChunk) -> Result<Self, Self::Error> {
        Ok(Self::UpstreamChunk(c.try_into()?))
    }
}

impl From<msg::UpstreamChunkAck> for UpstreamChunkAck {
    fn from(ack: msg::UpstreamChunkAck) -> Self {
        Self {
            stream_id_alias: ack.stream_id_alias,
            results: ack.results.into_iter().map(|r| r.into()).collect(),
            data_id_aliases: ack.data_id_aliases,
            ..Default::default()
        }
    }
}

impl From<UpstreamChunkAck> for msg::UpstreamChunkAck {
    fn from(ack: UpstreamChunkAck) -> Self {
        Self {
            stream_id_alias: ack.stream_id_alias,
            results: ack.results.into_iter().map(|r| r.into()).collect(),
            data_id_aliases: ack.data_id_aliases,
        }
    }
}

impl From<UpstreamChunkAck> for msg::Message {
    fn from(p: UpstreamChunkAck) -> Self {
        Self::UpstreamChunkAck(p.into())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn utf8() {
        let info = UpstreamInfo {
            stream_id: vec![0, 159, 146, 150].into(),
            ..Default::default()
        };
        let want = msg::UpstreamInfo::default();
        assert_eq!(want, msg::UpstreamInfo::from(info))
    }
}
