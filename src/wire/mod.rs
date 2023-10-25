//! iSCP のワイヤレベルのプロトコルを定義するモジュールです。

use crate::{error::Result, message as msg};
mod connection;
mod connector;
mod transport;

pub use connector::new_connector;

use async_trait::async_trait;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::sync::mpsc;

#[cfg(test)]
use mockall::{automock, mock, predicate::*};

/// iSCP のトランスポート層を抽象化したインターフェースです。
#[cfg_attr(test, automock)]
#[async_trait]
pub trait Transport: Send + Sync {
    /// メッセージを読み出します。
    async fn read_message(&self) -> Result<msg::Message>;
    /// メッセージを書き込みます。
    async fn write_message(&self, msg: msg::Message) -> Result<()>;
    /// コネクションを切断します。
    async fn close(&self) -> Result<()>;
}

#[cfg(test)]
mock! {
    pub MockTransport{}
    #[async_trait]
    impl Transport for MockTransport {
        async fn read_message(&self) -> Result<msg::Message>;
        async fn write_message(&self, msg: msg::Message) -> Result<()>;
        async fn close(&self) -> Result<()>;
    }
}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait UnreliableTransport: Send + Sync {
    /// 信頼性のないトランスポートからメッセージを読み出します。
    async fn unreliable_read_message(&self) -> Result<msg::Message>;
    /// 信頼性のないトランスポートへメッセージを書き込みます。
    async fn unreliable_write_message(&self, msg: msg::Message) -> Result<()>;
}

pub type BoxedUnreliableTransport = Box<dyn UnreliableTransport>;

#[async_trait]
impl<U: UnreliableTransport> UnreliableTransport for Box<U> {
    async fn unreliable_read_message(&self) -> Result<msg::Message> {
        (**self).unreliable_read_message().await
    }
    async fn unreliable_write_message(&self, msg: msg::Message) -> Result<()> {
        (**self).unreliable_write_message(msg).await
    }
}

#[cfg(test)]
mock! {
    pub MockUnreliableTransport{}
    #[async_trait]
    impl UnreliableTransport for MockUnreliableTransport {
        async fn unreliable_read_message(&self) -> Result<msg::Message>;
        async fn unreliable_write_message(&self, msg: msg::Message) -> Result<()>;
    }
}

#[derive(Debug)]
pub struct DisconnectNotificationReceiver(broadcast::Receiver<()>);
impl DisconnectNotificationReceiver {
    pub async fn recv(&mut self) {
        let _ = self.0.recv().await; // ignore error
    }
    pub(crate) fn new(r: broadcast::Receiver<()>) -> Self {
        Self(r)
    }
}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait Connection: Send + Sync {
    fn subscribe_disconnect_notify(&self) -> DisconnectNotificationReceiver;
    async fn close(&self) -> Result<()>;
    async fn open_request(&self, msg: msg::ConnectRequest) -> Result<msg::ConnectResponse>;
    async fn disconnect(&self, msg: msg::Disconnect) -> Result<()>;

    async fn upstream_open_request(
        &self,
        msg: msg::UpstreamOpenRequest,
    ) -> Result<msg::UpstreamOpenResponse>;
    async fn upstream_resume_request(
        &self,
        msg: msg::UpstreamResumeRequest,
        qos: msg::QoS,
    ) -> Result<msg::UpstreamResumeResponse>;
    async fn upstream_metadata(
        &self,
        msg: msg::UpstreamMetadata,
    ) -> Result<msg::UpstreamMetadataAck>;
    async fn upstream_chunk(&self, msg: msg::UpstreamChunk) -> Result<()>;
    async fn upstream_close_request(
        &self,
        msg: msg::UpstreamCloseRequest,
    ) -> Result<msg::UpstreamCloseResponse>;

    async fn downstream_open_request(
        &self,
        msg: msg::DownstreamOpenRequest,
    ) -> Result<msg::DownstreamOpenResponse>;
    async fn downstream_resume_request(
        &self,
        msg: msg::DownstreamResumeRequest,
    ) -> Result<msg::DownstreamResumeResponse>;

    async fn downstream_close_request(
        &self,
        msg: msg::DownstreamCloseRequest,
    ) -> Result<msg::DownstreamCloseResponse>;

    async fn downstream_chunk_ack(&self, msg: msg::DownstreamChunkAck) -> Result<()>;

    async fn upstream_call(&self, msg: msg::UpstreamCall) -> Result<msg::UpstreamCallAck>;

    fn subscribe_upstream(&self, stream_id_alias: u32) -> Result<mpsc::Receiver<msg::Message>>;
    fn unsubscribe_upstream(&self, stream_id_alias: u32) -> Result<()>;

    fn subscribe_downstream(
        &self,
        stream_id_alias: u32,
        qos: msg::QoS,
    ) -> Result<mpsc::Receiver<msg::Message>>;
    fn unsubscribe_downstream(&self, stream_id_alias: u32) -> Result<()>;

    fn subscribe_downstream_meta(&self) -> broadcast::Receiver<msg::DownstreamMetadata>;
    fn unsubscribe_downstream_meta(&self);

    fn subscribe_downstream_call(&self) -> broadcast::Receiver<msg::DownstreamCall>;
    fn unsubscribe_downstream_call(&self);
}

#[cfg(test)]
mock! {
    pub MockConnection{}
    #[async_trait]
    impl Connection for MockConnection {
        fn subscribe_disconnect_notify(&self) -> DisconnectNotificationReceiver;
        async fn close(&self) -> Result<()>;
        async fn open_request(
            &self,
            msg: msg::ConnectRequest,
        ) -> Result<msg::ConnectResponse>;
        async fn disconnect(
            &self,
            msg: msg::Disconnect,
        ) -> Result<()>;

        async fn upstream_open_request(
            &self,
            msg: msg::UpstreamOpenRequest,
        ) -> Result<msg::UpstreamOpenResponse>;
        async fn upstream_resume_request(
            &self,
            msg: msg::UpstreamResumeRequest,
            qos: msg::QoS,
        ) -> Result<msg::UpstreamResumeResponse>;
        async fn upstream_metadata(
            &self,
            msg: msg::UpstreamMetadata,
        ) -> Result<msg::UpstreamMetadataAck>;
        async fn upstream_chunk(&self, msg: msg::UpstreamChunk) -> Result<()>;
        async fn upstream_close_request(
            &self,
            msg: msg::UpstreamCloseRequest,
        ) -> Result<msg::UpstreamCloseResponse>;

        async fn downstream_open_request(
            &self,
            msg: msg::DownstreamOpenRequest,
        ) -> Result<msg::DownstreamOpenResponse>;
        async fn downstream_resume_request (&self, msg: msg::DownstreamResumeRequest) -> Result<msg::DownstreamResumeResponse>;
        async fn downstream_close_request(
            &self,
            msg: msg::DownstreamCloseRequest,
        ) -> Result<msg::DownstreamCloseResponse>;
        async fn downstream_chunk_ack(&self, msg: msg::DownstreamChunkAck) -> Result<()>;

        async fn upstream_call(&self, msg: msg::UpstreamCall) -> Result<msg::UpstreamCallAck>;

        fn subscribe_upstream(&self, stream_id_alias: u32) -> Result<mpsc::Receiver<msg::Message>>;
        fn unsubscribe_upstream(&self, stream_id_alias: u32) -> Result<()>;

        fn subscribe_downstream(
            &self,
            stream_id_alias: u32,
            qos: msg::QoS,
        ) -> Result<mpsc::Receiver<msg::Message>>;
        fn unsubscribe_downstream(&self, stream_id_alias: u32) -> Result<()>;

        fn subscribe_downstream_meta(&self) -> broadcast::Receiver<msg::DownstreamMetadata>;
        fn unsubscribe_downstream_meta(&self);

        fn subscribe_downstream_call(&self) -> broadcast::Receiver<msg::DownstreamCall>;
        fn unsubscribe_downstream_call(&self);
    }
}

pub type BoxedConnection = Box<dyn Connection>;

#[async_trait]
impl<C: Connection + ?Sized> Connection for Box<C> {
    fn subscribe_disconnect_notify(&self) -> DisconnectNotificationReceiver {
        (**self).subscribe_disconnect_notify()
    }
    async fn close(&self) -> Result<()> {
        (**self).close().await
    }
    async fn open_request(&self, msg: msg::ConnectRequest) -> Result<msg::ConnectResponse> {
        (**self).open_request(msg).await
    }
    async fn disconnect(&self, msg: msg::Disconnect) -> Result<()> {
        (**self).disconnect(msg).await
    }
    async fn upstream_open_request(
        &self,
        msg: msg::UpstreamOpenRequest,
    ) -> Result<msg::UpstreamOpenResponse> {
        (**self).upstream_open_request(msg).await
    }
    async fn upstream_resume_request(
        &self,
        msg: msg::UpstreamResumeRequest,
        qos: msg::QoS,
    ) -> Result<msg::UpstreamResumeResponse> {
        (**self).upstream_resume_request(msg, qos).await
    }
    async fn upstream_metadata(
        &self,
        msg: msg::UpstreamMetadata,
    ) -> Result<msg::UpstreamMetadataAck> {
        (**self).upstream_metadata(msg).await
    }

    async fn upstream_chunk(&self, msg: msg::UpstreamChunk) -> Result<()> {
        (**self).upstream_chunk(msg).await
    }

    async fn upstream_close_request(
        &self,
        msg: msg::UpstreamCloseRequest,
    ) -> Result<msg::UpstreamCloseResponse> {
        (**self).upstream_close_request(msg).await
    }

    async fn downstream_open_request(
        &self,
        msg: msg::DownstreamOpenRequest,
    ) -> Result<msg::DownstreamOpenResponse> {
        (**self).downstream_open_request(msg).await
    }
    async fn downstream_resume_request(
        &self,
        msg: msg::DownstreamResumeRequest,
    ) -> Result<msg::DownstreamResumeResponse> {
        (**self).downstream_resume_request(msg).await
    }
    async fn downstream_close_request(
        &self,
        msg: msg::DownstreamCloseRequest,
    ) -> Result<msg::DownstreamCloseResponse> {
        (**self).downstream_close_request(msg).await
    }

    async fn downstream_chunk_ack(&self, msg: msg::DownstreamChunkAck) -> Result<()> {
        (**self).downstream_chunk_ack(msg).await
    }

    async fn upstream_call(&self, msg: msg::UpstreamCall) -> Result<msg::UpstreamCallAck> {
        (**self).upstream_call(msg).await
    }

    fn subscribe_upstream(&self, stream_id_alias: u32) -> Result<mpsc::Receiver<msg::Message>> {
        (**self).subscribe_upstream(stream_id_alias)
    }
    fn unsubscribe_upstream(&self, stream_id_alias: u32) -> Result<()> {
        (**self).unsubscribe_upstream(stream_id_alias)
    }

    fn subscribe_downstream_meta(&self) -> broadcast::Receiver<msg::DownstreamMetadata> {
        (**self).subscribe_downstream_meta()
    }
    fn unsubscribe_downstream_meta(&self) {
        (**self).unsubscribe_downstream_meta()
    }

    fn subscribe_downstream(
        &self,
        stream_id_alias: u32,
        qos: msg::QoS,
    ) -> Result<mpsc::Receiver<msg::Message>> {
        (**self).subscribe_downstream(stream_id_alias, qos)
    }
    fn unsubscribe_downstream(&self, stream_id_alias: u32) -> Result<()> {
        (**self).unsubscribe_downstream(stream_id_alias)
    }

    fn subscribe_downstream_call(&self) -> broadcast::Receiver<msg::DownstreamCall> {
        (**self).subscribe_downstream_call()
    }
    fn unsubscribe_downstream_call(&self) {
        (**self).unsubscribe_downstream_call()
    }
}

#[async_trait]
pub trait Connector: Send + Sync {
    async fn connect(&self, timeout: Option<Duration>) -> Result<BoxedConnection>;
}

pub type BoxedConnector = Box<dyn Connector>;

#[async_trait]
impl<C: Connector + ?Sized> Connector for Box<C> {
    async fn connect(&self, timeout: Option<Duration>) -> Result<BoxedConnection> {
        (**self).connect(timeout).await
    }
}
