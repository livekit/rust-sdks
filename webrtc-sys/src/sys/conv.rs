use crate::{peer_connection_factory::IceServer, sys};

// Helper function to convert Vec<IceServer> to *mut sys::lkIceServer
pub fn toLKIceServers(servers: &Vec<IceServer>) -> *mut sys::lkIceServer {
    if servers.is_empty() {
        return std::ptr::null_mut();
    }
    // Allocate a Vec of sys::lkIceServer
    let mut native_servers: Vec<sys::lkIceServer> = servers.iter().map(|s| {
        sys::lkIceServer {
            urlsCount: s.urls.len() as i32,
            urls: {
                let mut url_ptrs: Vec<*const i8> = s.urls.iter()
                    .map(|url| std::ffi::CString::new(url.clone()).unwrap().into_raw() as *const i8)
                    .collect();
                let ptr = url_ptrs.as_mut_ptr();
                std::mem::forget(url_ptrs); // Prevent deallocation
                ptr
            },
            username: std::ffi::CString::new(s.username.clone()).unwrap().into_raw(),
            password: std::ffi::CString::new(s.password.clone()).unwrap().into_raw(),
        }
    }).collect();

    let ptr = native_servers.as_mut_ptr();
    std::mem::forget(native_servers); // Prevent deallocation
    ptr
}
