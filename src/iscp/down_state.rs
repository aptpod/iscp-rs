use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use crossbeam::atomic::AtomicCell;
use fnv::FnvHashMap;
use uuid::Uuid;

use crate::internal::conv_hasher;
use crate::message::ResultCode;
use crate::Error;

use super::misc::parse_stream_id;
use super::types::*;

pub(super) struct State {
    data_id_aliases: Mutex<FnvHashMap<u32, DataId>>,
    infos: Mutex<FnvHashMap<u32, Arc<UpstreamInfo>>>,
    for_ack: Mutex<HashMap<u32, crate::message::UpstreamInfo>>,
    last_issued_upstream_info_alias: AtomicCell<Option<u32>>,
    last_issued_chunk_ack_id: AtomicCell<Option<u32>>,
    results: Mutex<Vec<crate::message::DownstreamChunkResult>>,
}

/// Downstream state.
pub struct DownstreamState {
    pub data_id_aliases: HashMap<u32, DataId>,
    pub last_issued_data_id_alias: Option<u32>,
    pub upstream_infos: HashMap<u32, Arc<UpstreamInfo>>,
    pub last_issued_upstream_info_alias: Option<u32>,
    pub last_issued_chunk_ack_id: Option<u32>,
}

impl State {
    pub fn new(data_id_aliases: HashMap<u32, DataId>) -> Self {
        Self {
            data_id_aliases: Mutex::new(conv_hasher(data_id_aliases)),
            infos: Mutex::default(),
            for_ack: Mutex::default(),
            last_issued_upstream_info_alias: AtomicCell::new(None),
            last_issued_chunk_ack_id: AtomicCell::new(None),
            results: Mutex::default(),
        }
    }

    pub fn add_sequence_number(&self, stream_id: Uuid, sequence_number: u32) {
        let mut results = self.results.lock().unwrap();
        results.push(crate::message::DownstreamChunkResult {
            stream_id_of_upstream: stream_id.into_bytes().to_vec().into(),
            sequence_number_in_upstream: sequence_number,
            result_code: ResultCode::Succeeded.into(),
            result_string: "OK".into(),
            ..Default::default()
        });
    }

    pub fn add_upstream_info(
        &self,
        upstream_info: crate::message::UpstreamInfo,
    ) -> Result<Arc<UpstreamInfo>, Error> {
        let info = UpstreamInfo {
            session_id: upstream_info.session_id,
            stream_id: parse_stream_id(&upstream_info.stream_id)?,
            source_node_id: upstream_info.source_node_id,
        };
        let mut infos = self.infos.lock().unwrap();
        if let Some((_, v)) = infos.iter().find(|(_, v)| ***v == info) {
            Ok(v.clone())
        } else {
            let info = Arc::new(info);
            let alias = if let Some(alias) = self.last_issued_upstream_info_alias.load() {
                alias.checked_add(1).expect("too many upstream info")
            } else {
                0
            };
            self.last_issued_upstream_info_alias.store(Some(alias));
            infos.insert(alias, info.clone());
            self.for_ack.lock().unwrap().insert(
                alias,
                crate::message::UpstreamInfo {
                    session_id: info.session_id.clone(),
                    stream_id: info.stream_id.into_bytes().to_vec().into(),
                    source_node_id: info.source_node_id.clone(),
                },
            );
            Ok(info)
        }
    }

    pub fn get_upstream_info(&self, alias: u32) -> Result<Arc<UpstreamInfo>, Error> {
        self.infos
            .lock()
            .unwrap()
            .get(&alias)
            .cloned()
            .ok_or_else(|| Error::invalid_value(format!("unknown upstream info alias ({})", alias)))
    }

    pub fn take_ack(&self, stream_id_alias: u32) -> Option<crate::message::DownstreamChunkAck> {
        let results = std::mem::take(&mut *self.results.lock().unwrap());
        let upstream_aliases = std::mem::take(&mut *self.for_ack.lock().unwrap());

        if results.is_empty() && upstream_aliases.is_empty() {
            return None;
        }

        let ack_id = if let Some(ack_id) = self.last_issued_chunk_ack_id.load() {
            ack_id.wrapping_add(1)
        } else {
            0
        };
        self.last_issued_chunk_ack_id.store(Some(ack_id));

        Some(crate::message::DownstreamChunkAck {
            stream_id_alias,
            ack_id,
            results,
            upstream_aliases,
            ..Default::default()
        })
    }

    pub fn get_data_id(&self, alias: u32) -> Option<DataId> {
        self.data_id_aliases.lock().unwrap().get(&alias).cloned()
    }

    pub fn last_issued_chunk_ack_id(&self) -> Option<u32> {
        self.last_issued_chunk_ack_id.load()
    }

    pub fn state(&self) -> DownstreamState {
        let data_id_aliases = self.data_id_aliases.lock().unwrap();
        let last_issued_data_id_alias = data_id_aliases.keys().max().copied();
        let upstream_infos = conv_hasher(self.infos.lock().unwrap().clone());

        DownstreamState {
            data_id_aliases: conv_hasher(data_id_aliases.clone()),
            last_issued_data_id_alias,
            upstream_infos,
            last_issued_upstream_info_alias: self.last_issued_upstream_info_alias.load(),
            last_issued_chunk_ack_id: self.last_issued_chunk_ack_id.load(),
        }
    }
}
