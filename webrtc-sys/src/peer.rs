use std::{os::raw::c_void, ptr::null};

use crate::sys::{self, lkPeerObserver};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum IceTransportType {
    None,
    Relay,
    All,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ContinualGatheringPolicy {
    GatherContinually,
    GatherOnce,
}

#[derive(Debug, Clone)]
pub struct IceServer {
    pub urls: Vec<String>,
    pub username: String,
    pub credential: String,
}

#[derive(Debug, Clone)]
pub struct RtcConfiguration {
    pub ice_servers: Vec<IceServer>,
    pub ice_transport_type: IceTransportType,
    pub continual_gathering_policy: ContinualGatheringPolicy,
}

#[derive(Debug)]
pub struct PeerFactory {
    factory: sys::RefCounted<sys::lkPeerFactory>,
}

impl Default for PeerFactory {
    fn default() -> Self {
        unsafe {
            let factory = sys::lkCreatePeerFactory();
            Self { factory: sys::RefCounted::from_raw(factory) }
        }
    }
}

impl PeerFactory {
    pub fn create_peer(&self, config: &RtcConfiguration, observer: lkPeerObserver, user_data: c_void) -> Peer {
        unsafe {
            let lk_config = sys::lkRtcConfiguration();
            lk_config.iceServersCount = config.ice_servers.len() as u32;
            let peer = sys::lkCreatePeer(self.factory, lk_config, observer, user_data);   
            Peer { peer: sys::RefCounted::from_raw(peer) }
        }
    }
}

pub struct Peer {
    peer: sys::RefCounted<sys::lkPeer>,
}

impl Peer {}
