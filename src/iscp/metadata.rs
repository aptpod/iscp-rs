//! iSCPメタデータ型の定義

use std::time::SystemTime;

use crate::message::{downstream_metadata::Metadata as MetadataEnum, DownstreamFilter, QoS};

use super::misc::{from_qos_i32, parse_stream_id, unix_epoch_to_system_time};
use super::*;

#[derive(Clone, PartialEq, Debug)]
#[non_exhaustive]
pub enum SendableMetadata {
    BaseTime(BaseTime),
}

#[derive(Clone, PartialEq, Debug)]
#[non_exhaustive]
pub enum ReceivableMetadata {
    BaseTime(BaseTime),
    UpstreamOpen(UpstreamOpen),
    UpstreamAbnormalClose(UpstreamAbnormalClose),
    UpstreamResume(UpstreamResume),
    UpstreamNormalClose(UpstreamNormalClose),
    DownstreamOpen(DownstreamOpen),
    DownstreamAbnormalClose(DownstreamAbnormalClose),
    DownstreamResume(DownstreamResume),
    DownstreamNormalClose(DownstreamNormalClose),
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct BaseTime {
    pub session_id: String,
    pub name: String,
    pub priority: u32,
    pub elapsed_time: Duration,
    pub base_time: SystemTime,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct UpstreamOpen {
    pub stream_id: Uuid,
    pub session_id: String,
    pub qos: QoS,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct UpstreamAbnormalClose {
    pub stream_id: Uuid,
    pub session_id: String,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct UpstreamResume {
    pub stream_id: Uuid,
    pub session_id: String,
    pub qos: QoS,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct UpstreamNormalClose {
    pub stream_id: Uuid,
    pub session_id: String,
    pub total_data_points: u64,
    pub final_sequence_number: u32,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct DownstreamOpen {
    pub stream_id: Uuid,
    pub downstream_filters: Vec<DownstreamFilter>,
    pub qos: QoS,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct DownstreamAbnormalClose {
    pub stream_id: Uuid,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct DownstreamResume {
    pub stream_id: Uuid,
    pub downstream_filters: Vec<DownstreamFilter>,
    pub qos: QoS,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct DownstreamNormalClose {
    pub stream_id: Uuid,
}

macro_rules! impl_from_sendable_metadata {
    ($($member:ident,)*) => {
        $(
            impl From<$member> for SendableMetadata {
                fn from(m: $member) -> Self {
                    Self::$member(m)
                }
            }
        )*
    }
}

impl_from_sendable_metadata!(BaseTime,);

macro_rules! impl_from_receivable_metadata {
    ($($member:ident,)*) => {
        $(
            impl From<$member> for ReceivableMetadata {
                fn from(m: $member) -> Self {
                    Self::$member(m)
                }
            }
        )*
    }
}

impl_from_receivable_metadata!(
    BaseTime,
    UpstreamOpen,
    UpstreamAbnormalClose,
    UpstreamResume,
    UpstreamNormalClose,
    DownstreamOpen,
    DownstreamAbnormalClose,
    DownstreamResume,
    DownstreamNormalClose,
);

impl ReceivableMetadata {
    pub(crate) fn from_prost(metadata: MetadataEnum) -> Result<Self, Error> {
        match metadata {
            MetadataEnum::BaseTime(m) => Ok(ReceivableMetadata::BaseTime(BaseTime {
                session_id: m.session_id,
                name: m.name,
                priority: m.priority,
                elapsed_time: Duration::from_nanos(m.elapsed_time),
                base_time: unix_epoch_to_system_time(m.base_time)?,
            })),
            MetadataEnum::UpstreamOpen(m) => Ok(ReceivableMetadata::UpstreamOpen(UpstreamOpen {
                stream_id: parse_stream_id(&m.stream_id)?,
                session_id: m.session_id,
                qos: from_qos_i32(m.qos)?,
            })),
            MetadataEnum::UpstreamAbnormalClose(m) => Ok(
                ReceivableMetadata::UpstreamAbnormalClose(UpstreamAbnormalClose {
                    stream_id: parse_stream_id(&m.stream_id)?,
                    session_id: m.session_id,
                }),
            ),
            MetadataEnum::UpstreamResume(m) => {
                Ok(ReceivableMetadata::UpstreamResume(UpstreamResume {
                    stream_id: parse_stream_id(&m.stream_id)?,
                    session_id: m.session_id,
                    qos: from_qos_i32(m.qos)?,
                }))
            }
            MetadataEnum::UpstreamNormalClose(m) => Ok(ReceivableMetadata::UpstreamNormalClose(
                UpstreamNormalClose {
                    stream_id: parse_stream_id(&m.stream_id)?,
                    session_id: m.session_id,
                    total_data_points: m.total_data_points,
                    final_sequence_number: m.final_sequence_number,
                },
            )),
            MetadataEnum::DownstreamOpen(m) => {
                Ok(ReceivableMetadata::DownstreamOpen(DownstreamOpen {
                    stream_id: parse_stream_id(&m.stream_id)?,
                    downstream_filters: m.downstream_filters,
                    qos: from_qos_i32(m.qos)?,
                }))
            }
            MetadataEnum::DownstreamAbnormalClose(m) => Ok(
                ReceivableMetadata::DownstreamAbnormalClose(DownstreamAbnormalClose {
                    stream_id: parse_stream_id(&m.stream_id)?,
                }),
            ),
            MetadataEnum::DownstreamResume(m) => {
                Ok(ReceivableMetadata::DownstreamResume(DownstreamResume {
                    stream_id: parse_stream_id(&m.stream_id)?,
                    downstream_filters: m.downstream_filters,
                    qos: from_qos_i32(m.qos)?,
                }))
            }
            MetadataEnum::DownstreamNormalClose(m) => Ok(
                ReceivableMetadata::DownstreamNormalClose(DownstreamNormalClose {
                    stream_id: parse_stream_id(&m.stream_id)?,
                }),
            ),
        }
    }
}

pub struct MetadataSender<'a> {
    conn: &'a Conn,
    metadata: SendableMetadata,
    persist: Option<bool>,
}

impl Conn {
    /// メタデータを送信
    pub fn metadata_sender<T: Into<SendableMetadata>>(&self, metadata: T) -> MetadataSender<'_> {
        MetadataSender {
            conn: self,
            metadata: metadata.into(),
            persist: None,
        }
    }
}

impl MetadataSender<'_> {
    pub async fn send(self) -> Result<(), Error> {
        let wire_conn = self.conn.inner.wire_conn()?;
        let mut request = convert_to_upstream_metadata(self.metadata)?;

        if let Some(persist) = self.persist {
            request.extension_fields =
                Some(crate::message::extensions::UpstreamMetadataExtensionFields { persist });
        }

        let _response = wire_conn.request_message_need_response(request).await?;
        Ok(())
    }

    pub fn persist(mut self, persist: bool) -> Self {
        self.persist = Some(persist);
        self
    }
}

fn convert_to_upstream_metadata(
    metadata: SendableMetadata,
) -> Result<crate::message::UpstreamMetadata, Error> {
    let SendableMetadata::BaseTime(base_time) = metadata;
    let Ok(elapsed_time) = base_time.elapsed_time.as_nanos().try_into() else {
        return Err(Error::invalid_value("elapsed_time overflow"));
    };
    let Ok(base_time_nsec) = base_time
        .base_time
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|e| Error::invalid_value(e.to_string()))?
        .as_nanos()
        .try_into()
    else {
        return Err(Error::invalid_value("elapsed_time overflow"));
    };

    let base_time = crate::message::BaseTime {
        session_id: base_time.session_id,
        name: base_time.name,
        priority: base_time.priority,
        elapsed_time,
        base_time: base_time_nsec,
    };

    Ok(crate::message::UpstreamMetadata {
        metadata: Some(crate::message::upstream_metadata::Metadata::BaseTime(
            base_time,
        )),
        ..Default::default()
    })
}
