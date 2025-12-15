use crate::{peer_connection_factory::IceServer, rtp_parameters::*, sys};

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

pub fn RtpCodecCapabilityFromNative(ffi: *mut sys::lkRtpCodecCapability) -> RtpCodecCapability {
    RtpCodecCapability {
        mime_type: unsafe {
            let ptr = sys::lkRtpCodecCapabilityGetMimeType(ffi);
            sys::RefCountedString::from_native(ptr).as_str()
        },
        clock_rate: Some(unsafe { sys::lkRtpCodecCapabilityGetClockRate(ffi) as u64 }),
        channels: Some(unsafe { sys::lkRtpCodecCapabilityGetChannels(ffi) as u16 }),
        sdp_fmtp_line: Some(unsafe {
            let ptr = sys::lkRtpCodecCapabilityGetSdpFmtpLine(ffi);
            sys::RefCountedString::from_native(ptr).as_str()
        }),
    }
}

pub fn RtpHeaderExtensionCapabilityFromNative(
    ffi: *mut sys::lkRtpHeaderExtensionCapability,
) -> RtpHeaderExtensionCapability {
    RtpHeaderExtensionCapability {
        uri: unsafe {
            let ptr = sys::lkRtpHeaderExtensionCapabilityGetUri(ffi);
            sys::RefCountedString::from_native(ptr).as_str()
        },
        direction: unsafe { sys::lkRtpHeaderExtensionCapabilityGetDirection(ffi).into() },
    }
}

pub fn RtpCapabilitiesFromNative(ffi: sys::RefCounted<sys::lkRtpCapabilities>) -> RtpCapabilities {
    let mut caps = RtpCapabilities { codecs: vec![], header_extensions: vec![] };
    {
        let lk_codecs_vec = unsafe { sys::lkRtpCapabilitiesGetCodecs(ffi.as_ptr()) };
        let codecs_ptrs = sys::RefCountedVector::from_native_vec(lk_codecs_vec);
        if !codecs_ptrs.vec.is_empty() {
            let mut items = Vec::new();
            for i in 0..codecs_ptrs.vec.len() as isize {
                items.push(RtpCodecCapabilityFromNative(
                    codecs_ptrs.vec[i as usize].as_ptr() as *mut sys::lkRtpCodecCapability
                ));
            }
            caps.codecs = items;
        }
    }
    {
        let lk_vec = unsafe { sys::lkRtpCapabilitiesGetHeaderExtensions(ffi.as_ptr()) };
        let header_extensions_ptrs = sys::RefCountedVector::from_native_vec(lk_vec);
        if !header_extensions_ptrs.vec.is_empty() {
            let mut items = Vec::new();
            for i in 0..header_extensions_ptrs.vec.len() as isize {
                items.push(RtpHeaderExtensionCapabilityFromNative(
                    header_extensions_ptrs.vec[i as usize].as_ptr()
                        as *mut sys::lkRtpCodecCapability,
                ));
            }
            caps.header_extensions = items;
        }
    }
    caps
}

pub fn RtcpParametersFromNative(ffi: *mut sys::lkRtcpParameters) -> RtcpParameters {
    RtcpParameters {
        cname: unsafe {
            let ptr = sys::lkRtcpParametersGetCname(ffi);
            sys::RefCountedString::from_native(ptr).as_str()
        },
        reduced_size: unsafe { sys::lkRtcpParametersGetReducedSize(ffi) },
    }
}

pub fn RtpCodecParametersFromNative(ffi: *mut sys::lkRtpCodecParameters) -> RtpCodecParameters {
    RtpCodecParameters {
        payload_type: unsafe { sys::lkRtpCodecParametersGetPayloadType(ffi) as u8 },
        mime_type: unsafe {
            let ptr = sys::lkRtpCodecParametersGetMimeType(ffi);
            sys::RefCountedString::from_native(ptr).as_str()
        },
        clock_rate: Some(unsafe { sys::lkRtpCodecParametersGetClockRate(ffi) as u64 }),
        channels: Some(unsafe { sys::lkRtpCodecParametersGetChannels(ffi) as u16 }),
    }
}

pub fn RtpHeaderExtensionParametersFromNative(
    ffi: *mut sys::lkRtpHeaderExtensionParameters,
) -> RtpHeaderExtensionParameters {
    RtpHeaderExtensionParameters {
        uri: unsafe {
            let ptr = sys::lkRtpHeaderExtensionParametersGetUri(ffi);
            sys::RefCountedString::from_native(ptr).as_str()
        },
        id: unsafe { sys::lkRtpHeaderExtensionParametersGetId(ffi) as i32 },
        encrypted: unsafe { sys::lkRtpHeaderExtensionParametersGetEncrypted(ffi) },
    }
}

pub fn RtpParametersFromNative(ffi: sys::RefCounted<sys::lkRtpParameters>) -> RtpParameters {
    let mut params = RtpParameters {
        codecs: vec![],
        header_extensions: vec![],
        rtcp: RtcpParameters::default(),
    };
    {
        let lk_codecs_vec = unsafe { sys::lkRtpParametersGetCodecs(ffi.as_ptr()) };
        let codecs_ptrs = sys::RefCountedVector::from_native_vec(lk_codecs_vec);
        if !codecs_ptrs.vec.is_empty() {
            let mut items = Vec::new();
            for i in 0..codecs_ptrs.vec.len() as isize {
                items.push(RtpCodecParametersFromNative(
                    codecs_ptrs.vec[i as usize].as_ptr() as *mut sys::lkRtpCodecParameters
                ));
            }
            params.codecs = items;
        }
    }
    {
        let rtcp_ptr = unsafe { sys::lkRtpParametersGetRtcp(ffi.as_ptr()) };
        if !rtcp_ptr.is_null() {
            params.rtcp = RtcpParametersFromNative(rtcp_ptr);
        }
    }

    {
        let lk_vec = unsafe { sys::lkRtpParametersGetHeaderExtensions(ffi.as_ptr()) };
        let header_extensions_ptrs = sys::RefCountedVector::from_native_vec(lk_vec);
        if !header_extensions_ptrs.vec.is_empty() {
            let mut items = Vec::new();
            for i in 0..header_extensions_ptrs.vec.len() as isize {
                items.push(RtpHeaderExtensionParametersFromNative(
                    header_extensions_ptrs.vec[i as usize].as_ptr()
                        as *mut sys::lkRtpHeaderExtensionParameters,
                ));
            }
            params.header_extensions = items;
        }
    }
    params
}
