use crate::error::{Error, Result};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use log::*;
use std::{
    borrow::Cow,
    collections::{hash_map::Entry, HashMap},
    sync::atomic::{AtomicU32, Ordering},
};

#[derive(Clone, PartialEq, Default, Debug)]
pub(crate) struct Frame<'a> {
    length: u32,
    payload: Cow<'a, [u8]>,
}

impl<'a> Frame<'a> {
    pub fn new(payload: &'a [u8]) -> Self {
        Self {
            length: payload.len() as u32,
            payload: Cow::Borrowed(payload),
        }
    }

    pub fn to_bytes(&self) -> Bytes {
        let mut res = BytesMut::new();
        res.put_u32(self.length);
        res.put_slice(self.payload.as_ref());

        res.into()
    }
}

#[derive(Default, Debug)]
pub(crate) struct DatagramsFrameIdGenerator(AtomicU32);
impl DatagramsFrameIdGenerator {
    pub fn next(&self) -> u32 {
        self.0.fetch_add(1, Ordering::Release)
    }
}

#[derive(Clone, PartialEq, Default, Debug)]
pub(crate) struct DatagramsFrame<'a> {
    id: u32,
    max_segment_index: u16,
    segment_index: u16,
    payload: Cow<'a, [u8]>,
}

impl<'a> DatagramsFrame<'a> {
    const HEADER_LEN: usize = 8;

    pub fn new(buf: &'a [u8], id: u32, mtu: usize) -> Result<Vec<Self>> {
        let mut res = Vec::new();

        if mtu <= Self::HEADER_LEN {
            return Err(Error::invalid_value("mtu must be grater then 8"));
        }

        let seg_len = mtu - Self::HEADER_LEN;

        let chunks = buf.chunks(seg_len);
        let num = chunks.len() - 1;

        if num > u16::MAX as usize {
            return Err(Error::invalid_value("payload length is too long"));
        }

        chunks.enumerate().for_each(|(i, chunk)| {
            res.push(DatagramsFrame {
                id,
                max_segment_index: num as u16,
                segment_index: i as u16,
                payload: Cow::Borrowed(chunk),
            })
        });

        Ok(res)
    }

    pub fn parse<B: Buf>(mut bin: B) -> Result<Self> {
        if bin.remaining() < Self::HEADER_LEN {
            return Err(Error::invalid_value(
                "binary len is shorter than header length",
            ));
        }

        let id = bin.get_u32();
        let max_segment_index = bin.get_u16();
        let segment_index = bin.get_u16();

        Ok(Self {
            id,
            max_segment_index,
            segment_index,
            payload: Cow::from(bin.chunk().to_owned()),
        })
    }

    pub fn into_bytes(self) -> Bytes {
        let Self {
            id,
            segment_index,
            max_segment_index,
            payload,
        } = self;
        let mut res = BytesMut::new();
        res.put_u32(id);
        res.put_u16(max_segment_index);
        res.put_u16(segment_index);

        res.put(payload.as_ref());

        res.into()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DatagramsBuffer<'a> {
    frames: HashMap<u32, Vec<DatagramsFrame<'a>>>,
    timestamps: HashMap<u32, chrono::DateTime<chrono::Utc>>,
    timeout: chrono::Duration,
}

impl<'a> Default for DatagramsBuffer<'a> {
    fn default() -> Self {
        Self {
            frames: HashMap::default(),
            timestamps: HashMap::default(),
            timeout: chrono::Duration::milliseconds(100),
        }
    }
}

impl<'a> DatagramsBuffer<'a> {
    pub fn push_and_parse(&mut self, frame: DatagramsFrame<'a>) -> Option<Vec<u8>> {
        let id = frame.id;
        if frame.max_segment_index == 0 {
            self.timestamps.remove(&id);
            self.frames.remove(&id);
            let payload: Vec<u8> = frame.payload.into_owned();
            return Some(payload);
        }

        let now = chrono::Utc::now();
        let mut entry = match self.frames.entry(id) {
            Entry::Occupied(e) => e,
            Entry::Vacant(e) => {
                e.insert(vec![frame]);
                self.timestamps.insert(id, now);
                return None;
            }
        };

        let mut ts_entry = match self.timestamps.entry(id) {
            Entry::Vacant(_) => unreachable!("must exist timestamp"),
            Entry::Occupied(e) => e,
        };
        let d = now - *ts_entry.get();

        if d.ge(&self.timeout) {
            entry.insert(vec![frame]);
            ts_entry.insert(now);
            return None;
        }

        let frames = entry.get_mut();
        let max_segment_index = frame.max_segment_index as usize;
        frames.push(frame);
        if frames.len() == (max_segment_index + 1) {
            match self.build_message(&id) {
                Ok(msg) => return Some(msg),
                Err(_) => {
                    error!("unexpected case");
                    return None;
                }
            }
        }

        None
    }

    fn build_message(&mut self, id: &u32) -> Result<Vec<u8>> {
        let mut frames = match self.frames.remove(id) {
            Some(frames) => {
                self.timestamps.remove(id);
                frames
            }
            None => return Err(Error::unexpected("NotFound")),
        };
        let mut frames: Vec<DatagramsFrame> = std::mem::take(&mut frames);
        if frames.is_empty() {
            return Err(Error::unexpected("NotFound"));
        }
        frames.sort_by(|a, b| a.segment_index.cmp(&b.segment_index));
        let frame = frames
            .into_iter()
            .flat_map(|f| f.payload.into_owned())
            .collect::<Vec<_>>();

        Ok(frame)
    }

    pub fn clear_timeout_buf(&mut self) {
        let now = chrono::Utc::now();

        let ids = self
            .timestamps
            .iter()
            .filter(|(_, time)| {
                let d = now - **time;
                d.ge(&self.timeout)
            })
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();

        ids.iter().for_each(|id| {
            self.frames.remove(id);
            self.timestamps.remove(id);
        })
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use super::*;

    #[test]
    fn frame() {
        let payload = vec![1, 2, 3, 4];
        let want = Frame {
            length: payload.len() as u32,
            payload: Cow::Borrowed(&payload),
        };

        let mut want_bin = (payload.len() as u32).to_be_bytes().to_vec();
        want_bin.append(payload.clone().as_mut());

        let testee = Frame::new(&payload);
        assert_eq!(want, testee);
        assert_eq!(want_bin, testee.to_bytes());
    }

    #[test]
    fn push_and_parse() {
        struct Case<'a> {
            timeout: chrono::Duration,
            stubs: Vec<DatagramsFrame<'a>>,
            want: Vec<u8>,
            want_err: bool,
        }
        impl<'a> Case<'a> {
            pub fn test(self) {
                let mut buf = DatagramsBuffer {
                    timeout: self.timeout,
                    ..Default::default()
                };

                for s in self.stubs.into_iter() {
                    if let Some(msg) = buf.push_and_parse(s) {
                        if self.want_err {
                            panic!("not want msg");
                        }
                        assert_eq!(msg, self.want);
                        return;
                    }
                }
                if !self.want_err {
                    panic!("want any msg")
                }
            }
        }

        let mut cases = Vec::new();
        cases.push(Case {
            timeout: chrono::Duration::milliseconds(1),
            stubs: vec![
                DatagramsFrame {
                    id: 1,
                    segment_index: 1,
                    max_segment_index: 1,
                    payload: Cow::from(vec![1u8]),
                },
                DatagramsFrame {
                    id: 1,
                    segment_index: 0,
                    max_segment_index: 1,
                    payload: Cow::from(vec![0u8]),
                },
            ],
            want: vec![0, 1],
            want_err: false,
        });

        cases.push(Case {
            timeout: chrono::Duration::milliseconds(1),
            stubs: vec![DatagramsFrame {
                id: 1,
                segment_index: 0,
                max_segment_index: 0,
                payload: Cow::from(vec![0u8]),
            }],
            want: vec![0],
            want_err: false,
        });

        cases.push(Case {
            timeout: chrono::Duration::milliseconds(0),
            stubs: vec![
                DatagramsFrame {
                    id: 1,
                    segment_index: 1,
                    max_segment_index: 1,
                    payload: Cow::from(vec![1]),
                },
                DatagramsFrame {
                    id: 1,
                    segment_index: 0,
                    max_segment_index: 1,
                    payload: Cow::from(vec![0]),
                },
            ],
            want: Vec::new(),
            want_err: true,
        });

        for case in cases.into_iter() {
            case.test()
        }
    }
    #[test]
    fn clear_timeout_buf() {
        let mut buf = DatagramsBuffer {
            timeout: chrono::Duration::milliseconds(1),
            ..Default::default()
        };
        buf.push_and_parse(DatagramsFrame {
            id: 1,
            segment_index: 0,
            max_segment_index: 1,
            ..Default::default()
        });
        assert_eq!(buf.frames.len(), 1);
        std::thread::sleep(Duration::from_millis(10));
        buf.push_and_parse(DatagramsFrame {
            id: 2,
            segment_index: 0,
            max_segment_index: 1,
            ..Default::default()
        });
        assert_eq!(buf.frames.len(), 2);
        buf.clear_timeout_buf();
        assert_eq!(buf.frames.len(), 1);
    }

    #[test]
    fn new_datagrams_frame() {
        let stub = vec![0, 1, 2];
        let want_frame_num = stub.len();
        let gots = DatagramsFrame::new(&stub, 1, 9).unwrap();
        assert_eq!(gots.len(), want_frame_num);

        for (i, got) in gots.into_iter().enumerate() {
            assert_eq!(
                got,
                DatagramsFrame {
                    id: 1,
                    segment_index: i as u16,
                    max_segment_index: (want_frame_num - 1) as u16,
                    payload: Cow::from(vec![i as u8]),
                }
            )
        }
        assert!(DatagramsFrame::new(&stub, 1, 0).is_err());
        assert!(DatagramsFrame::new(&stub, 1, DatagramsFrame::HEADER_LEN).is_err());

        let too_long_payload = [0u8; 0x10001];
        assert!(DatagramsFrame::new(&too_long_payload, 1, 9).is_err());
    }
    #[test]
    fn datagrams_frame_into_bytes() {
        let payload = vec![1, 2, 3, 4];
        let mut testee = DatagramsFrame::new(&payload, 1, DatagramsFrame::HEADER_LEN + 2).unwrap();
        assert_eq!(testee.len(), 2);

        let testee = testee.first_mut().unwrap();
        let testee: DatagramsFrame = std::mem::take(testee);
        let got = testee.into_bytes();
        let want = vec![
            0, 0, 0, 1, // id
            0, 1, // max segment index,
            0, 0, // segment index
            1, 2, // payload
        ];
        let want = Bytes::from(want);
        assert_eq!(got, want);
    }

    #[test]
    fn datagrams_frame_parse() {
        let empty_binary = Bytes::new();
        assert!(DatagramsFrame::parse(empty_binary).is_err());

        let stub = vec![
            0, 0, 0, 1, // id
            0, 0, // segment index
            0, 0, // max segment index,
            1, 2, 3, 4, // payload
        ];
        let stub = Bytes::from(stub);
        let want = DatagramsFrame {
            id: 1,
            max_segment_index: 0,
            segment_index: 0,
            payload: Cow::from(vec![1, 2, 3, 4]),
        };
        let got = DatagramsFrame::parse(stub).unwrap();
        assert_eq!(got, want);
    }
}
