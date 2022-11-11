use std::sync::Arc;

use crate::{DataId, FlushPolicy, QoS, ReceiveAckHooker, SendDataPointsHooker, UpstreamConfig};

/// Ackの返却間隔を設定します。
pub fn with_ack_interval(
    ack_interval: chrono::Duration,
) -> Box<impl Fn(&mut UpstreamConfig) + 'static> {
    Box::new(move |cfg: &mut UpstreamConfig| cfg.ack_interval = ack_interval)
}

/// 有効期限を設定します。
pub fn with_expiry_interval(
    expiry_interval: chrono::Duration,
) -> Box<impl Fn(&mut UpstreamConfig) + 'static> {
    Box::new(move |cfg: &mut UpstreamConfig| cfg.expiry_interval = expiry_interval)
}

/// データIDリストを設定します。
pub fn with_data_ids(data_ids: Vec<DataId>) -> Box<impl Fn(&mut UpstreamConfig) + 'static> {
    Box::new(move |cfg: &mut UpstreamConfig| cfg.data_ids = data_ids.clone())
}

/// QoSを設定します。
pub fn with_qos(qos: QoS) -> Box<impl Fn(&mut UpstreamConfig) + 'static> {
    Box::new(move |cfg: &mut UpstreamConfig| cfg.qos = qos)
}

/// 永続化するかどうかを設定します。
pub fn with_persist(persist: Option<bool>) -> Box<impl Fn(&mut UpstreamConfig) + 'static> {
    Box::new(move |cfg: &mut UpstreamConfig| cfg.persist = persist)
}

/// フラッシュポリシーを設定します。
pub fn with_flush_policy(flush_policy: FlushPolicy) -> Box<impl Fn(&mut UpstreamConfig) + 'static> {
    Box::new(move |cfg: &mut UpstreamConfig| cfg.flush_policy = flush_policy)
}

/// Ack受信時のフックを設定します。
pub fn with_recv_ack_hooker(
    recv_ack_hooker: Arc<dyn ReceiveAckHooker>,
) -> Box<impl Fn(&mut UpstreamConfig) + 'static> {
    Box::new(move |cfg: &mut UpstreamConfig| cfg.recv_ack_hooker = recv_ack_hooker.clone())
}

/// データポイント送信時のフックを設定します。
pub fn with_send_data_points_hooker(
    send_data_points_hooker: Arc<dyn SendDataPointsHooker>,
) -> Box<impl Fn(&mut UpstreamConfig) + 'static> {
    Box::new(move |cfg: &mut UpstreamConfig| {
        cfg.send_data_points_hooker = send_data_points_hooker.clone()
    })
}
