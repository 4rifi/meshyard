//! Bounded v1 framing. No signed event bytes are interpreted or rewritten here.
use crate::{
    NetError,
    limits::{ConnectionPermit, StreamPermit},
};
use iroh::endpoint::{RecvStream, SendStream};
use meshyard_core::wire::v1::{FrameHeaderV1, MAX_FRAME};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::time::timeout;
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(120);
pub const PAIR_TIMEOUT: Duration = Duration::from_secs(30);
pub const PAIR_MAX_FRAME: usize = 16 * 1024;
/// Reader and writer retain shared stream admission until both halves are dropped.
pub struct FrameStream {
    reader: FrameReader,
    writer: FrameWriter,
}
pub struct FrameReader {
    recv: RecvStream,
    connection: Arc<ConnectionPermit>,
    _permit: Arc<StreamPermit>,
    reads: Arc<AtomicU64>,
    max_frame: usize,
    deadline: Duration,
    pair_wait: Option<tokio::time::Instant>,
}
pub struct FrameWriter {
    send: SendStream,
    _permit: Arc<StreamPermit>,
    max_frame: usize,
    deadline: Duration,
}
impl FrameStream {
    pub(crate) fn new(
        send: SendStream,
        recv: RecvStream,
        connection: Arc<ConnectionPermit>,
        reads: Arc<AtomicU64>,
        pair: bool,
    ) -> Result<Self, NetError> {
        // Defense in depth; ADR013 disables early data in the actual TLS configs.
        if recv.is_0rtt() {
            return Err(NetError::EarlyData);
        }
        let permit = Arc::new(connection.stream()?);
        let max_frame = if pair { PAIR_MAX_FRAME } else { MAX_FRAME };
        let deadline = if pair {
            PAIR_TIMEOUT
        } else {
            HANDSHAKE_TIMEOUT
        };
        Ok(Self {
            reader: FrameReader {
                recv,
                connection,
                _permit: permit.clone(),
                reads,
                pair_wait: None,
                max_frame,
                deadline,
            },
            writer: FrameWriter {
                send,
                _permit: permit,
                max_frame,
                deadline,
            },
        })
    }
    pub fn wait_for_pair_decision(&mut self, remaining: Duration) {
        if self.reader.max_frame == PAIR_MAX_FRAME {
            self.reader.pair_wait =
                Some(tokio::time::Instant::now() + remaining.min(Duration::from_secs(300)));
        }
    }
    pub fn enter_live_phase(&mut self) {
        if self.reader.max_frame != PAIR_MAX_FRAME {
            self.reader.deadline = IDLE_TIMEOUT;
            self.writer.deadline = IDLE_TIMEOUT;
        }
    }
    pub fn split(self) -> (FrameReader, FrameWriter) {
        (self.reader, self.writer)
    }
    pub async fn read(&mut self, types: &[u8]) -> Result<(u8, Vec<u8>), NetError> {
        self.reader.read(types).await
    }
    pub async fn write(&mut self, kind: u8, payload: &[u8]) -> Result<(), NetError> {
        self.writer.write(kind, payload).await
    }
    pub fn finish(&mut self) -> Result<(), NetError> {
        self.writer.finish()
    }
    pub(crate) async fn acknowledged(&mut self) -> Result<(), NetError> {
        self.writer.acknowledged().await
    }
}
impl FrameReader {
    pub async fn read(&mut self, types: &[u8]) -> Result<(u8, Vec<u8>), NetError> {
        let mut bytes = [0; 10];
        let start = if let Some(deadline) = self.pair_wait {
            tokio::time::timeout_at(deadline, self.recv.read_exact(&mut bytes[..1]))
                .await
                .map_err(|_| NetError::Timeout)?
                .map_err(|_| NetError::Frame)?;
            1
        } else {
            0
        };
        timeout(self.deadline, async {
            self.connection.ingress(10)?;
            self.reads.fetch_add(1, Ordering::Relaxed);
            self.recv
                .read_exact(&mut bytes[start..])
                .await
                .map_err(|_| NetError::Frame)?;
            let header = FrameHeaderV1::decode(bytes, types).map_err(|_| NetError::Frame)?;
            if header.len as usize > self.max_frame {
                return Err(NetError::Frame);
            }
            self.connection.ingress(header.len as usize)?;
            let mut payload = vec![0; header.len as usize];
            self.recv
                .read_exact(&mut payload)
                .await
                .map_err(|_| NetError::Frame)?;
            Ok((header.frame_type, payload))
        })
        .await
        .map_err(|_| NetError::Timeout)?
    }
}
impl FrameWriter {
    pub async fn write(&mut self, kind: u8, payload: &[u8]) -> Result<(), NetError> {
        if payload.len() > self.max_frame {
            return Err(NetError::Frame);
        }
        let header = FrameHeaderV1 {
            frame_type: kind,
            len: payload.len() as u32,
        }
        .encode()
        .map_err(|_| NetError::Frame)?;
        timeout(self.deadline, async {
            self.send
                .write_all(&header)
                .await
                .map_err(|_| NetError::Frame)?;
            self.send
                .write_all(payload)
                .await
                .map_err(|_| NetError::Frame)
        })
        .await
        .map_err(|_| NetError::Timeout)?
    }
    pub fn finish(&mut self) -> Result<(), NetError> {
        self.send.finish().map_err(|_| NetError::Closed)
    }
    pub(crate) async fn acknowledged(&mut self) -> Result<(), NetError> {
        timeout(self.deadline, self.send.stopped())
            .await
            .map_err(|_| NetError::Timeout)?
            .map_err(|_| NetError::Closed)?;
        Ok(())
    }
}
