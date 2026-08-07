// Copyright 2026 LiveKit, Inc.
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

// Compatibility shim for Android NDK ABI changes.
//
// The WebRTC M150 prebuilt library was compiled against an older Android NDK
// (via Chromium's bundled toolchain) in which `std::__ndk1::__hash_memory` is
// an out-of-line function exported from `libc++_static.a`. Starting with NDK
// r28 (LLVM 18), this function was annotated with `_LIBCPP_HIDE_FROM_ABI` and
// is no longer present as an exported symbol. This causes a link failure when
// the prebuilt is linked against a newer system NDK.
//
// This file provides a **weak** fallback definition so that the prebuilt can be
// linked against any NDK version:
//  - With NDK r27 and older: the strong definition from `libc++_static.a`
//    overrides this weak one; behaviour is identical to before.
//  - With NDK r28 and newer: this weak definition is used; it implements the
//    same FNV-1a algorithm that LLVM libc++ historically used.

#if defined(__ANDROID__)
#include <cstddef>

namespace std {
inline namespace __ndk1 {

__attribute__((weak)) size_t __hash_memory(void const* p, size_t n) noexcept {
    auto const* ptr = static_cast<unsigned char const*>(p);
#if defined(__LP64__)
    // 64-bit FNV-1a
    size_t hash = 14695981039346656037ULL;
    for (size_t i = 0; i < n; ++i) {
        hash ^= static_cast<size_t>(ptr[i]);
        hash *= 1099511628211ULL;
    }
#else
    // 32-bit FNV-1a
    size_t hash = 2166136261U;
    for (size_t i = 0; i < n; ++i) {
        hash ^= static_cast<size_t>(ptr[i]);
        hash *= 16777619U;
    }
#endif
    return hash;
}

}  // namespace __ndk1
}  // namespace std
#endif  // defined(__ANDROID__)
