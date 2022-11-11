//! フックに関するモジュールです。

use std::sync::Arc;

use crate::{Ack, DataPoint};

/// Ack受信時のフックのインターフェースです。
pub trait ReceiveAckHooker: Sync + Send {
    /// Ackを受信した直後に呼び出されます。
    fn hook_after_recv_ack(&self, sid: uuid::Uuid, sequence: u32, ack: Ack);
}

impl<R: ReceiveAckHooker + ?Sized> ReceiveAckHooker for Box<R> {
    fn hook_after_recv_ack(&self, sid: uuid::Uuid, sequence: u32, ack: Ack) {
        (**self).hook_after_recv_ack(sid, sequence, ack)
    }
}

impl<R: ReceiveAckHooker + ?Sized> ReceiveAckHooker for Arc<R> {
    fn hook_after_recv_ack(&self, sid: uuid::Uuid, sequence: u32, ack: Ack) {
        (**self).hook_after_recv_ack(sid, sequence, ack)
    }
}

/// Ack受信時のフックで何も実行しないフッカーです。
pub struct NopReceiveAckHooker;

impl ReceiveAckHooker for NopReceiveAckHooker {
    fn hook_after_recv_ack(&self, _sid: uuid::Uuid, _sequence: u32, _ack: Ack) {}
}

/// データ送信時のフックのインターフェースです。
pub trait SendDataPointsHooker: Sync + Send {
    /// データを送信する直前に呼び出されます。
    fn hook_before_send_data_points(&self, sid: uuid::Uuid, sequence: u32, dps: Vec<DataPoint>);
}

impl<F: SendDataPointsHooker + ?Sized> SendDataPointsHooker for Box<F> {
    fn hook_before_send_data_points(&self, sid: uuid::Uuid, sequence: u32, dps: Vec<DataPoint>) {
        (**self).hook_before_send_data_points(sid, sequence, dps)
    }
}

impl<F: SendDataPointsHooker + ?Sized> SendDataPointsHooker for Arc<F> {
    fn hook_before_send_data_points(&self, sid: uuid::Uuid, sequence: u32, dps: Vec<DataPoint>) {
        (**self).hook_before_send_data_points(sid, sequence, dps)
    }
}

/// データ送信時のフックで何も実行しないフッカーです。
pub struct NopSendDataPointsHooker;
impl SendDataPointsHooker for NopSendDataPointsHooker {
    fn hook_before_send_data_points(&self, _sid: uuid::Uuid, _sequence: u32, _dps: Vec<DataPoint>) {
    }
}
