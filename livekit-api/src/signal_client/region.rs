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

use http::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::Deserialize;

use crate::http_client;

use super::{get_livekit_url, SignalError, SignalResult};

pub struct RegionUrlProvider;

#[derive(Deserialize)]
pub struct RegionUrlResponse {
    pub regions: Vec<RegionUrlInfo>,
}

#[derive(Deserialize)]
pub struct RegionUrlInfo {
    pub region: String,
    pub url: String,
    pub distance: String,
}

impl RegionUrlProvider {
    pub async fn fetch_region_urls(url: &str, token: &str) -> SignalResult<Vec<String>> {
        if is_cloud_url(url)? {
            let client = http_client::Client::new();
            let mut headers = HeaderMap::new();
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
            );
            let res = client
                .get(region_endpoint(url)?)
                .headers(headers)
                .send()
                .await
                .map_err(|e| SignalError::RegionError(e.to_string()))?;

            if !res.status().is_success() {
                return Err(SignalError::Client(
                    res.status(),
                    res.text().await.unwrap_or_default(),
                ));
            }
            let res = res
                .json::<RegionUrlResponse>()
                .await
                .map_err(|e| SignalError::RegionError(e.to_string()))?;
            Ok(res.regions.into_iter().map(|i| i.url).collect())
        } else {
            Ok(vec![])
        }
    }
}

fn is_cloud_url(url: &str) -> SignalResult<bool> {
    let url = url::Url::parse(url).map_err(|err| SignalError::UrlParse(err.to_string()))?;
    let host = match url.host() {
        Some(host) => host.to_string(),
        None => {
            return Err(SignalError::UrlParse("invalid hostname".into()));
        }
    };

    Ok(host.ends_with(".livekit.cloud") || host.ends_with(".livekit.run"))
}

fn region_endpoint(url: &str) -> SignalResult<String> {
    let mut url = url::Url::parse(url).map_err(|err| SignalError::UrlParse(err.to_string()))?;
    match url.scheme() {
        "wss" => url.set_scheme("https").unwrap(),
        "ws" => url.set_scheme("http").unwrap(),
        _ => (),
    }
    url.set_path("/settings/regions");

    Ok(url.to_string())
}
