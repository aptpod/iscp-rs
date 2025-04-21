use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use byteorder::{BigEndian, ByteOrder};
use bytes::{BufMut, Bytes, BytesMut};
use fnv::FnvHashMap;

pub struct Splitter {
    pub sequence_number: u32,
    pub max_segment_size: usize,
    pub max_message_size: usize,
}

impl Splitter {
    pub fn new(max_datagram_size: usize) -> Self {
        assert!(max_datagram_size > 8);
        Self {
            sequence_number: 0,
            max_segment_size: max_datagram_size - 8,
            max_message_size: (max_datagram_size - 8) * u16::MAX as usize,
        }
    }

    pub fn split(&mut self, data: &[u8], frames: &mut VecDeque<Bytes>) -> Result<(), ()> {
        if data.len() >= self.max_message_size {
            return Err(());
        }
        let sequence_number = self.sequence_number;
        self.sequence_number = self.sequence_number.wrapping_add(1);

        let mut buf = BytesMut::new();

        let max_segment_i = data.len() / self.max_segment_size;
        let max_segment_i = if data.len() % self.max_segment_size == 0 && max_segment_i > 0 {
            max_segment_i - 1
        } else {
            max_segment_i
        };
        let max_segment_i_u16: u16 = max_segment_i.try_into().unwrap();

        for i in 0..(max_segment_i + 1) {
            let segment_data = if i == (max_segment_i) {
                &data[(i * self.max_segment_size)..]
            } else {
                &data[(i * self.max_segment_size)..((i + 1) * self.max_segment_size)]
            };
            buf.put_u32(sequence_number);
            buf.put_u16(max_segment_i_u16);
            buf.put_u16(i as u16);
            buf.extend_from_slice(segment_data);
            let frame = buf.split().freeze();
            frames.push_back(frame);
        }

        Ok(())
    }
}

pub struct FrameBuf {
    expire: Duration,
    checked_at: Instant,
    frames: FnvHashMap<u32, (Instant, usize, Vec<Bytes>)>,
}

impl FrameBuf {
    pub fn new() -> Self {
        Self {
            expire: Duration::from_secs(20),
            checked_at: Instant::now(),
            frames: FnvHashMap::default(),
        }
    }

    pub fn push(&mut self, frame: Bytes) -> Option<Bytes> {
        if frame.len() < 9 {
            return None;
        }

        let now = Instant::now();
        if now - self.checked_at > self.expire {
            self.frames.retain(|_, (t, _, _)| now - *t > self.expire);
        }

        let sequence_number = BigEndian::read_u32(&frame[0..4]);
        let max_segment_index = BigEndian::read_u16(&frame[4..6]);

        if max_segment_index == 0 {
            return Some(frame.slice(8..));
        }

        if let Some((_, needed_frame, frames)) = self.frames.get_mut(&sequence_number) {
            frames.push(frame);

            if *needed_frame == frames.len() {
                let frames = self.frames.remove(&sequence_number).unwrap().2;
                return Some(join_frames(frames));
            }
        } else {
            self.frames.insert(
                sequence_number,
                (Instant::now(), max_segment_index as usize + 1, vec![frame]),
            );
        }

        None
    }
}

fn join_frames(mut frames: Vec<Bytes>) -> Bytes {
    let mut buf = BytesMut::new();

    frames.sort_unstable_by_key(|frame| BigEndian::read_u16(&frame[6..8]));

    for frame in frames.into_iter() {
        buf.extend_from_slice(&frame[8..]);
    }

    buf.freeze()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn datagram() {
        struct Case {
            max_datagram_size: usize,
            data: &'static [u8],
            frames: &'static [&'static [u8]],
        }

        let cases = &[
            Case {
                max_datagram_size: 12,
                data: &[0x00, 0x01, 0x02, 0x03],
                frames: &[&[
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03,
                ]],
            },
            Case {
                max_datagram_size: 12,
                data: &[0x00, 0x01, 0x02, 0x03, 0x04, 0x05],
                frames: &[
                    &[
                        0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03,
                    ],
                    &[0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x04, 0x05],
                ],
            },
            Case {
                max_datagram_size: 12,
                data: &[0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07],
                frames: &[
                    &[
                        0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03,
                    ],
                    &[
                        0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x04, 0x05, 0x06, 0x07,
                    ],
                ],
            },
        ];

        let mut frame_buf = FrameBuf::new();

        for case in cases {
            let mut splitter = Splitter::new(case.max_datagram_size);
            let mut frames = VecDeque::new();
            splitter.split(case.data, &mut frames).unwrap();
            let frames: Vec<&[u8]> = frames.iter().map(|frame| &frame[..]).collect();

            assert_eq!(case.frames, &frames);

            for (i, frame) in frames.iter().enumerate() {
                if let Some(data) = frame_buf.push(frame.to_vec().into()) {
                    assert_eq!(i, frames.len() - 1);
                    assert_eq!(case.data, data);
                } else {
                    assert_ne!(i, frames.len() - 1);
                }
            }
        }
    }
}
