use std::time::Duration;

use super::downstream::*;
use super::upstream::*;
use super::*;

/// Builder type for [Upstream].
pub struct UpstreamBuilder {
    pub(crate) conn: Conn,
    pub(crate) config: UpstreamConfig,
}

impl UpstreamBuilder {
    pub fn ack_interval(&mut self, ack_interval: Duration) -> &mut Self {
        self.config.ack_interval = ack_interval;
        self
    }

    pub fn expiry_interval(&mut self, expiry_interval: Duration) -> &mut Self {
        self.config.expiry_interval = expiry_interval;
        self
    }

    pub fn data_ids(&mut self, data_ids: Vec<DataId>) -> &mut Self {
        self.config.data_ids = data_ids;
        self
    }

    pub fn qos(&mut self, qos: QoS) -> &mut Self {
        self.config.qos = qos;
        self
    }

    pub fn persist(&mut self, persist: bool) -> &mut Self {
        self.config.persist = persist;
        self
    }

    pub fn flush_policy(&mut self, flush_policy: FlushPolicy) -> &mut Self {
        self.config.flush_policy = flush_policy;
        self
    }

    pub fn close_timeout(&mut self, close_timeout: Duration) -> &mut Self {
        self.config.close_timeout = close_timeout;
        self
    }

    pub fn send_data_points_callback<T: Into<Option<SendDataPointsCallback>>>(
        &mut self,
        callback: T,
    ) -> &mut Self {
        self.config.send_data_points_callback = callback.into();
        self
    }

    pub fn receive_ack_callback<T: Into<Option<ReceiveAckCallback>>>(
        &mut self,
        callback: T,
    ) -> &mut Self {
        self.config.receive_ack_callback = callback.into();
        self
    }

    pub fn resumed_callback<T: Into<Option<UpstreamResumedCallback>>>(
        &mut self,
        callback: T,
    ) -> &mut Self {
        self.config.resumed_callback = callback.into();
        self
    }

    pub async fn build(&self) -> Result<Upstream, Error> {
        self.conn
            .open_upstream_with_config(Arc::new(self.config.clone()))
            .await
    }
}

/// Builder type for [Downstream].
pub struct DownstreamBuilder {
    pub(crate) conn: Conn,
    pub(crate) config: DownstreamConfig,
}

impl DownstreamBuilder {
    pub fn expiry_interval(&mut self, expiry_interval: Duration) -> &mut Self {
        self.config.expiry_interval = expiry_interval;
        self
    }

    pub fn data_ids(&mut self, data_ids: Vec<DataId>) -> &mut Self {
        self.config.data_ids = data_ids;
        self
    }

    pub fn qos(&mut self, qos: QoS) -> &mut Self {
        self.config.qos = qos;
        self
    }

    pub fn ack_interval(&mut self, ack_interval: Duration) -> &mut Self {
        self.config.ack_interval = ack_interval;
        self
    }

    pub fn omit_empty_chunk(&mut self, omit_empty_chunk: bool) -> &mut Self {
        self.config.omit_empty_chunk = omit_empty_chunk;
        self
    }

    pub fn reordering(&mut self, reordering: DownstreamReordering) -> &mut Self {
        self.config.reordering = reordering;
        self
    }

    pub fn reordering_chunks(&mut self, reordering_chunks: usize) -> &mut Self {
        self.config.reordering_chunks = reordering_chunks;
        self
    }

    pub async fn build(&self) -> Result<(Downstream, DownstreamMetadataReader), Error> {
        self.conn
            .open_downstream_with_config(Arc::new(self.config.clone()))
            .await
    }
}
