use std::{collections::HashMap, hash::BuildHasher};

use crossbeam::atomic::AtomicCell;
use tokio::sync::{broadcast, oneshot};

macro_rules! check_result {
    ($level:ident, $e:expr, $msg:expr) => {
        if $e.is_err() {
            log::$level!("{}", $msg);
        }
    };
}

macro_rules! cancelled_return {
    ($ct:expr, $e:expr) => {
        tokio::select! {
            result = $e => result,
            _ = $ct.cancelled() => {
                return;
            }
        }
    };
}

macro_rules! cancelled_return_err {
    ($ct:expr, $e:expr) => {
        tokio::select! {
            result = $e => result,
            _ = $ct.cancelled() => {
                return Err(crate::error::Error::CancelledByClose);
            }
        }
    };
}

pub async fn timeout_with_ct<F, R>(
    ct: &tokio_util::sync::CancellationToken,
    duration: std::time::Duration,
    f: F,
) -> Result<R, crate::error::Error>
where
    F: std::future::Future<Output = R>,
{
    tokio::select! {
        result = tokio::time::timeout(duration, f) => {
            match result {
                Ok(result) => Ok(result),
                Err(_) => Err(crate::error::Error::timeout("")),
            }
        }
        _ = ct.cancelled() => {
            Err(crate::error::Error::CancelledByClose)
        }
    }
}

#[derive(Debug)]
pub struct Waiter(broadcast::Receiver<()>);

impl Waiter {
    pub fn new() -> (Waiter, WaitGroup) {
        let (sender, r) = broadcast::channel(1);
        (Waiter(r), WaitGroup { _sender: sender })
    }

    pub async fn wait(&self) {
        let mut receiver = self.0.resubscribe();
        let _ = receiver.recv().await;
    }
}

#[derive(Clone, Debug)]
pub struct WaitGroup {
    _sender: broadcast::Sender<()>,
}

pub fn may_send_err<R, E: std::error::Error, T: std::fmt::Display>(
    tx: &AtomicCell<Option<oneshot::Sender<Result<R, E>>>>,
    err_msg: T,
    result: Result<R, E>,
) -> bool {
    if let Err(e) = result {
        log::error!("{}: {}", err_msg, e);
        if let Some(tx) = tx.take() {
            let _ = tx.send(Err(e));
        }
        return true;
    }
    if let Some(tx) = tx.take() {
        let _ = tx.send(result);
    }
    false
}

pub fn conv_hasher<K: Eq + std::hash::Hash, V, S0: BuildHasher, S1: BuildHasher + Default>(
    map: HashMap<K, V, S0>,
) -> HashMap<K, V, S1> {
    map.into_iter().collect()
}
