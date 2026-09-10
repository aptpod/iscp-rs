use super::{Conn, types::*};
use crate::{ConnectionState, Error};

impl Conn {
    /// E2Eコールを送信し、コールIDを返却
    pub async fn send_call(&self, call: UpstreamCall) -> Result<String, Error> {
        loop {
            self.wait_for_state(ConnectionState::Connected).await?;
            match self.try_send_call(call.clone()).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if !e.can_retry() {
                        return Err(e);
                    }
                    log::warn!("e2e call failed and retry: {e}");
                }
            }
        }
    }

    /// リプライコールを送信し、コールIDを返却
    pub async fn send_reply_call(&self, call: UpstreamReplyCall) -> Result<String, Error> {
        loop {
            self.wait_for_state(ConnectionState::Connected).await?;
            match self.try_send_reply_call(call.clone()).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if !e.can_retry() {
                        return Err(e);
                    }
                    log::warn!("e2e call failed and retry: {e}");
                }
            }
        }
    }

    /// E2Eコールを送信し、それに対応するリプライコールを受信
    ///
    /// このメソッドはリプライコールを受信できるまで処理をブロックします。
    pub async fn send_call_and_wait_reply_call(
        &self,
        call: UpstreamCall,
    ) -> Result<DownstreamReplyCall, Error> {
        loop {
            self.wait_for_state(ConnectionState::Connected).await?;
            match self.try_send_call_and_wait_reply_call(call.clone()).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if !e.can_retry() {
                        return Err(e);
                    }
                    log::warn!("e2e call failed and retry: {e}");
                }
            }
        }
    }

    /// リプライコールでないコールを受信
    pub async fn recv_call(&self) -> Result<DownstreamCall, Error> {
        loop {
            self.wait_for_state(ConnectionState::Connected).await?;
            match self.try_recv_call().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if !e.can_retry() {
                        return Err(e);
                    }
                    log::warn!("e2e call failed and retry: {e}");
                }
            }
        }
    }

    /// リプライコールを受信
    pub async fn recv_reply_call(&self) -> Result<DownstreamReplyCall, Error> {
        loop {
            self.wait_for_state(ConnectionState::Connected).await?;
            match self.try_recv_reply_call().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if !e.can_retry() {
                        return Err(e);
                    }
                    log::warn!("e2e call failed and retry: {e}");
                }
            }
        }
    }

    async fn try_send_call(&self, call: UpstreamCall) -> Result<String, Error> {
        let call_id = new_call_id();

        let call = crate::message::UpstreamCall {
            call_id: call_id.clone(),
            request_call_id: "".into(),
            destination_node_id: call.destination_node_id,
            name: call.name,
            type_: call.type_,
            payload: call.payload,
            ..Default::default()
        };

        cancelled_return_err!(&self.inner.ct, self.send_call_inner(call))
    }

    async fn try_send_reply_call(&self, call: UpstreamReplyCall) -> Result<String, Error> {
        let call_id = new_call_id();

        let call = crate::message::UpstreamCall {
            call_id: call_id.clone(),
            request_call_id: call.request_call_id,
            destination_node_id: call.destination_node_id,
            name: call.name,
            type_: call.type_,
            payload: call.payload,
            ..Default::default()
        };

        cancelled_return_err!(&self.inner.ct, self.send_call_inner(call))
    }

    async fn send_call_inner(&self, call: crate::message::UpstreamCall) -> Result<String, Error> {
        let wire_conn = self.inner.wire_conn()?;
        let call_id = call.call_id.clone();

        let (rx, _guard) = wire_conn.subscribe_call_ack(call_id.clone()).await?;
        wire_conn.send_message(call).await?;
        let ack = rx.await.map_err(|_| Error::ConnectionClosed)?;
        debug_assert_eq!(ack.call_id, call_id);

        Ok(call_id)
    }

    async fn try_send_call_and_wait_reply_call(
        &self,
        call: UpstreamCall,
    ) -> Result<DownstreamReplyCall, Error> {
        cancelled_return_err!(
            &self.inner.ct,
            self.send_call_and_wait_reply_call_inner(call)
        )
    }

    async fn send_call_and_wait_reply_call_inner(
        &self,
        call: UpstreamCall,
    ) -> Result<DownstreamReplyCall, Error> {
        let call_id = new_call_id();
        let call = crate::message::UpstreamCall {
            call_id: call_id.clone(),
            request_call_id: "".into(),
            destination_node_id: call.destination_node_id,
            name: call.name,
            type_: call.type_,
            payload: call.payload,
            ..Default::default()
        };
        let wire_conn = self.inner.wire_conn()?;

        let (rx_ack, _guard) = wire_conn.subscribe_call_ack(call_id.clone()).await?;
        let mut rx_downstream_call = wire_conn.subscribe_downstream_call();
        wire_conn.send_message(call).await?;
        let ack = rx_ack.await.map_err(|_| Error::ConnectionClosed)?;
        debug_assert_eq!(ack.call_id, call_id);

        // Bound the wait so a lost reply cannot hang forever.
        let call = wait_matching_reply(
            &mut rx_downstream_call,
            &call_id,
            self.inner.e2e_call_reply_timeout,
        )
        .await?;

        Ok(DownstreamReplyCall {
            request_call_id: call.request_call_id,
            source_node_id: call.source_node_id,
            type_: call.type_,
            name: call.name,
            payload: call.payload,
        })
    }

    async fn try_recv_call(&self) -> Result<DownstreamCall, Error> {
        loop {
            let call = cancelled_return_err!(&self.inner.ct, self.recv_call_inner())?;
            if !call.request_call_id.is_empty() {
                continue;
            }

            return Ok(DownstreamCall {
                call_id: call.call_id,
                source_node_id: call.source_node_id,
                name: call.name,
                type_: call.type_,
                payload: call.payload,
            });
        }
    }

    async fn try_recv_reply_call(&self) -> Result<DownstreamReplyCall, Error> {
        loop {
            let call = cancelled_return_err!(&self.inner.ct, self.recv_call_inner())?;
            if call.request_call_id.is_empty() {
                continue;
            }

            return Ok(DownstreamReplyCall {
                request_call_id: call.request_call_id,
                source_node_id: call.source_node_id,
                name: call.name,
                type_: call.type_,
                payload: call.payload,
            });
        }
    }

    async fn recv_call_inner(&self) -> Result<crate::message::DownstreamCall, Error> {
        let wire_conn = self.inner.wire_conn()?;
        let mut rx_downstream_call = wire_conn.subscribe_downstream_call();
        rx_downstream_call
            .recv()
            .await
            .map_err(|_| Error::ConnectionClosed)
    }
}

pub(super) fn new_call_id() -> String {
    uuid::Uuid::new_v4().hyphenated().to_string()
}

/// Wait for the reply call matching `call_id`, bounded by `reply_timeout`.
/// Returns [`Error::Timeout`] on deadline, [`Error::ConnectionClosed`] if the
/// receiver closes.
async fn wait_matching_reply(
    rx_downstream_call: &mut crate::wire::DownstreamCallReceiver,
    call_id: &str,
    reply_timeout: std::time::Duration,
) -> Result<crate::message::DownstreamCall, Error> {
    let wait_reply = async {
        loop {
            let Ok(downstream_call) = rx_downstream_call.recv().await else {
                return Err(Error::ConnectionClosed);
            };

            if downstream_call.request_call_id == call_id {
                return Ok(downstream_call);
            }
        }
    };
    match tokio::time::timeout(reply_timeout, wait_reply).await {
        Ok(result) => result,
        Err(_) => Err(Error::timeout("wait reply call")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::DownstreamCall;
    use std::time::Duration;
    use tokio::sync::broadcast;

    fn make_receiver(
        capacity: usize,
    ) -> (
        broadcast::Sender<DownstreamCall>,
        crate::wire::DownstreamCallReceiver,
    ) {
        let (tx, rx) = broadcast::channel(capacity);
        (tx, crate::wire::DownstreamCallReceiver::new(rx))
    }

    // A missing reply must time out, not hang. The 10s outer guard only fires if
    // the wait regresses to unbounded.
    #[tokio::test]
    async fn test_wait_matching_reply_times_out_when_reply_missing() {
        let (_tx, mut rx) = make_receiver(1024);

        let result = tokio::time::timeout(
            Duration::from_secs(10),
            wait_matching_reply(&mut rx, "missing-call-id", Duration::from_millis(50)),
        )
        .await
        .expect("wait_matching_reply must return, not hang");

        assert!(
            matches!(result, Err(Error::Timeout(_))),
            "expected Error::Timeout, got {result:?}"
        );
    }

    // A non-matching reply is skipped; the matching one is returned within the deadline.
    #[tokio::test]
    async fn test_wait_matching_reply_returns_matching_reply() {
        let (tx, mut rx) = make_receiver(1024);

        tx.send(DownstreamCall {
            request_call_id: "other".into(),
            ..Default::default()
        })
        .unwrap();
        tx.send(DownstreamCall {
            request_call_id: "wanted".into(),
            payload: b"ok".to_vec().into(),
            ..Default::default()
        })
        .unwrap();

        let call = wait_matching_reply(&mut rx, "wanted", Duration::from_secs(60))
            .await
            .expect("matching reply must be returned");
        assert_eq!(call.request_call_id, "wanted");
        assert_eq!(&call.payload[..], b"ok");
    }

    // A closed channel surfaces `Error::ConnectionClosed` (retryable), not a hang.
    #[tokio::test]
    async fn test_wait_matching_reply_connection_closed() {
        let (tx, mut rx) = make_receiver(1024);
        std::mem::drop(tx);

        let result = wait_matching_reply(&mut rx, "any", Duration::from_secs(60)).await;
        assert!(
            matches!(result, Err(Error::ConnectionClosed)),
            "expected Error::ConnectionClosed, got {result:?}"
        );
    }
}
