use std::time::Duration;

/// Flush policy for iSCP upstream.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FlushPolicy {
    None,
    IntervalOnly {
        interval: Duration,
    },
    BufferSizeOnly {
        buffer_size: u64,
    },
    IntervalOrBufferSize {
        interval: Duration,
        buffer_size: u64,
    },
    Immediately,
}

impl Default for FlushPolicy {
    fn default() -> Self {
        Self::IntervalOnly {
            interval: Duration::from_millis(100),
        }
    }
}

impl FlushPolicy {
    pub fn interval(&self) -> Option<Duration> {
        match self {
            Self::IntervalOnly { interval } => Some(*interval),
            Self::IntervalOrBufferSize { interval, .. } => Some(*interval),
            _ => None,
        }
    }

    pub fn buffer_size(&self) -> Option<u64> {
        match self {
            Self::BufferSizeOnly { buffer_size } => Some(*buffer_size),
            Self::IntervalOrBufferSize { buffer_size, .. } => Some(*buffer_size),
            _ => None,
        }
    }

    pub(crate) async fn sleep_by_interval(&self) {
        if let Some(interval) = self.interval() {
            tokio::time::sleep(interval).await;
        } else {
            std::future::pending().await
        }
    }

    pub(crate) fn need_flush(&self, current_buffer_size: usize) -> bool {
        match self {
            Self::Immediately => true,
            Self::BufferSizeOnly { buffer_size } => (*buffer_size as usize) <= current_buffer_size,
            Self::IntervalOrBufferSize { buffer_size, .. } => {
                (*buffer_size as usize) <= current_buffer_size
            }
            _ => false,
        }
    }
}
