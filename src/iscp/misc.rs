use std::{
    future::Future,
    pin::Pin,
    time::{Duration, SystemTime},
};

use rand::Rng;
use uuid::Uuid;

use crate::{Error, QoS};

pub fn parse_stream_id(bytes: &[u8]) -> Result<Uuid, Error> {
    match Uuid::from_slice(bytes) {
        Ok(stream_id) => Ok(stream_id),
        Err(e) => Err(Error::invalid_value(format!(
            "invalid bytes for stream id: {}",
            e,
        ))),
    }
}

pub fn unix_epoch_to_system_time(nsecs: i64) -> Result<SystemTime, Error> {
    let d = Duration::from_nanos(
        nsecs
            .try_into()
            .map_err(|_| Error::invalid_value("server time overflow"))?,
    );
    SystemTime::UNIX_EPOCH
        .checked_add(d)
        .ok_or_else(|| Error::invalid_value("server time overflow"))
}

pub fn from_qos_i32(qos: i32) -> Result<QoS, Error> {
    QoS::try_from(qos).map_err(|e| Error::invalid_value(e.to_string()))
}

/// Return value of callbacks.
pub struct CallbackReturnValue {
    future: Option<Pin<Box<dyn Future<Output = ()> + Send + 'static>>>,
}

impl CallbackReturnValue {
    pub fn complete() -> Self {
        Self { future: None }
    }

    pub fn from_task<F: Future<Output = ()> + Send + 'static>(f: F) -> Self {
        Self {
            future: Some(Box::pin(f)),
        }
    }
}

impl std::future::Future for CallbackReturnValue {
    type Output = ();
    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        if let Some(future) = &mut self.as_mut().future {
            future.as_mut().poll(cx)
        } else {
            std::task::Poll::Ready(())
        }
    }
}

pub struct ReconnectWaiter {
    max: f64,
    current: f64,
}

impl ReconnectWaiter {
    pub fn new() -> Self {
        Self {
            max: 5000.0,
            current: 50.0,
        }
    }

    pub async fn wait(&mut self) {
        self.current *= 2.0;
        if self.current > self.max {
            self.current = self.max;
        }
        let ms = self.current * rand::thread_rng().gen_range(0.5..1.5);
        let duration = Duration::from_millis(ms as u64);
        tokio::time::sleep(duration).await
    }
}
