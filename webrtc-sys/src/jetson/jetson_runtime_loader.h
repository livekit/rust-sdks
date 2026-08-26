/*
 * Copyright 2026 LiveKit, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#ifndef JETSON_RUNTIME_LOADER_H_
#define JETSON_RUNTIME_LOADER_H_

extern "C" {

// dlopen callback used by the implib-generated lazy-loading stubs in
// src/lazy_load_deps_for/jetson (see --dlopen-callback in
// generate_implibs.sh). Also usable directly to load a Jetson userspace
// library by name.
void* lk_jetson_dlopen(const char* lib_name);

// Returns non-zero when the Jetson multimedia userspace libraries
// (libnvbufsurface, libv4l2/libnvv4l2) can be loaded. Must be checked
// before calling into any MMAPI/NvBufSurface code path: those symbols
// are lazily bound and the process aborts if they are invoked while the
// libraries are absent.
int lk_jetson_runtime_libs_available(void);

}  // extern "C"

#endif  // JETSON_RUNTIME_LOADER_H_
