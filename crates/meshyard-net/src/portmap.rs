//! One owned mapper; tests supply events and Tokio's virtual clock, never a router.
use crate::{NetError, PortMapping};
use iroh::{Endpoint, Watcher};
use std::{
    future::Future,
    net::{SocketAddr, SocketAddrV4},
    num::NonZeroU16,
    pin::Pin,
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::watch,
    task::JoinHandle,
    time::{Instant, sleep_until},
};

/// Clock and event source are separate seams: lifecycle tests need neither a
/// router nor wall-clock sleeps. Production uses the runtime's monotonic clock.
trait MappingClock: Send + Sync + 'static {
    fn now(&self) -> Instant;
    fn wait_until(&self, at: Instant) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}
struct TokioClock;
impl MappingClock for TokioClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
    fn wait_until(&self, at: Instant) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(sleep_until(at))
    }
}

pub(crate) enum MappingEvent {
    External(Option<SocketAddrV4>),
    NetworkChanged,
}
pub(crate) trait MappingSource: Send + 'static {
    fn next(&mut self) -> Pin<Box<dyn Future<Output = Option<MappingEvent>> + Send + '_>>;
    fn restart(&mut self);
    fn deactivate(&mut self);
}
struct RouterMapper {
    client: portmapper::Client,
    external: watch::Receiver<Option<SocketAddrV4>>,
    _monitor: netwatch::netmon::Monitor,
    network: n0_watch::NetworkWatcher,
    port: NonZeroU16,
}
// Keep the concrete watcher type local without introducing another direct dependency.
mod n0_watch {
    pub(super) type NetworkWatcher = Box<dyn NetworkChange>;
    pub(super) trait NetworkChange: Send {
        fn changed(&mut self) -> std::pin::Pin<Box<dyn Future<Output = bool> + Send + '_>>;
    }
    impl<W> NetworkChange for W
    where
        W: iroh::Watcher<Value = netwatch::netmon::State> + Send,
    {
        fn changed(&mut self) -> std::pin::Pin<Box<dyn Future<Output = bool> + Send + '_>> {
            Box::pin(async move { self.updated().await.is_ok() })
        }
    }
}
impl RouterMapper {
    async fn new(port: NonZeroU16) -> Result<Self, NetError> {
        let monitor = netwatch::netmon::Monitor::new()
            .await
            .map_err(|_| NetError::MappingMonitor)?;
        let mut network = monitor.interface_state();
        // Consume the initial snapshot before observing actual changes.
        let _ = network.get();
        let client = portmapper::Client::default();
        let external = client.watch_external_address();
        client.update_local_port(port);
        Ok(Self {
            client,
            external,
            network: Box::new(network),
            _monitor: monitor,
            port,
        })
    }
}
impl MappingSource for RouterMapper {
    fn next(&mut self) -> Pin<Box<dyn Future<Output = Option<MappingEvent>> + Send + '_>> {
        Box::pin(async move {
            tokio::select! {
                biased;
                // Invalidate the old generation before considering a queued grant.
                ok = self.network.changed() => ok.then_some(MappingEvent::NetworkChanged),
                changed = self.external.changed() => {
                    changed.ok()?;
                    Some(MappingEvent::External(*self.external.borrow_and_update()))
                }
            }
        })
    }
    fn restart(&mut self) {
        // Fresh client/watcher excludes a stale grant queued by the old network.
        self.client.deactivate();
        self.client = portmapper::Client::default();
        self.external = self.client.watch_external_address();
        self.client.update_local_port(self.port);
    }
    fn deactivate(&mut self) {
        self.client.deactivate();
    }
}

pub(crate) trait AddressSink: Send + Sync + 'static {
    fn add(&self, addr: SocketAddr) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
    fn remove(&self, addr: SocketAddr) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}
impl AddressSink for Endpoint {
    fn add(&self, addr: SocketAddr) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(self.add_external_addr(addr))
    }
    fn remove(&self, addr: SocketAddr) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            self.remove_external_addr(&addr).await;
        })
    }
}
pub(crate) struct MapperTask {
    stop: watch::Sender<bool>,
    task: JoinHandle<()>,
}
impl MapperTask {
    pub(crate) async fn router(
        endpoint: Endpoint,
        state: watch::Sender<PortMapping>,
    ) -> Result<Self, NetError> {
        let port = endpoint
            .bound_sockets()
            .iter()
            .find(|a| a.is_ipv4())
            .and_then(|a| NonZeroU16::new(a.port()))
            .ok_or(NetError::MappingPort)?;
        let source = RouterMapper::new(port).await?;
        Ok(Self::spawn(source, Arc::new(endpoint), state))
    }
    pub(crate) fn spawn(
        source: impl MappingSource,
        sink: Arc<dyn AddressSink>,
        state: watch::Sender<PortMapping>,
    ) -> Self {
        Self::spawn_with_clock(source, sink, state, Arc::new(TokioClock))
    }
    fn spawn_with_clock(
        source: impl MappingSource,
        sink: Arc<dyn AddressSink>,
        state: watch::Sender<PortMapping>,
        clock: Arc<dyn MappingClock>,
    ) -> Self {
        let (stop, rx) = watch::channel(false);
        let task = tokio::spawn(run(source, sink, state, rx, clock));
        Self { stop, task }
    }
    pub(crate) async fn shutdown(&mut self) {
        let _ = self.stop.send(true);
        let _ = (&mut self.task).await;
    }
}
impl Drop for MapperTask {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}

async fn run(
    mut source: impl MappingSource,
    sink: Arc<dyn AddressSink>,
    state: watch::Sender<PortMapping>,
    mut stop: watch::Receiver<bool>,
    clock: Arc<dyn MappingClock>,
) {
    let mut advertised = None;
    let mut deadline = Some(clock.now() + Duration::from_secs(10));
    state.send_replace(PortMapping::Probing);
    loop {
        tokio::select! {
            biased;
            _ = stop.changed() => break,
            event = source.next() => {
                // Remove first, including expiry and network transitions.
                let previous = advertised.take();
                if let Some(old) = previous { sink.remove(SocketAddr::V4(old)).await; }
                match event {
                    Some(MappingEvent::External(Some(addr))) => {
                        sink.add(SocketAddr::V4(addr)).await;
                        advertised = Some(addr);
                        deadline = None;
                        state.send_replace(PortMapping::Mapped { external: addr });
                    }
                    Some(MappingEvent::External(None)) => {
                        // Repeated empty observations do not postpone the initial
                        // 10-second observation window indefinitely.
                        if previous.is_some() {
                            deadline = Some(clock.now() + Duration::from_secs(10));
                            state.send_replace(PortMapping::Probing);
                        }
                    }
                    Some(MappingEvent::NetworkChanged) => {
                        source.restart();
                        deadline = Some(clock.now() + Duration::from_secs(10));
                        state.send_replace(PortMapping::Probing);
                    }
                    None => {
                        // A closed observer proves no refusal. Keep the observation honest.
                        state.send_replace(PortMapping::NotObserved { after_secs: 0 });
                        break;
                    }
                }
            }
            _ = async { match deadline { Some(at) => clock.wait_until(at).await, None => std::future::pending().await } } => {
                deadline = None;
                state.send_replace(PortMapping::NotObserved { after_secs: 10 });
            }
        }
    }
    if let Some(old) = advertised {
        sink.remove(SocketAddr::V4(old)).await;
    }
    source.deactivate();
    state.send_replace(PortMapping::Disabled);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::limits::lock;
    use std::{
        collections::BTreeSet,
        net::Ipv4Addr,
        sync::{
            Mutex,
            atomic::{AtomicUsize, Ordering},
        },
    };
    use tokio::sync::mpsc;
    struct Injected {
        rx: mpsc::Receiver<MappingEvent>,
        restarts: Arc<AtomicUsize>,
        stops: Arc<AtomicUsize>,
    }
    impl MappingSource for Injected {
        fn next(&mut self) -> Pin<Box<dyn Future<Output = Option<MappingEvent>> + Send + '_>> {
            Box::pin(self.rx.recv())
        }
        fn restart(&mut self) {
            self.restarts.fetch_add(1, Ordering::SeqCst);
        }
        fn deactivate(&mut self) {
            self.stops.fetch_add(1, Ordering::SeqCst);
        }
    }
    #[derive(Default)]
    struct Sink {
        current: Mutex<BTreeSet<SocketAddr>>,
        actions: Mutex<Vec<(bool, SocketAddr)>>,
    }
    impl AddressSink for Sink {
        fn add(&self, addr: SocketAddr) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
            Box::pin(async move {
                assert!(
                    lock(&self.current).is_empty(),
                    "old mapping must be removed before replacement"
                );
                lock(&self.current).insert(addr);
                lock(&self.actions).push((true, addr));
            })
        }
        fn remove(&self, addr: SocketAddr) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
            Box::pin(async move {
                assert!(lock(&self.current).remove(&addr));
                lock(&self.actions).push((false, addr));
            })
        }
    }
    struct InjectedClock(AtomicUsize);
    impl MappingClock for InjectedClock {
        fn now(&self) -> Instant {
            Instant::now()
        }
        fn wait_until(&self, at: Instant) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(sleep_until(at))
        }
    }
    #[tokio::test(start_paused = true)]
    async fn mapping_change_expiry_network_shutdown_and_honest_ten_seconds() {
        let (tx, rx) = mpsc::channel(8);
        let restarts = Arc::new(AtomicUsize::new(0));
        let stops = Arc::new(AtomicUsize::new(0));
        let source = Injected {
            rx,
            restarts: restarts.clone(),
            stops: stops.clone(),
        };
        let sink = Arc::new(Sink::default());
        let (state_tx, mut state) = watch::channel(PortMapping::Disabled);
        let clock = Arc::new(InjectedClock(AtomicUsize::new(0)));
        let mut task = MapperTask::spawn_with_clock(source, sink.clone(), state_tx, clock.clone());
        state
            .wait_for(|s| *s == PortMapping::Probing)
            .await
            .unwrap();
        tokio::time::advance(Duration::from_secs(9)).await;
        assert_eq!(*state.borrow(), PortMapping::Probing);
        tx.send(MappingEvent::External(None)).await.unwrap();
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(1)).await;
        state
            .wait_for(|s| *s == PortMapping::NotObserved { after_secs: 10 })
            .await
            .unwrap();
        let a = SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 1), 1001);
        let b = SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 2), 1002);
        for addr in [a, b] {
            tx.send(MappingEvent::External(Some(addr))).await.unwrap();
            state
                .wait_for(|s| *s == PortMapping::Mapped { external: addr })
                .await
                .unwrap();
            assert_eq!(*lock(&sink.current), BTreeSet::from([SocketAddr::V4(addr)]));
        }
        tx.send(MappingEvent::External(None)).await.unwrap();
        state
            .wait_for(|s| *s == PortMapping::Probing)
            .await
            .unwrap();
        assert!(lock(&sink.current).is_empty());
        tx.send(MappingEvent::External(Some(a))).await.unwrap();
        state
            .wait_for(|s| *s == PortMapping::Mapped { external: a })
            .await
            .unwrap();
        tx.send(MappingEvent::NetworkChanged).await.unwrap();
        state
            .wait_for(|s| *s == PortMapping::Probing)
            .await
            .unwrap();
        assert!(lock(&sink.current).is_empty());
        assert_eq!(restarts.load(Ordering::SeqCst), 1);
        tx.send(MappingEvent::External(Some(b))).await.unwrap();
        state
            .wait_for(|s| *s == PortMapping::Mapped { external: b })
            .await
            .unwrap();
        task.shutdown().await;
        assert!(lock(&sink.current).is_empty());
        assert_eq!(*state.borrow(), PortMapping::Disabled);
        assert_eq!(stops.load(Ordering::SeqCst), 1);
        assert_eq!(lock(&sink.actions).len(), 8);
        assert!(clock.0.load(Ordering::SeqCst) > 0);
    }
    #[tokio::test(start_paused = true)]
    async fn drop_signals_cleanup_without_a_router() {
        let (tx, rx) = mpsc::channel(1);
        let stops = Arc::new(AtomicUsize::new(0));
        let sink = Arc::new(Sink::default());
        let source = Injected {
            rx,
            restarts: Arc::new(AtomicUsize::new(0)),
            stops: stops.clone(),
        };
        let (state_tx, mut state) = watch::channel(PortMapping::Disabled);
        let task = MapperTask::spawn(source, sink.clone(), state_tx);
        let a = SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 3), 1003);
        tx.send(MappingEvent::External(Some(a))).await.unwrap();
        state
            .wait_for(|s| *s == PortMapping::Mapped { external: a })
            .await
            .unwrap();
        drop(task);
        state
            .wait_for(|s| *s == PortMapping::Disabled)
            .await
            .unwrap();
        assert!(lock(&sink.current).is_empty());
        assert_eq!(stops.load(Ordering::SeqCst), 1);
    }
}
