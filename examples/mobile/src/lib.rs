use lazy_static::lazy_static;
use livekit::{Room, RoomOptions};

struct App {
    async_runtime: tokio::runtime::Runtime,
}

impl Default for App {
    fn default() -> Self {
        App {
            async_runtime: tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap(),
        }
    }
}

lazy_static! {
    static ref APP: App = App::default();
}

pub fn livekit_connect(url: String, token: String) {
    log::info!("Connecting to {} with token {}", url, token);

    APP.async_runtime.spawn(async move {
        let res = Room::connect(&url, &token, RoomOptions::default()).await;

        if let Err(err) = res {
            log::error!("Failed to connect: {}", err);
            return;
        }

        let (room, mut events) = res.unwrap();
        log::info!("Connected to room {}", String::from(room.sid().await));

        while let Some(event) = events.recv().await {
            log::info!("Received event {:?}", event);
        }
    });
}

#[cfg(target_os = "ios")]
pub mod ios {
    use std::ffi::{c_char, CStr};

    #[no_mangle]
    pub extern "C" fn livekit_connect(url: *const c_char, token: *const c_char) {
        let (url, token) = unsafe {
            let url = CStr::from_ptr(url).to_str().unwrap().to_owned();
            let token = CStr::from_ptr(token).to_str().unwrap().to_owned();
            (url, token)
        };

        super::livekit_connect(url, token);
    }
}

#[cfg(target_os = "android")]
pub mod android {
    use android_logger::Config;
    use jni::{
        sys::{jint, JNI_VERSION_1_6},
        JavaVM,
    };
    use log::LevelFilter;
    use std::os::raw::c_void;

    #[allow(non_snake_case)]
    #[no_mangle]
    pub extern "C" fn JNI_OnLoad(vm: JavaVM, _: *mut c_void) -> jint {
        android_logger::init_once(
            Config::default().with_max_level(LevelFilter::Debug).with_tag("livekit-rustexample"),
        );

        log::info!("JNI_OnLoad, initializing LiveKit");
        livekit::webrtc::android::initialize_android(&vm);
        JNI_VERSION_1_6
    }

    #[allow(non_snake_case)]
    #[no_mangle]
    pub extern "C" fn Java_io_livekit_rustexample_App_connect(
        mut env: jni::JNIEnv,
        _: jni::objects::JClass,
        url: jni::objects::JString,
        token: jni::objects::JString,
    ) {
        let url: String = env.get_string(&url).unwrap().into();
        let token: String = env.get_string(&token).unwrap().into();

        super::livekit_connect(url, token);
    }
}
