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

const LIVEKIT_URL_ENV_NAME: &'static str = "LIVEKIT_URL";
const LIVEKIT_API_KEY_ENV_NAME: &'static str = "LIVEKIT_API_KEY";
const LIVEKIT_API_SECRET_ENV_NAME: &'static str = "LIVEKIT_API_SECRET";


/// MinterCredentials provides a way to configure a TokenSourceMinter with a `LIVEKIT_URL`,
/// `LIVEKIT_API_KEY`, and `LIVEKIT_API_SECRET` value.
pub trait MinterCredentialsSource {
    fn get(&self) -> MinterCredentials;
}

#[derive(Debug, Clone)]
pub struct MinterCredentials {
    pub url: String,
    pub api_key: String,
    pub api_secret: String,
}
impl MinterCredentials {
    pub fn new(server_url: &str, api_key: &str, api_secret: &str) -> Self {
        Self {
            url: server_url.into(),
            api_key: api_key.into(),
            api_secret: api_secret.into(),
        }
    }
}

impl MinterCredentialsSource for MinterCredentials {
    fn get(&self) -> MinterCredentials {
        self.clone()
    }
}

// FIXME: maybe add dotenv source too? Or have this look there too?
pub struct MinterCredentialsEnvironment(String, String, String);

impl MinterCredentialsEnvironment {
    pub fn new(url_variable: &str, api_key_variable: &str, api_secret_variable: &str) -> Self {
        Self(url_variable.into(), api_key_variable.into(), api_secret_variable.into())
    }
}

impl MinterCredentialsSource for MinterCredentialsEnvironment {
    fn get(&self) -> MinterCredentials {
        let (url_variable, api_key_variable, api_secret_variable) = (&self.0, &self.1, &self.2);
        let url = std::env::var(url_variable).expect(format!("{url_variable} is not set").as_str());
        let api_key = std::env::var(api_key_variable).expect(format!("{api_key_variable} is not set").as_str());
        let api_secret = std::env::var(api_secret_variable).expect(format!("{api_secret_variable} is not set").as_str());
        MinterCredentials { url, api_key, api_secret }
    }
}

impl Default for MinterCredentialsEnvironment {
    fn default() -> Self {
        Self(LIVEKIT_URL_ENV_NAME.into(), LIVEKIT_API_KEY_ENV_NAME.into(), LIVEKIT_API_SECRET_ENV_NAME.into())
    }
}

impl<F: Fn() -> MinterCredentials> MinterCredentialsSource for F {
    fn get(&self) -> MinterCredentials {
        self()
    }
}
