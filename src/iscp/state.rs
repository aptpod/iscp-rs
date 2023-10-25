use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Mutex,
    },
};

use tokio::sync::{broadcast, oneshot};

use crate::{msg, Error, Result};

pub(super) type CallId = String;

#[derive(Debug)]
pub(super) struct State {
    pub(super) close: AtomicBool,
    downstream_alias: AtomicU32,
    waiting_reply: Mutex<HashMap<CallId, oneshot::Sender<msg::DownstreamCall>>>,
    pub(super) tx_reply: broadcast::Sender<msg::DownstreamCall>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            close: AtomicBool::new(true),
            downstream_alias: AtomicU32::new(1),
            waiting_reply: Mutex::default(),
            tx_reply: broadcast::channel(1).0,
        }
    }
}

impl State {
    pub(super) fn new(close: AtomicBool) -> Self {
        State {
            close,
            ..Default::default()
        }
    }

    pub(super) fn next_downstream_alias(&self) -> u32 {
        // todo: overflow対策
        loop {
            let res = self.downstream_alias.fetch_add(1, Ordering::Release);
            if res != 0 {
                return res;
            }
        }
    }
    pub(super) fn check_open(&self) -> Result<()> {
        if self.close.load(Ordering::Acquire) {
            return Err(Error::ConnectionClosed("".into()));
        }

        Ok(())
    }
    pub(super) fn take_waiting_reply(
        &self,
        call_id: &CallId,
    ) -> Option<oneshot::Sender<msg::DownstreamCall>> {
        let mut waiting = self.waiting_reply.lock().unwrap();
        waiting.remove(call_id)
    }

    pub(super) fn register_waiting_reply_with_call_id(
        &self,
        call_id: CallId,
        sender: oneshot::Sender<msg::DownstreamCall>,
    ) -> bool {
        let mut waiting = self.waiting_reply.lock().unwrap();
        if waiting.get(&call_id).is_some() {
            return false;
        }
        waiting.insert(call_id, sender).is_none()
    }
}

/// コールIDを作成します。
pub(super) fn new_call_id() -> CallId {
    uuid::Uuid::new_v4().hyphenated().to_string()
}

/// send_call_and_wait_reply_call()で使用するためのコールIDを作成します。
pub(super) fn new_internal_call_id() -> CallId {
    uuid::Uuid::new_v4().simple().to_string()
}
