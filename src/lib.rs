#[cxx::bridge(namespace = "lk")]
mod ffi {

    unsafe extern "C++" {
        include!("peer_connection_factory.h");

        type PeerConnectionFactory;
        fn CreatePeerConnectionFactory() -> UniquePtr<PeerConnectionFactory>;
    }
}

#[no_mangle]
extern "C" fn test_rust() {
    println!("Called test_rust");
    let _factory = ffi::CreatePeerConnectionFactory();
}
