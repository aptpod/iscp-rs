use std::time::Duration;

/// アップストリームのフラッシュの方法について定義します。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FlushPolicy {
    /// データポイントが書き込まれる内部バッファをフラッシュしないポリシーです。
    None,
    /// データポイントが書き込まれる内部バッファを時間間隔でフラッシュするポリシーです。
    IntervalOnly { interval: Duration },
    /// データポイントが書き込まれる内部バッファを、指定したバッファサイズを超えた時にフラッシュするポリシーです。
    BufferSizeOnly { buffer_size: u64 },
    /// データポイントが書き込まれる内部バッファを、時間間隔または指定したバッファサイズのいずれかの条件を満たした時にフラッシュするポリシーです。
    IntervalOrBufferSize {
        interval: Duration,
        buffer_size: u64,
    },
    /// データポイントが内部バッファに書き込まれたタイミングで即時フラッシュするポリシーです。
    Immediately,
}

impl Default for FlushPolicy {
    fn default() -> Self {
        Self::IntervalOnly {
            interval: Duration::from_millis(10),
        }
    }
}

impl FlushPolicy {
    pub(super) fn interval(&self) -> Option<Duration> {
        match self {
            Self::IntervalOnly { interval } => Some(*interval),
            Self::IntervalOrBufferSize { interval, .. } => Some(*interval),
            _ => None,
        }
    }

    pub(super) fn buffer_size(&self) -> Option<u64> {
        match self {
            Self::BufferSizeOnly { buffer_size } => Some(*buffer_size),
            Self::IntervalOrBufferSize { buffer_size, .. } => Some(*buffer_size),
            _ => None,
        }
    }
}
