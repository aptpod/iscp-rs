use std::collections::{BTreeMap, HashMap};

use super::{downstream::DownstreamReordering, DownstreamChunk, DownstreamConfig, QoS};
use crate::error::Error;

use uuid::Uuid;

pub struct DownReorderer {
    enabled: bool,
    strict: bool,
    limit: usize,
    next: HashMap<Uuid, u32>,
    store: HashMap<Uuid, BTreeMap<u32, DownstreamChunk>>,
}

pub enum DownReordererIter<'a> {
    NotReordered(Option<DownstreamChunk>),
    Reordered(&'a mut DownReorderer, Uuid),
}

impl DownReorderer {
    pub fn new(config: &DownstreamConfig) -> Self {
        let enabled =
            config.qos == QoS::Reliable && config.reordering != DownstreamReordering::None;

        Self {
            enabled,
            strict: config.reordering == DownstreamReordering::Strict,
            limit: config.reordering_chunks,
            next: HashMap::new(),
            store: HashMap::new(),
        }
    }

    pub fn iter(&mut self, chunk: DownstreamChunk) -> Result<DownReordererIter<'_>, Error> {
        if self.enabled {
            let stream_uuid = chunk.upstream.stream_id;
            if let std::collections::hash_map::Entry::Vacant(e) = self.next.entry(stream_uuid) {
                e.insert(chunk.sequence_number);
                self.store.insert(stream_uuid, BTreeMap::new());
            }

            if self.strict
                && self.store.get_mut(&stream_uuid).unwrap().len() >= self.limit
                && self.next[&stream_uuid] != chunk.sequence_number
            {
                return Err(Error::Reordering(stream_uuid));
            }

            self.store
                .get_mut(&stream_uuid)
                .unwrap()
                .insert(chunk.sequence_number, chunk);
            Ok(DownReordererIter::Reordered(self, stream_uuid))
        } else {
            Ok(DownReordererIter::NotReordered(Some(chunk)))
        }
    }

    fn pop(&mut self, stream_uuid: &Uuid) -> Option<DownstreamChunk> {
        debug_assert!(self.enabled);

        let store = self.store.get_mut(stream_uuid).unwrap();
        let next = self.next.get_mut(stream_uuid).unwrap();

        if !self.strict && store.len() >= self.limit {
            if let Some((n, chunk)) = store.pop_first() {
                *next = n.wrapping_add(1);
                return Some(chunk);
            } else {
                return None;
            }
        }

        if let Some(chunk) = store.remove(next) {
            *next = next.wrapping_add(1);
            Some(chunk)
        } else {
            None
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }
}

impl Iterator for DownReordererIter<'_> {
    type Item = DownstreamChunk;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            DownReordererIter::NotReordered(chunk) => chunk.take(),
            DownReordererIter::Reordered(reorderer, stream_uuid) => reorderer.pop(stream_uuid),
        }
    }
}
