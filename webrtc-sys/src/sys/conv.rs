use crate::{
    peer_connection_factory::IceServer, rtp_parameters::RtpEncodingParameters,
    rtp_transceiver::RtpTransceiverInit, sys,
};

// Helper function to convert Vec<IceServer> to *mut sys::lkIceServer
pub fn toLKIceServers(servers: &Vec<IceServer>) -> *mut sys::lkIceServer {
    if servers.is_empty() {
        return std::ptr::null_mut();
    }
    // Allocate a Vec of sys::lkIceServer
    let mut native_servers: Vec<sys::lkIceServer> = servers
        .iter()
        .map(|s| {
            sys::lkIceServer {
                urlsCount: s.urls.len() as i32,
                urls: {
                    let mut url_ptrs: Vec<*const i8> = s
                        .urls
                        .iter()
                        .map(|url| {
                            std::ffi::CString::new(url.clone()).unwrap().into_raw() as *const i8
                        })
                        .collect();
                    let ptr = url_ptrs.as_mut_ptr();
                    std::mem::forget(url_ptrs); // Prevent deallocation
                    ptr
                },
                username: std::ffi::CString::new(s.username.clone()).unwrap().into_raw(),
                password: std::ffi::CString::new(s.password.clone()).unwrap().into_raw(),
            }
        })
        .collect();

    let ptr = native_servers.as_mut_ptr();
    std::mem::forget(native_servers); // Prevent deallocation
    ptr
}

pub fn toLkRtpEncodingParameters(encoding: &RtpEncodingParameters) -> sys::lkRtpEncodingParameters {
    sys::lkRtpEncodingParameters {
        has_payload_type: false,
        rid: if let Some(rid) = &encoding.rid {
            let c_string = std::ffi::CString::new(rid.as_str()).unwrap();
            unsafe { sys::lkCreateString(c_string.as_ptr()) }
        } else {
            std::ptr::null_mut()
        },
        active: encoding.active,
        has_max_bitrate_bps: encoding.max_bitrate.is_some(),
        max_bitrate_bps: encoding.max_bitrate.unwrap_or(0) as u32,
        has_min_bitrate_bps: false,
        has_max_framerate: encoding.max_framerate.is_some(),
        max_framerate: encoding.max_framerate.unwrap_or(0.0),
    }
}

pub fn toLkRtpTransceiverInit(init: &RtpTransceiverInit) -> sys::lkRtpTransceiverInit {
    let ffi = unsafe { sys::lkRtpTransceiverInitCreate() };
    unsafe {
        sys::lkRtpTransceiverInitSetDirection(
            ffi,
            sys::lkRtpTransceiverDirection::from(init.direction.clone()),
        );

        let stream_ids_vec = init
            .stream_ids
            .iter()
            .map(|id| {
                let c_string = std::ffi::CString::new(id.as_str()).unwrap();
                sys::lkCreateString(c_string.as_ptr())
            })
            .collect::<Vec<*mut sys::lkString>>();

        sys::lkRtpTransceiverInitSetStreamIds(
            ffi,
            stream_ids_vec.as_ptr(),
            stream_ids_vec.len() as u32,
        );

        let encoding_params_vec = init
            .send_encodings
            .iter()
            .map(|enc| toLkRtpEncodingParameters(enc))
            .collect::<Vec<sys::lkRtpEncodingParameters>>();

        sys::lkRtpTransceiverInitSetSendEncodings(
            ffi,
            encoding_params_vec.as_ptr(),
            encoding_params_vec.len() as u32,
        );
    }
    ffi
}
