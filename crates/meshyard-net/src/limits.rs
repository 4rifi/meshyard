use crate::NetError;
use iroh::EndpointId;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard},
};
use tokio::time::Instant;

pub(crate) const MAX_CONNECTIONS: usize = 4;
pub(crate) const MAX_STREAMS: usize = 16;
pub(crate) const MAX_TOTAL: usize = 256;
const BURST: f64 = (4 * 1024 * 1024) as f64;
const RATE: f64 = (1024 * 1024) as f64;

pub(crate) fn lock<T>(value: &Mutex<T>) -> MutexGuard<'_, T> {
    value
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

struct Device {
    connections: usize,
    streams: usize,
    tokens: f64,
    updated: Instant,
}
#[derive(Default)]
pub(crate) struct Limits(Mutex<BTreeMap<EndpointId, Device>>);
pub(crate) struct ConnectionPermit {
    limits: Arc<Limits>,
    peer: EndpointId,
}
pub(crate) struct StreamPermit {
    // Retain the device's rate bucket until every stream has gone away.
    _connection: Arc<ConnectionPermit>,
}
impl Limits {
    pub(crate) fn connection(
        self: &Arc<Self>,
        peer: EndpointId,
    ) -> Result<Arc<ConnectionPermit>, NetError> {
        let mut devices = lock(&self.0);
        // Retain exhausted buckets across rapid reconnects. Four seconds fully refills
        // any idle bucket, after which eviction cannot grant extra ingress.
        let now = Instant::now();
        devices.retain(|_, d| {
            d.connections > 0 || now.duration_since(d.updated).as_secs_f64() < BURST / RATE
        });
        if !devices.contains_key(&peer) && devices.len() >= MAX_TOTAL {
            return Err(NetError::Limit);
        }
        if devices.values().map(|d| d.connections).sum::<usize>() >= MAX_TOTAL {
            return Err(NetError::Limit);
        }
        let d = devices.entry(peer).or_insert_with(|| Device {
            connections: 0,
            streams: 0,
            tokens: BURST,
            updated: Instant::now(),
        });
        if d.connections >= MAX_CONNECTIONS {
            return Err(NetError::Limit);
        }
        d.connections += 1;
        Ok(Arc::new(ConnectionPermit {
            limits: self.clone(),
            peer,
        }))
    }
}
impl ConnectionPermit {
    pub(crate) fn stream(self: &Arc<Self>) -> Result<StreamPermit, NetError> {
        let mut devices = lock(&self.limits.0);
        let d = devices.get_mut(&self.peer).ok_or(NetError::Closed)?;
        if d.streams >= MAX_STREAMS {
            return Err(NetError::Limit);
        }
        d.streams += 1;
        Ok(StreamPermit {
            _connection: self.clone(),
        })
    }
    pub(crate) fn ingress(&self, bytes: usize) -> Result<(), NetError> {
        let mut devices = lock(&self.limits.0);
        let d = devices.get_mut(&self.peer).ok_or(NetError::Closed)?;
        let now = Instant::now();
        d.tokens = (d.tokens + now.duration_since(d.updated).as_secs_f64() * RATE).min(BURST);
        d.updated = now;
        if bytes as f64 > d.tokens {
            return Err(NetError::RateLimited);
        }
        d.tokens -= bytes as f64;
        Ok(())
    }
}
impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        let mut devices = lock(&self.limits.0);
        if let Some(d) = devices.get_mut(&self.peer) {
            d.connections -= 1;
        }
    }
}
impl Drop for StreamPermit {
    fn drop(&mut self) {
        if let Some(d) = lock(&self._connection.limits.0).get_mut(&self._connection.peer) {
            d.streams -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    #[tokio::test(start_paused = true)]
    async fn device_caps_and_shared_ingress_recover() {
        let limits = Arc::new(Limits::default());
        let peer = iroh::SecretKey::from_bytes(&[7; 32]).public();
        let connections: Vec<_> = (0..4).map(|_| limits.connection(peer).unwrap()).collect();
        assert!(matches!(limits.connection(peer), Err(NetError::Limit)));
        let mut streams: Vec<_> = (0..16)
            .map(|i| connections[i % 4].stream().unwrap())
            .collect();
        assert!(matches!(connections[1].stream(), Err(NetError::Limit)));
        streams.pop();
        assert!(connections[0].stream().is_ok());
        connections[0].ingress(4 * 1024 * 1024).unwrap();
        assert!(matches!(
            connections[1].ingress(1),
            Err(NetError::RateLimited)
        ));
        tokio::time::advance(std::time::Duration::from_secs(1)).await;
        connections[1].ingress(1024 * 1024).unwrap();
        drop(streams);
        drop(connections);
        let reconnected = limits.connection(peer).unwrap();
        assert!(matches!(reconnected.ingress(1), Err(NetError::RateLimited)));
        drop(reconnected);
        tokio::time::advance(std::time::Duration::from_secs(4)).await;
        limits
            .connection(peer)
            .unwrap()
            .ingress(4 * 1024 * 1024)
            .unwrap();
    }
}
