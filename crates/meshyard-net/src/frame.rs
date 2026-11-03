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

/// A stream can only be constructed after authenticated device admission.
/// Receives are bounded before allocation, including aggregate per-device ingress.
/// Dropping a stream releases its shared per-device stream permit.
pub struct FrameStream {
    send: SendStream,
    recv: RecvStream,
    connection: Arc<ConnectionPermit>,
    _permit: StreamPermit,
    reads: Arc<AtomicU64>,
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
        // Defensive check. The pinned API flags streams accepted before handshake,
        // not the wire provenance of bytes buffered during a handshake. See report:
        // a public server TLS early-data disable switch is missing in iroh 1.3.0.
        if recv.is_0rtt() {
            return Err(NetError::EarlyData);
        }
        let permit = connection.stream()?;
        Ok(Self {
            send,
            recv,
            connection,
            _permit: permit,
            reads,
            max_frame: if pair { PAIR_MAX_FRAME } else { MAX_FRAME },
            deadline: if pair {
                PAIR_TIMEOUT
            } else {
                HANDSHAKE_TIMEOUT
            },
        })
    }
    pub fn enter_live_phase(&mut self) {
        if self.max_frame != PAIR_MAX_FRAME {
            self.deadline = IDLE_TIMEOUT;
        }
    }
    pub async fn read(&mut self, allowed_types: &[u8]) -> Result<(u8, Vec<u8>), NetError> {
        timeout(self.deadline, async {
            self.connection.ingress(10)?;
            self.reads.fetch_add(1, Ordering::Relaxed);
            let mut bytes = [0; 10];
            self.recv
                .read_exact(&mut bytes)
                .await
                .map_err(|_| NetError::Frame)?;
            let header =
                FrameHeaderV1::decode(bytes, allowed_types).map_err(|_| NetError::Frame)?;
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
    pub async fn write(&mut self, frame_type: u8, payload: &[u8]) -> Result<(), NetError> {
        if payload.len() > self.max_frame {
            return Err(NetError::Frame);
        }
        let header = FrameHeaderV1 {
            frame_type,
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
