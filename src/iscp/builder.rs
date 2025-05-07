use std::time::Duration;

use super::downstream::*;
use super::upstream::*;
use super::*;

/// [Upstream]のビルダー型
pub struct UpstreamBuilder {
    pub(crate) conn: Conn,
    pub(crate) config: UpstreamConfig,
}

impl UpstreamBuilder {
    /// Ack間隔を設定
    pub fn ack_interval(&mut self, ack_interval: Duration) -> &mut Self {
        self.config.ack_interval = ack_interval;
        self
    }

    /// 再開の有効期限を設定
    pub fn expiry_interval(&mut self, expiry_interval: Duration) -> &mut Self {
        self.config.expiry_interval = expiry_interval;
        self
    }

    /// データIDのリストを設定
    pub fn data_ids(&mut self, data_ids: Vec<DataId>) -> &mut Self {
        self.config.data_ids = data_ids;
        self
    }

    /// QoSを設定
    pub fn qos(&mut self, qos: QoS) -> &mut Self {
        self.config.qos = qos;
        self
    }

    /// 永続化を設定
    pub fn persist(&mut self, persist: bool) -> &mut Self {
        self.config.persist = persist;
        self
    }

    /// フラッシュポリシーを設定
    pub fn flush_policy(&mut self, flush_policy: FlushPolicy) -> &mut Self {
        self.config.flush_policy = flush_policy;
        self
    }

    /// クローズのタイムアウトを設定
    pub fn close_timeout(&mut self, close_timeout: Duration) -> &mut Self {
        self.config.close_timeout = close_timeout;
        self
    }

    /// データポイントをフラッシュ時に呼ばれるコールバックを設定
    pub fn send_data_points_callback<T: Into<Option<SendDataPointsCallback>>>(
        &mut self,
        callback: T,
    ) -> &mut Self {
        self.config.send_data_points_callback = callback.into();
        self
    }

    /// Ackの受信のコールバックを設定
    pub fn receive_ack_callback<T: Into<Option<ReceiveAckCallback>>>(
        &mut self,
        callback: T,
    ) -> &mut Self {
        self.config.receive_ack_callback = callback.into();
        self
    }

    /// アップストリームの再開のコールバックを設定
    pub fn resumed_callback<T: Into<Option<UpstreamResumedCallback>>>(
        &mut self,
        callback: T,
    ) -> &mut Self {
        self.config.resumed_callback = callback.into();
        self
    }

    /// アップストリームを作成
    pub async fn build(&self) -> Result<Upstream, Error> {
        self.conn
            .open_upstream_with_config(Arc::new(self.config.clone()))
            .await
    }
}

/// [Downstream]のビルダー型
pub struct DownstreamBuilder {
    pub(crate) conn: Conn,
    pub(crate) config: DownstreamConfig,
}

impl DownstreamBuilder {
    /// 再開の有効期限を設定
    pub fn expiry_interval(&mut self, expiry_interval: Duration) -> &mut Self {
        self.config.expiry_interval = expiry_interval;
        self
    }

    /// データIDのリストを設定
    pub fn data_ids(&mut self, data_ids: Vec<DataId>) -> &mut Self {
        self.config.data_ids = data_ids;
        self
    }

    /// QoSを設定
    pub fn qos(&mut self, qos: QoS) -> &mut Self {
        self.config.qos = qos;
        self
    }

    /// Ack間隔を設定    
    pub fn ack_interval(&mut self, ack_interval: Duration) -> &mut Self {
        self.config.ack_interval = ack_interval;
        self
    }

    /// 空のチャンクを捨てるかの設定
    pub fn omit_empty_chunk(&mut self, omit_empty_chunk: bool) -> &mut Self {
        self.config.omit_empty_chunk = omit_empty_chunk;
        self
    }

    /// Reliable用のチャンク並び替えの設定
    pub fn reordering(&mut self, reordering: DownstreamReordering) -> &mut Self {
        self.config.reordering = reordering;
        self
    }

    /// 並び替えを待つチャンクの最大値
    pub fn reordering_chunks(&mut self, reordering_chunks: usize) -> &mut Self {
        self.config.reordering_chunks = reordering_chunks;
        self
    }

    /// ダウンストリームを作成
    pub async fn build(&self) -> Result<(Downstream, DownstreamMetadataReader), Error> {
        self.conn
            .open_downstream_with_config(Arc::new(self.config.clone()))
            .await
    }
}
