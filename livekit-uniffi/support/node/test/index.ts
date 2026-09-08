/*
 * Copyright 2025 LiveKit, Inc.
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

import {
  buildVersion,
  logForwardBootstrap,
  LogForwardFilter,
  type ApiCredentials,
  tokenGenerate,
  tokenVerify,
  logForwardReceive,
} from '@livekit/uniffi';

async function main() {
  // Receive log messages from Rust
  logForwardBootstrap(LogForwardFilter.Debug);

  // Print FFI version
  console.log(`FFI version: v${buildVersion()}`);

  const credentials: ApiCredentials = { key: "devkey", secret: "secret" };

  const jwt = tokenGenerate(
    {
      identity: "some_participant",
      roomConfiguration: {
        name: "test",
        emptyTimeout: 1000,
        departureTimeout: 1000,
        maxParticipants: 1000,
        metadata: "",
        minPlayoutDelay: 1000,
        maxPlayoutDelay: 1000,
        syncStreams: false,
        agents: [],
      },
    },
    credentials,
  );
  console.log("Generated JWT:", jwt);

  const decodedGrants = tokenVerify(jwt, credentials);
  console.log("Verified generated JWT:", decodedGrants);

  // The Rust log-forward channel has no end-of-stream signal today — its
  // sender lives as long as the process (see livekit-uniffi/src/log_forward.rs),
  // so logForwardReceive() never resolves to `undefined` on its own here. Bound
  // each receive with a short deadline via the generated AbortSignal support
  // instead of waiting on a terminator that won't arrive; still handle
  // `undefined` so this loop is already correct the day the Rust side gets one.
  const RECEIVE_TIMEOUT_MS = 2000;
  while (true) {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), RECEIVE_TIMEOUT_MS);
    let message: Awaited<ReturnType<typeof logForwardReceive>>;
    try {
      message = await logForwardReceive({ signal: controller.signal });
    } catch (err) {
      if (controller.signal.aborted) {
        console.log('No further log messages within timeout, stopping');
        break;
      }
      throw err;
    } finally {
      clearTimeout(timer);
    }
    if (!message) {
      console.log('Log forwarding ended');
      break;
    }
    console.log('Log from Rust:', message);
  }
}

await main();
