use chrono::TimeZone;

use crate::encoding::internal::autogen::{
    BaseTime, DownstreamAbnormalClose, DownstreamMetadata, DownstreamMetadataAck,
    DownstreamNormalClose, DownstreamOpen, DownstreamResume, UpstreamAbnormalClose,
    UpstreamMetadata, UpstreamMetadataAck, UpstreamMetadataExtensionFields, UpstreamNormalClose,
    UpstreamOpen, UpstreamResume,
};
use crate::message as msg;

impl From<msg::SendableMetadata>
    for crate::encoding::internal::autogen::upstream_metadata::Metadata
{
    fn from(m: msg::SendableMetadata) -> Self {
        match m {
            msg::SendableMetadata::BaseTime(bt) => Self::BaseTime(bt.into()),
        }
    }
}
impl From<crate::encoding::internal::autogen::upstream_metadata::Metadata>
    for msg::SendableMetadata
{
    fn from(m: crate::encoding::internal::autogen::upstream_metadata::Metadata) -> Self {
        use crate::encoding::internal::autogen::upstream_metadata::Metadata as OneOf;
        match m {
            OneOf::BaseTime(bt) => Self::BaseTime(bt.into()),
        }
    }
}

impl From<msg::ReceivableMetadata>
    for crate::encoding::internal::autogen::downstream_metadata::Metadata
{
    fn from(m: msg::ReceivableMetadata) -> Self {
        match m {
            msg::ReceivableMetadata::BaseTime(bt) => Self::BaseTime(bt.into()),

            msg::ReceivableMetadata::DownstreamOpen(o) => Self::DownstreamOpen(o.into()),
            msg::ReceivableMetadata::DownstreamNormalClose(c) => {
                Self::DownstreamNormalClose(c.into())
            }
            msg::ReceivableMetadata::DownstreamAbnormalClose(c) => {
                Self::DownstreamAbnormalClose(c.into())
            }
            msg::ReceivableMetadata::DownstreamResume(r) => Self::DownstreamResume(r.into()),

            msg::ReceivableMetadata::UpstreamOpen(o) => Self::UpstreamOpen(o.into()),
            msg::ReceivableMetadata::UpstreamNormalClose(c) => Self::UpstreamNormalClose(c.into()),
            msg::ReceivableMetadata::UpstreamAbnormalClose(c) => {
                Self::UpstreamAbnormalClose(c.into())
            }
            msg::ReceivableMetadata::UpstreamResume(r) => Self::UpstreamResume(r.into()),
        }
    }
}
impl From<crate::encoding::internal::autogen::downstream_metadata::Metadata>
    for msg::ReceivableMetadata
{
    fn from(m: crate::encoding::internal::autogen::downstream_metadata::Metadata) -> Self {
        use crate::encoding::internal::autogen::downstream_metadata::Metadata as OneOf;
        match m {
            OneOf::BaseTime(bt) => Self::BaseTime(bt.into()),

            OneOf::DownstreamOpen(o) => Self::DownstreamOpen(o.into()),
            OneOf::DownstreamNormalClose(c) => Self::DownstreamNormalClose(c.into()),
            OneOf::DownstreamAbnormalClose(c) => Self::DownstreamAbnormalClose(c.into()),
            OneOf::DownstreamResume(r) => Self::DownstreamResume(r.into()),

            OneOf::UpstreamOpen(o) => Self::UpstreamOpen(o.into()),
            OneOf::UpstreamNormalClose(c) => Self::UpstreamNormalClose(c.into()),
            OneOf::UpstreamAbnormalClose(c) => Self::UpstreamAbnormalClose(c.into()),
            OneOf::UpstreamResume(r) => Self::UpstreamResume(r.into()),
        }
    }
}

impl From<msg::BaseTime> for BaseTime {
    fn from(bt: msg::BaseTime) -> Self {
        Self {
            session_id: bt.session_id,
            name: bt.name,
            priority: bt.priority,
            elapsed_time: bt.elapsed_time.num_nanoseconds().unwrap_or(0) as u64,
            base_time: bt.base_time.timestamp_nanos(),
        }
    }
}

impl From<BaseTime> for msg::BaseTime {
    fn from(bt: BaseTime) -> Self {
        Self {
            session_id: bt.session_id,
            name: bt.name,
            priority: bt.priority,
            elapsed_time: chrono::Duration::nanoseconds(bt.elapsed_time as i64),
            base_time: chrono::Utc.timestamp_nanos(bt.base_time),
        }
    }
}

impl From<msg::DownstreamAbnormalClose> for DownstreamAbnormalClose {
    fn from(c: msg::DownstreamAbnormalClose) -> Self {
        Self {
            stream_id: c.stream_id.as_bytes().to_vec(),
        }
    }
}

impl From<DownstreamAbnormalClose> for msg::DownstreamAbnormalClose {
    fn from(c: DownstreamAbnormalClose) -> Self {
        Self {
            stream_id: uuid::Builder::from_slice(&c.stream_id)
                .unwrap_or_else(|_| uuid::Builder::nil())
                .into_uuid(),
        }
    }
}

impl From<msg::DownstreamNormalClose> for DownstreamNormalClose {
    fn from(c: msg::DownstreamNormalClose) -> Self {
        Self {
            stream_id: c.stream_id.as_bytes().to_vec(),
        }
    }
}

impl From<DownstreamNormalClose> for msg::DownstreamNormalClose {
    fn from(c: DownstreamNormalClose) -> Self {
        Self {
            stream_id: uuid::Builder::from_slice(&c.stream_id)
                .unwrap_or_else(|_| uuid::Builder::nil())
                .into_uuid(),
        }
    }
}

impl From<msg::DownstreamOpen> for DownstreamOpen {
    fn from(o: msg::DownstreamOpen) -> Self {
        Self {
            stream_id: o.stream_id.as_bytes().to_vec(),
            downstream_filters: o.downstream_filters,
            qos: o.qos.into(),
        }
    }
}

impl From<DownstreamOpen> for msg::DownstreamOpen {
    fn from(o: DownstreamOpen) -> Self {
        Self {
            stream_id: uuid::Builder::from_slice(&o.stream_id)
                .unwrap_or_else(|_| uuid::Builder::nil())
                .into_uuid(),
            downstream_filters: o.downstream_filters,
            qos: o.qos.into(),
        }
    }
}

impl From<msg::DownstreamResume> for DownstreamResume {
    fn from(r: msg::DownstreamResume) -> Self {
        Self {
            stream_id: r.stream_id.as_bytes().to_vec(),
            downstream_filters: r.downstream_filters,
            qos: r.qos.into(),
        }
    }
}

impl From<DownstreamResume> for msg::DownstreamResume {
    fn from(r: DownstreamResume) -> Self {
        Self {
            stream_id: uuid::Builder::from_slice(&r.stream_id)
                .unwrap_or_else(|_| uuid::Builder::nil())
                .into_uuid(),
            downstream_filters: r.downstream_filters,
            qos: r.qos.into(),
        }
    }
}

impl From<msg::UpstreamAbnormalClose> for UpstreamAbnormalClose {
    fn from(c: msg::UpstreamAbnormalClose) -> Self {
        Self {
            stream_id: c.stream_id.as_bytes().to_vec(),
            session_id: c.session_id,
        }
    }
}

impl From<UpstreamAbnormalClose> for msg::UpstreamAbnormalClose {
    fn from(c: UpstreamAbnormalClose) -> Self {
        Self {
            stream_id: uuid::Builder::from_slice(&c.stream_id)
                .unwrap_or_else(|_| uuid::Builder::nil())
                .into_uuid(),
            session_id: c.session_id,
        }
    }
}

impl From<msg::UpstreamNormalClose> for UpstreamNormalClose {
    fn from(c: msg::UpstreamNormalClose) -> Self {
        Self {
            stream_id: c.stream_id.as_bytes().to_vec(),
            session_id: c.session_id.to_string(),
            total_data_points: c.total_data_points,
            final_sequence_number: c.final_sequence_number,
        }
    }
}

impl From<UpstreamNormalClose> for msg::UpstreamNormalClose {
    fn from(c: UpstreamNormalClose) -> Self {
        Self {
            stream_id: uuid::Builder::from_slice(&c.stream_id)
                .unwrap_or_else(|_| uuid::Builder::nil())
                .into_uuid(),
            session_id: uuid::Uuid::parse_str(&c.session_id).unwrap_or_default(),
            total_data_points: c.total_data_points,
            final_sequence_number: c.final_sequence_number,
        }
    }
}

impl From<msg::UpstreamOpen> for UpstreamOpen {
    fn from(o: msg::UpstreamOpen) -> Self {
        Self {
            stream_id: o.stream_id.as_bytes().to_vec(),
            session_id: o.session_id,
            qos: o.qos.into(),
        }
    }
}

impl From<UpstreamOpen> for msg::UpstreamOpen {
    fn from(o: UpstreamOpen) -> Self {
        Self {
            stream_id: uuid::Builder::from_slice(&o.stream_id)
                .unwrap_or_else(|_| uuid::Builder::nil())
                .into_uuid(),
            session_id: o.session_id,
            qos: o.qos.into(),
        }
    }
}

impl From<msg::UpstreamResume> for UpstreamResume {
    fn from(r: msg::UpstreamResume) -> Self {
        Self {
            stream_id: r.stream_id.as_bytes().to_vec(),
            session_id: r.session_id,
            qos: r.qos.into(),
        }
    }
}

impl From<UpstreamResume> for msg::UpstreamResume {
    fn from(r: UpstreamResume) -> Self {
        Self {
            stream_id: uuid::Builder::from_slice(&r.stream_id)
                .unwrap_or_else(|_| uuid::Builder::nil())
                .into_uuid(),
            session_id: r.session_id,
            qos: r.qos.into(),
        }
    }
}

impl From<msg::UpstreamMetadata> for UpstreamMetadata {
    fn from(m: msg::UpstreamMetadata) -> Self {
        let mut res = Self {
            request_id: m.request_id.value(),
            metadata: Some(m.metadata.into()),
            ..Default::default()
        };
        if let Some(persist) = m.persist {
            let ext = UpstreamMetadataExtensionFields { persist };
            res.extension_fields = Some(ext);
        }

        res
    }
}

impl TryFrom<UpstreamMetadata> for msg::UpstreamMetadata {
    type Error = crate::error::Error;

    fn try_from(m: UpstreamMetadata) -> Result<Self, Self::Error> {
        let metadata = match &m.metadata {
            Some(m) => m.clone(),
            None => {
                return Err(crate::error::Error::malformed_message(
                    "no upstream metadata",
                ))
            }
        };
        let persist = m.extension_fields.map(|ext| ext.persist);

        Ok(Self {
            request_id: m.request_id.into(),
            metadata: metadata.into(),
            persist,
        })
    }
}

impl TryFrom<UpstreamMetadata> for msg::Message {
    type Error = crate::error::Error;
    fn try_from(m: UpstreamMetadata) -> Result<Self, Self::Error> {
        Ok(Self::UpstreamMetadata(m.try_into()?))
    }
}

impl From<msg::UpstreamMetadataAck> for UpstreamMetadataAck {
    fn from(ack: msg::UpstreamMetadataAck) -> Self {
        Self {
            request_id: ack.request_id.value(),
            result_code: ack.result_code.into(),
            result_string: ack.result_string,
            ..Default::default()
        }
    }
}

impl From<UpstreamMetadataAck> for msg::UpstreamMetadataAck {
    fn from(ack: UpstreamMetadataAck) -> Self {
        Self {
            request_id: ack.request_id.into(),
            result_code: ack.result_code.into(),
            result_string: ack.result_string,
        }
    }
}

impl From<UpstreamMetadataAck> for msg::Message {
    fn from(m: UpstreamMetadataAck) -> Self {
        Self::UpstreamMetadataAck(m.into())
    }
}

impl From<msg::DownstreamMetadata> for DownstreamMetadata {
    fn from(m: msg::DownstreamMetadata) -> Self {
        Self {
            source_node_id: m.source_node_id,
            request_id: m.request_id.value(),
            metadata: Some(m.metadata.into()),
            ..Default::default()
        }
    }
}

impl TryFrom<DownstreamMetadata> for msg::DownstreamMetadata {
    type Error = crate::error::Error;
    fn try_from(m: DownstreamMetadata) -> Result<Self, Self::Error> {
        let metadata = match &m.metadata {
            Some(m) => m.clone(),
            None => {
                return Err(crate::error::Error::malformed_message(
                    "no downstream metadata",
                ))
            }
        };
        Ok(Self {
            request_id: m.request_id.into(),
            source_node_id: m.source_node_id,
            metadata: metadata.into(),
        })
    }
}

impl TryFrom<DownstreamMetadata> for msg::Message {
    type Error = crate::error::Error;
    fn try_from(m: DownstreamMetadata) -> Result<Self, Self::Error> {
        Ok(Self::DownstreamMetadata(m.try_into()?))
    }
}

impl From<msg::DownstreamMetadataAck> for DownstreamMetadataAck {
    fn from(ack: msg::DownstreamMetadataAck) -> Self {
        Self {
            request_id: ack.request_id.value(),
            result_code: ack.result_code.into(),
            result_string: ack.result_string,
            ..Default::default()
        }
    }
}

impl From<DownstreamMetadataAck> for msg::DownstreamMetadataAck {
    fn from(ack: DownstreamMetadataAck) -> Self {
        Self {
            request_id: ack.request_id.into(),
            result_code: ack.result_code.into(),
            result_string: ack.result_string,
        }
    }
}

impl From<DownstreamMetadataAck> for msg::Message {
    fn from(m: DownstreamMetadataAck) -> Self {
        Self::DownstreamMetadataAck(m.into())
    }
}

#[cfg(test)]
#[allow(clippy::all)]
mod test {
    use super::*;

    macro_rules! invalid_uuid {
        ($msg:ident, $id:ident) => {
            let testee = $msg {
                $id: vec![0, 159, 146, 150],
                ..Default::default()
            };

            let want = msg::$msg::default();
            assert_eq!(want, msg::$msg::from(testee));
        };
    }

    #[test]
    fn invalid_uuid() {
        invalid_uuid!(DownstreamAbnormalClose, stream_id);
        invalid_uuid!(DownstreamNormalClose, stream_id);
        invalid_uuid!(DownstreamOpen, stream_id);
        invalid_uuid!(DownstreamResume, stream_id);

        invalid_uuid!(UpstreamAbnormalClose, stream_id);
        invalid_uuid!(UpstreamNormalClose, stream_id);
        invalid_uuid!(UpstreamOpen, stream_id);
        invalid_uuid!(UpstreamResume, stream_id);
    }
}
