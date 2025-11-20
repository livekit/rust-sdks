# Copyright 2025 LiveKit, Inc.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

import sys
import os

sys.path.append(os.path.abspath(os.path.join(os.path.dirname(__file__), '..', 'generated', 'python')))

import asyncio
from livekit_uniffi import *

def main():
    # Receive log messages from Rust
    log_forward_bootstrap(level=LogForwardFilter.DEBUG)

    # Print FFI version
    print(f"FFI version: v{build_version()}")

    credentials = ApiCredentials(key="devkey", secret="secret")

    jwt = generate_token(
        options=TokenOptions(room_name="test", identity="some_participant"),
        credentials=credentials,
    )
    print(f"Generated JWT: {jwt}")

    decoded_grants = verify_token(
        token=jwt,
        credentials=credentials,
    )
    print(f"Verified generated JWT: {decoded_grants}")

    async def receive_log_messages():
        while True:
            message = await log_forward_receive()
            if message is None:
                print("Log forwarding ended")
                break
            print(f"Log from Rust: {message}")

    asyncio.run(receive_log_messages())

if __name__ == "__main__":
    main()
