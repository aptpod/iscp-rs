use crate::{DownstreamConfig, QoS};

/// 有効期限を設定します。
pub fn with_expiry_interval(
    expiry_interval: chrono::Duration,
) -> Box<impl Fn(&mut DownstreamConfig) + 'static> {
    Box::new(move |cfg: &mut DownstreamConfig| cfg.expiry_interval = expiry_interval)
}

/// QoSを設定します。
pub fn with_qos(qos: QoS) -> Box<impl Fn(&mut DownstreamConfig) + 'static> {
    Box::new(move |cfg: &mut DownstreamConfig| cfg.qos = qos)
}
