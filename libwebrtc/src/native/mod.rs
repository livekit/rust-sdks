pub mod apm;
pub mod audio_resampler;
pub mod yuv_helper;
pub mod frame_cryptor;

pub use apm::*;
pub use audio_resampler::*;
pub use yuv_helper::*;
pub use frame_cryptor::*;

#[cfg(not(target_arch = "wasm32"))]
pub fn create_random_uuid() -> String {
    use uuid::Uuid;
    Uuid::new_v4().to_string()
}
