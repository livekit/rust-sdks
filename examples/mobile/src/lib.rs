use lazy_static::lazy_static;
use livekit::{Room, RoomOptions};
use std::ffi::{c_char, CStr};

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

#[no_mangle]
pub extern "C" fn livekit_connect(url: *const c_char, token: *const c_char) {
    let (url, token) = unsafe {
        let url = CStr::from_ptr(url).to_str().unwrap().to_owned();
        let token = CStr::from_ptr(token).to_str().unwrap().to_owned();
        (url, token)
    };

    println!("Connecting to {} with token {}", url, token);

    APP.async_runtime.spawn(async move {
        let (room, mut events) = Room::connect(&url, &token, RoomOptions::default())
            .await
            .unwrap();

        println!("Connected to room {}", room.sid());

        while let Some(event) = events.recv().await {
            println!("Received event {:?}", event);
        }
    });
}

#[cfg(target_os = "android")]
mod android {
    use jni::{
        sys::{jint, JNI_VERSION_1_6},
        JavaVM,
    };
    use std::os::raw::c_void;

    #[allow(non_snake_case)]
    #[no_mangle]
    pub extern "C" fn JNI_OnLoad(vm: JavaVM, _: *mut c_void) -> jint {
        println!("JNI_OnLoad, initializing LiveKit");
        livekit::webrtc::android::initialize_android(&vm);
        JNI_VERSION_1_6
    }
}
