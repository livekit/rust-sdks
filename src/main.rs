use std::thread::sleep;
use std::time;

#[cxx::bridge(namespace = "lk")]
mod ffi {

    unsafe extern "C++" {
        include!("peer_connection_factory.h");

        type PeerConnectionFactory;
        fn CreatePeerConnectionFactory() -> UniquePtr<PeerConnectionFactory>;
    }
}


fn main() {
    println!("Hello, world!");

    let factory = ffi::CreatePeerConnectionFactory();
}
