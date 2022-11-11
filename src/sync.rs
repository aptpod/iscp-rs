use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use tokio::sync::{broadcast, mpsc};

use crate::error::{Error, Result};

#[derive(Clone, Debug)]
pub(crate) struct Cancel {
    called: Arc<AtomicBool>,
    notify: broadcast::Sender<()>,
}

impl Default for Cancel {
    fn default() -> Self {
        Self::new()
    }
}

impl Cancel {
    pub fn new() -> Self {
        let (notify, _) = broadcast::channel(1);
        Self {
            called: Arc::new(AtomicBool::new(false)),
            notify,
        }
    }
}

impl Cancel {
    pub fn is_called(&self) -> bool {
        self.called.load(Ordering::Acquire)
    }

    pub fn notify(&self) -> Result<()> {
        if self.is_called() {
            return Ok(());
        }
        self.called.store(true, Ordering::Release);
        self.notify.send(()).map_err(Error::unexpected)?;
        Ok(())
    }

    pub async fn notified(&self) -> Result<()> {
        if self.is_called() {
            return Ok(());
        }
        let mut sub = self.notify.subscribe();
        sub.recv().await.map_err(Error::unexpected)?;
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct Waiter(mpsc::Receiver<()>);

impl Waiter {
    pub async fn wait(&mut self) {
        let _ = self.0.recv().await;
    }
}

#[derive(Clone, Debug)]
pub(crate) struct WaitGroup(mpsc::Sender<()>);

impl WaitGroup {
    pub(crate) fn new() -> (Waiter, WaitGroup) {
        let (s, r) = mpsc::channel(1);
        (Waiter(r), WaitGroup(s))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[tokio::test]
    async fn cancel() {
        let cancel = Cancel::new();
        let cancel_c = cancel.clone();
        let testee = tokio::spawn(async move {
            while !cancel_c.is_called() {
                let _res = cancel_c.notified().await;
            }
        });

        let _ = cancel.notify();
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(10)) => {
                panic!("timeout");
            }
            _ = testee => {}
        }
    }
    #[tokio::test]
    async fn cancel_from_background() {
        let cancel = Cancel::new();
        let cancel_c = cancel.clone();

        let _testee = tokio::spawn(async move {
            let _ = cancel_c.notify();
        });

        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(10)) => {
                panic!("timeout");
            }
            _ = cancel.notified() => {}
        }
    }

    #[tokio::test]
    async fn wait_group() {
        let cancel = Cancel::new();
        let (mut waiter, wg) = WaitGroup::new();

        let cancel_c = cancel.clone();
        tokio::spawn(async move {
            let _wg = wg;
            let _ = cancel_c.notified().await;
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        });
        let _ = cancel.notify();
        let before = chrono::Utc::now();
        waiter.wait().await;
        let d = chrono::Utc::now() - before;
        if d < chrono::Duration::seconds(1) {
            panic!("error")
        }
    }
}
