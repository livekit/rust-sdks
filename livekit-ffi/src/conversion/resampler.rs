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

use crate::{proto, server::resampler};

impl From<proto::SoxResamplerDataType> for resampler::SoxResamplerDataType {
    fn from(value: proto::SoxResamplerDataType) -> Self {
        match value {
            proto::SoxResamplerDataType::SoxrDatatypeInt16i => Self::Interleaved,
            proto::SoxResamplerDataType::SoxrDatatypeInt16s => Self::Split,
        }
    }
}

impl From<proto::SoxQualityRecipe> for resampler::SoxQualityRecipe {
    fn from(value: proto::SoxQualityRecipe) -> Self {
        match value as std::os::raw::c_ulong {
            0 => Self::Quick,
            1 => Self::Low,
            2 => Self::Medium,
            3 => Self::High,
            4 => Self::VeryHigh,
            _ => Self::Medium, // default to Medium if unknown
        }
    }
}
