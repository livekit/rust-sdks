// Copyright 2025 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{DeviceInfo, DeviceInfoError, DeviceType};
use jni::objects::{GlobalRef, JObject, JValue};
use jni::JavaVM;
use std::sync::OnceLock;

struct AndroidContext {
    vm: JavaVM,
    context: GlobalRef,
}

static ANDROID_CONTEXT: OnceLock<AndroidContext> = OnceLock::new();

/// Initialize the Android JNI context. Must be called before `device_info()`.
///
/// Typically called from `JNI_OnLoad` or early in your Android application's lifecycle.
pub fn init(vm: &JavaVM, context: JObject) {
    let vm = unsafe { JavaVM::from_raw(vm.get_java_vm_pointer()).unwrap() };
    let mut env = vm.get_env().expect("failed to get JNI env");
    let context = env.new_global_ref(context).expect("failed to create global ref");
    let _ = ANDROID_CONTEXT.set(AndroidContext { vm, context });
}

pub fn device_info() -> Result<DeviceInfo, DeviceInfoError> {
    let ctx = ANDROID_CONTEXT.get().ok_or(DeviceInfoError::NotInitialized)?;

    let mut env =
        ctx.vm.attach_current_thread().map_err(|e| DeviceInfoError::Jni(e.to_string()))?;

    let model = get_build_field(&mut env, "MODEL")?;
    let manufacturer = get_build_field(&mut env, "MANUFACTURER")?;
    let name = get_device_name(&mut env, &ctx.context).unwrap_or_else(|_| model.clone());
    let device_type = detect_device_type(&manufacturer);

    Ok(DeviceInfo { model, name, device_type })
}

fn get_build_field(env: &mut jni::JNIEnv, field: &str) -> Result<String, DeviceInfoError> {
    let build_class = env
        .find_class("android/os/Build")
        .map_err(|e| DeviceInfoError::Jni(format!("find Build class: {e}")))?;

    let value = env
        .get_static_field(build_class, field, "Ljava/lang/String;")
        .map_err(|e| DeviceInfoError::Jni(format!("get Build.{field}: {e}")))?
        .l()
        .map_err(|e| DeviceInfoError::Jni(format!("Build.{field} is not an Object: {e}")))?;

    let jstring: jni::objects::JString = value.into();
    let rust_str = env
        .get_string(&jstring)
        .map_err(|e| DeviceInfoError::Jni(format!("get string Build.{field}: {e}")))?;

    Ok(rust_str.into())
}

fn get_device_name(env: &mut jni::JNIEnv, context: &GlobalRef) -> Result<String, DeviceInfoError> {
    let content_resolver = env
        .call_method(
            context.as_obj(),
            "getContentResolver",
            "()Landroid/content/ContentResolver;",
            &[],
        )
        .map_err(|e| DeviceInfoError::Jni(format!("getContentResolver: {e}")))?
        .l()
        .map_err(|e| DeviceInfoError::Jni(format!("getContentResolver result: {e}")))?;

    // Try Settings.Global "device_name" first, then fall back to "bluetooth_name".
    // Neither is guaranteed to exist on all devices/manufacturers.
    for key_name in &["device_name", "bluetooth_name"] {
        if let Some(name) = get_settings_string(env, &content_resolver, key_name)? {
            if !name.is_empty() {
                return Ok(name);
            }
        }
    }

    Err(DeviceInfoError::Query("device name not available".into()))
}

fn get_settings_string(
    env: &mut jni::JNIEnv,
    content_resolver: &JObject,
    key_name: &str,
) -> Result<Option<String>, DeviceInfoError> {
    let settings_class = env
        .find_class("android/provider/Settings$Global")
        .map_err(|e| DeviceInfoError::Jni(format!("find Settings.Global: {e}")))?;

    let key =
        env.new_string(key_name).map_err(|e| DeviceInfoError::Jni(format!("new_string: {e}")))?;

    let result = env
        .call_static_method(
            settings_class,
            "getString",
            "(Landroid/content/ContentResolver;Ljava/lang/String;)Ljava/lang/String;",
            &[JValue::Object(content_resolver), JValue::Object(&key)],
        )
        .map_err(|e| DeviceInfoError::Jni(format!("Settings.Global.getString({key_name}): {e}")))?
        .l()
        .map_err(|e| DeviceInfoError::Jni(format!("getString result: {e}")))?;

    if result.is_null() {
        return Ok(None);
    }

    let jstring = jni::objects::JString::from(result);
    let rust_str: String = env
        .get_string(&jstring)
        .map_err(|e| DeviceInfoError::Jni(format!("get string {key_name}: {e}")))?
        .into();

    Ok(Some(rust_str))
}

fn detect_device_type(manufacturer: &str) -> DeviceType {
    let m = manufacturer.to_lowercase();
    if m.contains("meta") || m.contains("oculus") {
        DeviceType::Headset
    } else {
        // Default to phone for Android devices; a more precise detection
        // would require checking screen configuration via JNI.
        DeviceType::Phone
    }
}
