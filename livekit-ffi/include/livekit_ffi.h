/*
 * Copyright 2023 LiveKit, Inc.
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

#ifndef livekit_ffi
#define livekit_ffi

/* Warning, this file is autogenerated. Don't modify this manually. */

#include <cstdarg>
#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <ostream>
#include <new>

using FfiHandleId = size_t;

constexpr static const FfiHandleId INVALID_HANDLE = 0;

extern "C" {

FfiHandleId livekit_ffi_request(const uint8_t *data,
                                size_t len,
                                const uint8_t **res_ptr,
                                size_t *res_len);

bool livekit_ffi_drop_handle(FfiHandleId handle_id);

} // extern "C"

#endif // livekit_ffi
