use crate::imp::audio_source as imp_as;

#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use super::imp_as;
    use crate::audio_frame::AudioFrame;
    use std::fmt::{Debug, Formatter};

    #[derive(Default, Clone)]
    pub struct NativeAudioSource {
        pub(crate) handle: imp_as::NativeAudioSource,
    }

    impl Debug for NativeAudioSource {
        fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
            f.debug_struct("NativeAudioSource").finish()
        }
    }

    impl NativeAudioSource {
        pub fn capture_frame(&self, frame: &AudioFrame) {
            self.handle.capture_frame(frame)
        }
    }
}
