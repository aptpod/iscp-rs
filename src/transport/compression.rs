//! Compression definitions.

use bytes::BytesMut;

use crate::transport::TransportError;

#[derive(Clone, Debug, Default)]
pub struct Compression {
    enabled: bool,
    level: Option<i8>,
    window_bits: Option<u8>,
}

impl Compression {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enable(mut self, level: i8) -> Self {
        self.enabled = true;
        self.level = Some(level);
        self.window_bits = None;
        self
    }

    pub fn enable_with_window_bits(mut self, level: i8, window_bits: u8) -> Self {
        self.enabled = true;
        self.level = Some(level);
        self.window_bits = Some(window_bits);
        self
    }

    pub fn disable(mut self) -> Self {
        self.enabled = false;
        self.level = None;
        self.window_bits = None;
        self
    }

    pub(crate) fn converters(&self) -> (Compressor, Extractor) {
        log::debug!("compression: {:?}", self);
        (
            Compressor::new(self.level, self.window_bits),
            Extractor::new(self.level, self.window_bits),
        )
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn level(&self) -> Option<i8> {
        self.level
    }

    pub fn context_takeover(&self) -> bool {
        self.window_bits.is_some()
    }

    pub fn window_bits(&self) -> Option<u8> {
        self.window_bits
    }
}

#[derive(Debug)]
pub(crate) struct Compressor {
    compress: Option<flate2::Compress>,
    buffer: Vec<u8>,
    buffer_increment_size: usize,
    dictionary: Option<DictionaryBuffer>,
}

impl Compressor {
    pub(crate) fn new(level: Option<i8>, window_bits: Option<u8>) -> Self {
        let buffer = Vec::new();
        let buffer_increment_size = 128;

        let level = match level {
            Some(level) => flate2::Compression::new(level as u32),
            None => {
                return Compressor {
                    compress: None,
                    buffer,
                    buffer_increment_size,
                    dictionary: None,
                }
            }
        };

        Compressor {
            compress: if let Some(window_bits) = window_bits {
                Some(flate2::Compress::new_with_window_bits(
                    level,
                    false,
                    window_bits,
                ))
            } else {
                Some(flate2::Compress::new(level, false))
            },
            buffer,
            buffer_increment_size,
            dictionary: window_bits.map(DictionaryBuffer::new),
        }
    }

    #[cfg(test)]
    pub(crate) fn buffer_increment_size(mut self, size: usize) -> Self {
        self.buffer_increment_size = size;
        self
    }

    pub(crate) fn compress(&mut self, data: &mut BytesMut) -> Result<(), TransportError> {
        if self.compress.is_none() {
            return Ok(());
        }

        let compress = self.compress.as_mut().unwrap();
        compress.reset();
        if let Some(dictionary) = &mut self.dictionary {
            if !dictionary.dictionary().is_empty() {
                compress
                    .set_dictionary(dictionary.dictionary())
                    .map_err(TransportError::new)?;
            }
            dictionary.push_msg(data);
        }

        self.buffer.clear();
        self.buffer.reserve(data.len());

        loop {
            let total_in = compress.total_in() as usize;
            let flush_compress = if total_in < data.len() {
                flate2::FlushCompress::None
            } else {
                flate2::FlushCompress::Finish
            };
            match compress
                .compress_vec(&data[total_in..], &mut self.buffer, flush_compress)
                .map_err(TransportError::new)?
            {
                flate2::Status::Ok => self.buffer.reserve(self.buffer_increment_size),
                flate2::Status::BufError => {
                    if compress.total_in() == 0 {
                        log::debug!("ignore to compress blank data");
                        break;
                    } else {
                        self.buffer.reserve(self.buffer_increment_size);
                    }
                }
                flate2::Status::StreamEnd => break,
            }
        }
        data.clear();
        data.extend_from_slice(&self.buffer);

        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct Extractor {
    decompress: Option<flate2::Decompress>,
    buffer: Vec<u8>,
    buffer_increment_size: usize,
    dictionary: Option<DictionaryBuffer>,
}

impl Extractor {
    pub(crate) fn new(level: Option<i8>, window_bits: Option<u8>) -> Self {
        let buffer = Vec::new();
        let buffer_increment_size = 128;

        if level.is_none() {
            return Extractor {
                decompress: None,
                buffer,
                buffer_increment_size,
                dictionary: None,
            };
        }

        Extractor {
            decompress: if let Some(window_bits) = window_bits {
                Some(flate2::Decompress::new_with_window_bits(false, window_bits))
            } else {
                Some(flate2::Decompress::new(false))
            },
            buffer,
            buffer_increment_size,
            dictionary: window_bits.map(DictionaryBuffer::new),
        }
    }

    #[cfg(test)]
    pub(crate) fn buffer_increment_size(mut self, size: usize) -> Self {
        self.buffer_increment_size = size;
        self
    }

    pub(crate) fn extract(&mut self, data: &mut BytesMut) -> Result<(), TransportError> {
        if self.decompress.is_none() {
            return Ok(());
        }

        let decompress = self.decompress.as_mut().unwrap();
        decompress.reset(false);
        if let Some(dictionary) = &self.dictionary {
            if !dictionary.dictionary().is_empty() {
                decompress
                    .set_dictionary(dictionary.dictionary())
                    .map_err(TransportError::new)?;
            }
        }

        self.buffer.clear();
        self.buffer.reserve(data.len() * 2); // Assume a compression ratio of 50% to reduce the number of memory allocations

        loop {
            let total_in = decompress.total_in() as usize;
            let flush_decompress = if total_in < data.len() {
                flate2::FlushDecompress::None
            } else {
                flate2::FlushDecompress::Finish
            };
            match decompress
                .decompress_vec(&data[total_in..], &mut self.buffer, flush_decompress)
                .map_err(TransportError::new)?
            {
                flate2::Status::Ok => self.buffer.reserve(self.buffer_increment_size),
                flate2::Status::BufError => {
                    if decompress.total_in() == 0 {
                        log::debug!("ignore to decompress blank data");
                        break;
                    } else {
                        self.buffer.reserve(self.buffer_increment_size);
                    }
                }
                flate2::Status::StreamEnd => break,
            }
        }
        data.clear();
        data.extend_from_slice(&self.buffer);

        if let Some(dictionary) = &mut self.dictionary {
            dictionary.push_msg(data);
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct DictionaryBuffer {
    size: usize,
    buf: Vec<u8>,
}

impl DictionaryBuffer {
    fn new(window_bits: u8) -> Self {
        assert!((9..=15).contains(&window_bits));
        let size = 2 << (window_bits - 1);
        Self {
            size,
            buf: Vec::with_capacity(size * 2),
        }
    }

    fn push_msg(&mut self, msg: &[u8]) {
        let buf_len = self.buf.len();
        let msg_len = msg.len();
        let remaining = self.size - buf_len;

        if msg.len() >= self.size {
            self.buf.clear();
            self.buf.extend_from_slice(&msg[(msg_len - self.size)..]);
        } else if remaining >= msg_len {
            self.buf.extend_from_slice(msg);
        } else {
            self.buf.extend_from_slice(msg);
            let l = self.buf.len();
            self.buf.copy_within((l - self.size).., 0);
            self.buf.resize_with(self.size, || unreachable!());
        }
        debug_assert!(self.buf.len() <= self.size);
    }

    fn dictionary(&self) -> &[u8] {
        &self.buf
    }
}

#[cfg(test)]
mod test {
    use rand::{distributions::Alphanumeric, Rng};

    use super::*;

    #[test]
    fn compress() {
        struct Case {
            compressor: Compressor,
            input: Vec<u8>,
            want: Vec<u8>,
        }
        let cases = vec![
            Case {
                compressor: Compressor::new(Some(1), None).buffer_increment_size(1),
                input: vec![26, 4, 18, 2, 79, 75],
                want: vec![147, 98, 17, 98, 242, 247, 6, 0],
            },
            Case {
                compressor: Compressor::new(Some(1), None).buffer_increment_size(1024),
                input: vec![26, 4, 18, 2, 79, 75],
                want: vec![147, 98, 17, 98, 242, 247, 6, 0],
            },
        ];

        impl Case {
            fn test(&mut self) {
                let mut buf = BytesMut::from(&self.input[..]);
                self.compressor.compress(&mut buf).unwrap();
                assert!(
                    buf == self.want,
                    "name: {:?}, want {:?}",
                    Vec::from(buf),
                    self.want
                );
            }
        }

        for mut case in cases {
            case.test();
        }
    }

    #[test]
    fn compress_extract() {
        let mut rng = rand::thread_rng();

        struct Case {
            compressor: Compressor,
            extractor: Extractor,
            input: Vec<u8>,
        }
        let cases = vec![
            Case {
                compressor: Compressor::new(Some(1), None).buffer_increment_size(1),
                extractor: Extractor::new(Some(1), None).buffer_increment_size(1),
                input: (0..1024 * 1024)
                    .map(|_| rng.sample(Alphanumeric) as u8)
                    .collect(),
            },
            Case {
                compressor: Compressor::new(Some(6), None).buffer_increment_size(1024),
                extractor: Extractor::new(Some(6), None).buffer_increment_size(1024),
                input: (0..1024 * 1024)
                    .map(|_| rng.sample(Alphanumeric) as u8)
                    .collect(),
            },
            Case {
                compressor: Compressor::new(Some(9), Some(15)).buffer_increment_size(1024),
                extractor: Extractor::new(Some(9), Some(15)).buffer_increment_size(1024),
                input: (0..1024 * 1024)
                    .map(|_| rng.sample(Alphanumeric) as u8)
                    .collect(),
            },
        ];

        impl Case {
            fn test(&mut self) {
                let mut buf = BytesMut::with_capacity(self.input.len());
                buf.extend_from_slice(&self.input);

                self.compressor.compress(&mut buf).unwrap();
                assert_ne!(self.input, buf);
                self.extractor.extract(&mut buf).unwrap();
                assert_eq!(self.input, buf);

                let mut buf = BytesMut::with_capacity(self.input.len());
                buf.extend_from_slice(&self.input);

                self.compressor.compress(&mut buf).unwrap();
                assert_ne!(self.input, buf);
                self.extractor.extract(&mut buf).unwrap();
                assert_eq!(self.input, buf);
            }
        }

        for mut case in cases {
            case.test();
        }
    }

    #[test]
    fn compress_extract_msgs() {
        struct Case {
            compressor: Compressor,
            extractor: Extractor,
            n_msg: usize,
            msg_size: usize,
        }

        let cases = vec![
            Case {
                compressor: Compressor::new(Some(9), Some(9)),
                extractor: Extractor::new(Some(9), Some(9)),
                n_msg: 20,
                msg_size: 5000,
            },
            Case {
                compressor: Compressor::new(Some(9), Some(15)),
                extractor: Extractor::new(Some(9), Some(15)),
                n_msg: 20,
                msg_size: 5000,
            },
            Case {
                compressor: Compressor::new(Some(9), Some(15)),
                extractor: Extractor::new(Some(9), Some(15)),
                n_msg: 3,
                msg_size: 10_000_000,
            },
        ];

        impl Case {
            fn test(&mut self) {
                let msgs: Vec<Vec<u8>> = (0..self.n_msg)
                    .map(|_| {
                        let mut rng = rand::thread_rng();
                        (0..self.msg_size)
                            .map(|_| rng.sample(Alphanumeric) as u8)
                            .collect()
                    })
                    .collect();

                for msg in &msgs {
                    let mut buf = BytesMut::new();
                    buf.extend_from_slice(msg);
                    self.compressor.compress(&mut buf).unwrap();
                    self.extractor.extract(&mut buf).unwrap();
                    assert_eq!(**msg, *buf);
                }
            }
        }

        for mut case in cases {
            case.test();
        }
    }
}
