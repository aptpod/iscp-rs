//! ストレージに関するモジュールです。

use bytes::Bytes;
use std::{
    collections::{hash_map::Entry, HashMap},
    fmt::Debug,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

use crate::{Cancel, DataId, DataPoint, DataPointGroup, Error, Result};

pub(crate) trait SentStorage: Sync + Send + Debug {
    fn store(&self, stream_id: Uuid, serial: u32, dpgs: Vec<DataPointGroup>) -> Result<()>;
    fn remove(&self, stream_id: Uuid, serial: u32) -> Option<Vec<DataPointGroup>>;

    /// remaining returns remaining counts of serial number
    fn remaining(&self, stream_id: Uuid) -> usize;
}

impl<S: SentStorage + ?Sized> SentStorage for Box<S> {
    fn store(&self, stream_id: Uuid, serial: u32, dpgs: Vec<DataPointGroup>) -> Result<()> {
        (**self).store(stream_id, serial, dpgs)
    }
    fn remove(&self, stream_id: Uuid, serial: u32) -> Option<Vec<DataPointGroup>> {
        (**self).remove(stream_id, serial)
    }
    fn remaining(&self, stream_id: Uuid) -> usize {
        (**self).remaining(stream_id)
    }
}

#[derive(Clone, Default, Debug)]
pub(super) struct InMemSentStorage {
    inner: Arc<Mutex<InMemStorageInner>>,
}

impl InMemSentStorage {
    pub(super) fn new() -> Self {
        Self::default()
    }
}

impl SentStorage for InMemSentStorage {
    fn store(&self, stream_id: Uuid, serial: u32, dpgs: Vec<DataPointGroup>) -> Result<()> {
        self.inner.lock().unwrap().save(stream_id, serial, dpgs)
    }
    fn remove(&self, stream_id: Uuid, serial: u32) -> Option<Vec<DataPointGroup>> {
        self.inner.lock().unwrap().remove(stream_id, serial)
    }
    fn remaining(&self, stream_id: Uuid) -> usize {
        self.inner.lock().unwrap().remaining(stream_id)
    }
}

#[derive(Clone, Default, Debug)]
pub(super) struct InMemStorageInner {
    buf: HashMap<Uuid, HashMap<u32, Vec<DataPointGroup>>>,
}

impl InMemStorageInner {
    fn save(&mut self, stream_id: Uuid, serial: u32, dpgs: Vec<DataPointGroup>) -> Result<()> {
        match self.buf.entry(stream_id) {
            Entry::Vacant(e) => e.insert(HashMap::new()).insert(serial, dpgs),
            Entry::Occupied(mut e) => e.get_mut().insert(serial, dpgs),
        };

        Ok(())
    }

    fn remove(&mut self, stream_id: Uuid, serial: u32) -> Option<Vec<DataPointGroup>> {
        match self.buf.entry(stream_id) {
            Entry::Vacant(_) => None,
            Entry::Occupied(mut e) => e.get_mut().remove(&serial),
        }
    }

    fn remaining(&mut self, stream_id: Uuid) -> usize {
        match self.buf.entry(stream_id) {
            Entry::Vacant(_) => 0,
            Entry::Occupied(e) => e.get().len(),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct InMemSentStorageNoPayload {
    inner: Arc<Mutex<HashMap<Uuid, InMemSentStorageNoPayloadInner>>>,
    timeout: std::time::Duration,
    cancel: Cancel,
}

impl InMemSentStorageNoPayload {
    pub(super) fn new(timeout: std::time::Duration) -> Self {
        let res = Self {
            inner: Arc::default(),
            cancel: Cancel::new(),
            timeout,
        };
        let mem = res.clone();
        tokio::task::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(timeout) => {},
                    _ = mem.cancel.notified() => break,
                }
                mem.inner
                    .lock()
                    .unwrap()
                    .iter_mut()
                    .for_each(|(_, mem)| mem.remove_expired());
            }
        });

        res
    }
}

impl Drop for InMemSentStorageNoPayload {
    fn drop(&mut self) {
        let _ = self.cancel.notify();
    }
}

impl Default for InMemSentStorageNoPayload {
    fn default() -> Self {
        Self::new(std::time::Duration::from_secs(10))
    }
}

impl SentStorage for InMemSentStorageNoPayload {
    fn store(&self, stream_id: Uuid, serial: u32, dpgs: Vec<DataPointGroup>) -> Result<()> {
        match self.inner.lock().unwrap().entry(stream_id) {
            Entry::Occupied(mut e) => e.get_mut().store(serial, dpgs),
            Entry::Vacant(e) => e
                .insert(InMemSentStorageNoPayloadInner::new(self.timeout))
                .store(serial, dpgs),
        }
    }

    fn remove(&self, stream_id: Uuid, serial: u32) -> Option<Vec<DataPointGroup>> {
        match self.inner.lock().unwrap().entry(stream_id) {
            Entry::Occupied(mut e) => e.get_mut().remove(serial),
            Entry::Vacant(_) => None,
        }
    }

    fn remaining(&self, stream_id: Uuid) -> usize {
        match self.inner.lock().unwrap().entry(stream_id) {
            Entry::Occupied(e) => e.get().remaining(),
            Entry::Vacant(_) => 0,
        }
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
struct DataPointId {
    pub id: DataId,
    pub elapsed_time: chrono::Duration,
}

#[derive(Clone, Debug)]
pub(super) struct InMemSentStorageNoPayloadInner {
    buf: HashMap<u32, Vec<DataPointId>>,
    expiries: HashMap<u32, std::time::SystemTime>,
    timeout: std::time::Duration,
}

impl Default for InMemSentStorageNoPayloadInner {
    fn default() -> Self {
        Self::new(std::time::Duration::from_secs(10))
    }
}

impl InMemSentStorageNoPayloadInner {
    pub(super) fn new(timeout: std::time::Duration) -> Self {
        Self {
            buf: HashMap::new(),
            expiries: HashMap::new(),
            timeout,
        }
    }

    fn store(&mut self, serial: u32, dpgs: Vec<DataPointGroup>) -> Result<()> {
        let entry = match self.buf.entry(serial) {
            Entry::Vacant(e) => e,
            Entry::Occupied(_) => return Err(Error::unexpected("recode already exist")),
        };

        let d = dpgs
            .into_iter()
            .map(|dpg| DataPointId {
                id: dpg.id,
                elapsed_time: dpg
                    .data_points
                    .get(0)
                    .map(|dp| dp.elapsed_time)
                    .unwrap_or_else(chrono::Duration::zero),
            })
            .collect::<Vec<_>>();
        entry.insert(d);

        self.expiries
            .insert(serial, std::time::SystemTime::now() + self.timeout);

        Ok(())
    }

    fn remove(&mut self, serial: u32) -> Option<Vec<DataPointGroup>> {
        self.expiries.remove(&serial);
        self.buf.remove(&serial).map(|v| {
            v.into_iter()
                .map(|id| DataPointGroup {
                    id: id.id,
                    data_points: vec![DataPoint {
                        elapsed_time: id.elapsed_time,
                        payload: Bytes::new(),
                    }],
                })
                .collect::<Vec<_>>()
        })
    }

    fn remove_expired(&mut self) {
        let now = std::time::SystemTime::now();
        self.expiries
            .iter()
            .filter(|(_, exp)| *exp <= &now)
            .map(|(s, _)| s)
            .cloned()
            .collect::<Vec<_>>()
            .iter()
            .for_each(|serial| {
                self.expiries.remove(serial);
                self.buf.remove(serial);
            });
    }

    fn remaining(&self) -> usize {
        self.buf.len()
    }
}

#[derive(Clone, Default, Debug)]
pub(super) struct InMemStreamRepository {
    up: Arc<Mutex<InMemStreamRepositoryInner<super::UpstreamInfo>>>,
    down: Arc<Mutex<InMemStreamRepositoryInner<super::DownstreamInfo>>>,
}

impl InMemStreamRepository {
    pub(super) fn new() -> Self {
        Self {
            up: Arc::default(),
            down: Arc::default(),
        }
    }
}

impl super::UpstreamRepository for InMemStreamRepository {
    fn save_upstream(&self, info: &super::UpstreamInfo) -> Result<()> {
        self.up.lock().unwrap().save(info.stream_id, info.clone())
    }
    fn find_upstream_by_id(&self, uuid: Uuid) -> Result<super::UpstreamInfo> {
        self.up.lock().unwrap().find(uuid)
    }
    fn remove_upstream_by_id(&self, uuid: Uuid) -> Result<()> {
        self.up.lock().unwrap().remove(uuid)
    }
}

impl super::DownstreamRepository for InMemStreamRepository {
    fn save_downstream(&self, info: &super::DownstreamInfo) -> Result<()> {
        self.down.lock().unwrap().save(info.stream_id, info.clone())
    }
    fn find_downstream_by_id(&self, uuid: Uuid) -> Result<super::DownstreamInfo> {
        self.down.lock().unwrap().find(uuid)
    }
    fn remove_downstream_by_id(&self, uuid: Uuid) -> Result<()> {
        self.down.lock().unwrap().remove(uuid)
    }
}

#[derive(Default, Debug)]
pub(super) struct InMemStreamRepositoryInner<T> {
    map: HashMap<Uuid, T>,
}

impl<T> InMemStreamRepositoryInner<T>
where
    T: Clone,
{
    fn save(&mut self, id: Uuid, info: T) -> Result<()> {
        self.map.insert(id, info);
        Ok(())
    }

    fn find(&mut self, id: Uuid) -> Result<T> {
        let e = match self.map.entry(id) {
            Entry::Occupied(e) => e,
            Entry::Vacant(_) => return Err(Error::StreamNotFound),
        };

        Ok(e.get().clone())
    }

    fn remove(&mut self, id: Uuid) -> Result<()> {
        self.map.remove(&id);
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn ut_remove_expired() {
        let stream_id = Uuid::new_v4();
        let m = InMemSentStorageNoPayload::new(std::time::Duration::from_millis(1));
        m.store(stream_id, 1, Vec::new()).unwrap();
        assert_eq!(1, m.remaining(stream_id));
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(0, m.remaining(stream_id));
    }
}
