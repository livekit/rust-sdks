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
  type ApiCredentials,
  tokenGenerate,
  tokenVerify,
  logForwardReceive,
} from '@livekit/uniffi';

async function main() {
  // Receive log messages from Rust
  logForwardBootstrap("debug");

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

  while (true) {
    const message = await logForwardReceive();
    if (!message) {
      console.log('Log forwarding ended');
      break;
    }
    console.log('Log from Rust:', message);
  }
}

if (require.main === module) {
  main();
}
