use crate::{peer_connection_factory::IceServer, rtp_parameters::*, sys}; // Ensure RtpTransceiverInit is imported

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
        clock_rate: unsafe {
            let cr = sys::lkRtpCodecCapabilityGetClockRate(ffi) as u64;
            if cr > 0 {
                Some(sys::lkRtpCodecCapabilityGetClockRate(ffi) as u64)
            } else {
                None
            }
        },
        channels: unsafe {
            let ch = sys::lkRtpCodecCapabilityGetChannels(ffi) as u16;
            if ch > 0 {
                Some(sys::lkRtpCodecCapabilityGetChannels(ffi) as u16)
            } else {
                None
            }
        },
        sdp_fmtp_line: unsafe {
            if sys::lkRtpCodecCapabilityHasSdpFmtpLine(ffi) {
                let ptr = sys::lkRtpCodecCapabilityGetSdpFmtpLine(ffi);
                Some(sys::RefCountedString::from_native(ptr).as_str())
            } else {
                None
            }
        },
        preferred_payload_type: unsafe {
            if sys::lkRtpCodecCapabilityHasPreferredPayloadType(ffi) {
                Some(sys::lkRtpCodecCapabilityGetPreferredPayloadType(ffi) as u8)
            } else {
                None
            }
        },
        rtcp_feedback: unsafe {
            let vec_ptr = sys::lkRtpCodecCapabilityGetRtcpFeedbacks(ffi);
            let feedback_vec = sys::RefCountedVector::from_native_vec(vec_ptr);
            let mut items = Vec::new();
            for i in 0..feedback_vec.vec.len() as isize {
                let lk_feedback_type = sys::lkRtcpFeedbackGetType(
                    feedback_vec.vec[i as usize].as_ptr() as *mut sys::lkRtcpFeedback,
                );
                let has_feedback_message_type = sys::lkRtcpFeedbackHasMessageType(
                    feedback_vec.vec[i as usize].as_ptr() as *mut sys::lkRtcpFeedback,
                );
                let lk_message_type = sys::lkRtcpFeedbackGetMessageType(
                    feedback_vec.vec[i as usize].as_ptr() as *mut sys::lkRtcpFeedback,
                );
                items.push(RtcpFeedback {
                    feedback_type: lk_feedback_type.into(),
                    has_message_type: has_feedback_message_type,
                    message_type: lk_message_type.into(),
                });
            }
            items
        },
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
            let ptr: *mut std::ffi::c_void = sys::lkRtcpParametersGetCname(ffi);
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

pub fn RtpTransceiverInitToNative(
    init: RtpTransceiverInit,
) -> sys::RefCounted<sys::lkRtpTransceiverInit> {
    unsafe {
        let lk_init = sys::lkRtpTransceiverInitCreate();
        sys::lkRtpTransceiverInitSetDirection(lk_init, init.direction.into());
        let mut lk_stream_ids_vec = sys::RefCountedVector::new();
        for stream_id in init.stream_ids.iter() {
            let c_stream_id = sys::RefCountedString::new(stream_id);
            lk_stream_ids_vec.push_back(c_stream_id.ffi.clone());
        }
        sys::lkRtpTransceiverInitSetStreamIds(lk_init, lk_stream_ids_vec.ffi.as_ptr());

        let mut lk_send_encodings_vec = sys::RefCountedVector::new();
        for encoding in init.send_encodings.iter() {
            let ptr = sys::lkRtpEncodingParametersCreate();
            let c_encoding = sys::RefCounted::from_raw(ptr);

            sys::lkRtpEncodingParametersSetActive(c_encoding.as_ptr(), encoding.active);
            if let Some(max_bitrate) = encoding.max_bitrate {
                sys::lkRtpEncodingParametersSetMaxBitrateBps(
                    c_encoding.as_ptr(),
                    max_bitrate as i64,
                );
            }

            if let Some(min_bitrate) = encoding.min_bitrate {
                sys::lkRtpEncodingParametersSetMinBitrateBps(
                    c_encoding.as_ptr(),
                    min_bitrate as i64,
                );
            }

            if let Some(max_framerate) = encoding.max_framerate {
                sys::lkRtpEncodingParametersSetMaxFramerate(c_encoding.as_ptr(), max_framerate);
            }

            if let Some(scalability_mode) = &encoding.scalability_mode {
                sys::lkRtpEncodingParametersSetScalabilityMode(
                    c_encoding.as_ptr(),
                    std::ffi::CString::new(scalability_mode.as_str()).unwrap().as_ptr(),
                );
            }

            if let Some(scale_resolution_down_by) = encoding.scale_resolution_down_by {
                sys::lkRtpEncodingParametersSetScaleResolutionDownBy(
                    c_encoding.as_ptr(),
                    scale_resolution_down_by,
                );
            }

            sys::lkRtpEncodingParametersSetRid(
                c_encoding.as_ptr(),
                std::ffi::CString::new(encoding.rid.as_str()).unwrap().as_ptr(),
            );

            lk_send_encodings_vec.push_back(c_encoding);
        }

        sys::lkRtpTransceiverInitSetSendEncodingsdings(lk_init, lk_send_encodings_vec.ffi.as_ptr());

        sys::RefCounted::from_raw(lk_init)
    }
}

pub fn RtpParametersToNative(params: RtpParameters) -> sys::RefCounted<sys::lkRtpParameters> {
    unsafe {
        let lk_params = sys::lkRtpParametersCreate();

        let mut lk_codecs_vec = sys::RefCountedVector::new();
        for codec in params.codecs.iter() {
            let ptr = sys::lkRtpCodecParametersCreate();
            let c_codec = sys::RefCounted::from_raw(ptr);

            sys::lkRtpCodecParametersSetPayloadType(
                c_codec.as_ptr(),
                codec.payload_type.try_into().unwrap(),
            );
            sys::lkRtpCodecParametersSetMimeType(
                c_codec.as_ptr(),
                std::ffi::CString::new(codec.mime_type.as_str()).unwrap().as_ptr(),
            );
            if let Some(clock_rate) = codec.clock_rate {
                sys::lkRtpCodecParametersSetClockRate(
                    c_codec.as_ptr(),
                    clock_rate.try_into().unwrap(),
                );
            }
            if let Some(channels) = codec.channels {
                sys::lkRtpCodecParametersSetChannels(
                    c_codec.as_ptr(),
                    channels.try_into().unwrap(),
                );
            }

            lk_codecs_vec.push_back(c_codec);
        }
        sys::lkRtpParametersSetCodecs(lk_params, lk_codecs_vec.ffi.as_ptr());

        let rtcp_ptr = sys::lkRtcpParametersCreate();
        sys::lkRtcpParametersSetCname(
            rtcp_ptr,
            std::ffi::CString::new(params.rtcp.cname.as_str()).unwrap().as_ptr(),
        );
        sys::lkRtcpParametersSetReducedSize(rtcp_ptr, params.rtcp.reduced_size);
        sys::lkRtpParametersSetRtcp(lk_params, rtcp_ptr);

        let lk_header_extensions_vec = sys::RefCountedVector::new();
        for header_extension in params.header_extensions.iter() {
            let ptr = sys::lkRtpHeaderExtensionParametersCreate();
            let c_header_extension = sys::RefCounted::from_raw(ptr);

            sys::lkRtpHeaderExtensionParametersSetUri(
                c_header_extension.as_ptr(),
                std::ffi::CString::new(header_extension.uri.as_str()).unwrap().as_ptr(),
            );
            sys::lkRtpHeaderExtensionParametersSetId(
                c_header_extension.as_ptr(),
                header_extension.id as u32,
            );
            sys::lkRtpHeaderExtensionParametersSetEncrypted(
                c_header_extension.as_ptr(),
                header_extension.encrypted,
            );
        }
        sys::lkRtpParametersSetHeaderExtensions(lk_params, lk_header_extensions_vec.ffi.as_ptr());

        sys::RefCounted::from_raw(lk_params)
    }
}
