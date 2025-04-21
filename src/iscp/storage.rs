use std::collections::BTreeMap;

use super::upstream::*;
use super::*;

pub(crate) struct UpstreamStorage {
    stream_id: Uuid,
    is_reliable: bool,
    send_data_points_callback: Option<SendDataPointsCallback>,
    receive_ack_callback: Option<ReceiveAckCallback>,
    chunks: BTreeMap<u32, crate::message::UpstreamChunk>,
    last_received_sequence_number: Option<u32>,
}

impl UpstreamStorage {
    pub fn new(
        stream_id: Uuid,
        qos: QoS,
        send_data_points_callback: Option<SendDataPointsCallback>,
        receive_ack_callback: Option<ReceiveAckCallback>,
    ) -> Self {
        Self {
            stream_id,
            is_reliable: qos == QoS::Reliable,
            send_data_points_callback,
            receive_ack_callback,
            chunks: BTreeMap::new(),
            last_received_sequence_number: None,
        }
    }

    pub async fn add_chunk(
        &mut self,
        chunk: &crate::message::UpstreamChunk,
        dpgs: Vec<DataPointGroup>,
    ) {
        let sequence_number = chunk.stream_chunk.as_ref().unwrap().sequence_number;
        if self.is_reliable {
            self.chunks.insert(sequence_number, chunk.clone());
        }
        if let Some(callback) = &self.send_data_points_callback {
            let chunk = UpstreamChunk {
                sequence_number,
                data_point_groups: dpgs,
            };
            callback(self.stream_id, chunk).await;
        }
    }

    pub async fn process_ack(&mut self, ack: crate::message::UpstreamChunkAck) {
        for result in ack.results {
            if self.last_received_sequence_number.is_none()
                || self.last_received_sequence_number.unwrap() < result.sequence_number
            {
                self.last_received_sequence_number = Some(result.sequence_number);
            }

            if self.is_reliable {
                self.chunks.remove(&result.sequence_number);
            }
            if let Some(callback) = &self.receive_ack_callback {
                match convert_upstream_chunk_result(result) {
                    Ok(result) => callback(self.stream_id, result).await,
                    Err(e) => {
                        log::error!("invalid upstream chunk result: {}", e);
                    }
                }
            }
        }
    }

    pub fn iter_chunks(&mut self) -> impl Iterator<Item = crate::message::UpstreamChunk> + '_ {
        self.chunks.values().cloned()
    }

    pub fn last_received_sequence_number(&self) -> Option<u32> {
        self.last_received_sequence_number
    }
}

fn convert_upstream_chunk_result(
    r: crate::message::UpstreamChunkResult,
) -> Result<UpstreamChunkResult, Error> {
    Ok(UpstreamChunkResult {
        sequence_number: r.sequence_number,
        result_code: r
            .result_code
            .try_into()
            .map_err(|_| Error::invalid_value("invalid result_code"))?,
        result_string: r.result_string,
    })
}
