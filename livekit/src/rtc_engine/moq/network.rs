// Copyright 2026 LiveKit, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    io,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket},
    time::Duration,
};

/// The source selected by the OS for this particular SFU, including an IPv6 scope ID.
/// Connecting a fresh UDP socket queries the route without sending a packet.
fn route_source(peer: SocketAddr) -> io::Result<SocketAddr> {
    let wildcard = if peer.is_ipv4() {
        SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0))
    } else {
        SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0))
    };
    let probe = UdpSocket::bind(wildcard)?;
    probe.connect(peer)?;
    let mut source = probe.local_addr()?;
    source.set_port(0);
    Ok(source)
}

/// One room owns one Quinn endpoint. Rebinding it preserves its live streams.
#[derive(Clone)]
pub(super) struct NetworkPath {
    endpoint: quinn::Endpoint,
    peer: SocketAddr,
    source: Option<SocketAddr>,
}

impl NetworkPath {
    /// Capture the initial route before starting the handshake.
    pub(super) fn new(endpoint: quinn::Endpoint, peer: SocketAddr) -> Self {
        Self { endpoint, peer, source: route_source(peer).ok() }
    }

    /// Follow changes to the OS route even while the MoQ actor is awaiting a response.
    pub(super) async fn monitor(mut self) {
        let mut tick = tokio::time::interval(Duration::from_millis(500));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            if let Err(err) = self.observe(route_source(self.peer).ok()) {
                log::debug!("MoQ socket rebind deferred: {err}");
            }
        }
    }

    /// Keep the usable socket across a route gap. Retry a failed bind on the next observation.
    fn observe(&mut self, source: Option<SocketAddr>) -> io::Result<()> {
        if self.source != source {
            if let Some(addr) = source {
                self.rebind(addr)?;
            }
            self.source = source;
        }
        Ok(())
    }

    /// Install an unconnected socket on the new path. Quinn retains the old socket on error.
    fn rebind(&self, source: SocketAddr) -> io::Result<()> {
        let socket = UdpSocket::bind(source)?;
        socket.set_nonblocking(true)?;
        self.endpoint.rebind(socket)
    }

    /// Exercise the socket change on a chosen local address without changing the machine's routes.
    #[cfg(feature = "__lk-e2e-test")]
    pub(super) fn simulate_handoff(&self, source: Option<std::net::IpAddr>) -> io::Result<()> {
        let addr = match source {
            Some(ip) => SocketAddr::new(ip, 0),
            None => route_source(self.peer)?,
        };
        self.rebind(addr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn monitor_rebinds_to_the_route_selected_by_the_os() {
        let peer = SocketAddr::from((Ipv4Addr::LOCALHOST, 443));
        let endpoint = quinn::Endpoint::client((Ipv4Addr::UNSPECIFIED, 0).into()).unwrap();
        let original = endpoint.local_addr().unwrap();
        let mut path = NetworkPath::new(endpoint.clone(), peer);
        path.source = Some(([192, 0, 2, 1], 0).into());
        let monitor = tokio::spawn(path.monitor());
        tokio::time::timeout(Duration::from_secs(3), async {
            while endpoint.local_addr().unwrap() == original {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(endpoint.local_addr().unwrap().ip(), Ipv4Addr::LOCALHOST);
        monitor.abort();
        assert!(monitor.await.unwrap_err().is_cancelled());
    }

    #[tokio::test]
    async fn route_loss_and_recovery_preserve_the_endpoint() {
        let local = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
        let endpoint = quinn::Endpoint::client(local).unwrap();
        let mut path = NetworkPath::new(endpoint.clone(), (Ipv4Addr::LOCALHOST, 443).into());
        let original = endpoint.local_addr().unwrap();
        assert_eq!(path.source, Some(local));
        path.observe(Some(local)).unwrap();
        path.observe(None).unwrap();
        assert_eq!(endpoint.local_addr().unwrap(), original);

        // An occupied port models a replacement socket that cannot be bound.
        assert!(path.observe(Some(original)).is_err());
        assert_eq!(endpoint.local_addr().unwrap(), original);
        assert_eq!(path.source, None);
        path.observe(Some(local)).unwrap();
        let recovered = endpoint.local_addr().unwrap();
        assert_ne!(recovered, original);
        path.observe(Some(local)).unwrap();
        assert_eq!(endpoint.local_addr().unwrap(), recovered);
    }
}
