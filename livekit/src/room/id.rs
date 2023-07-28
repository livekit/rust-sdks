// Copyright 2023 LiveKit, Inc.
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

use std::fmt;

macro_rules! id_str {
    ($($name:ident;)*) => {
        $(
            impl From<String> for $name {
                fn from(str: String) -> $name {
                    $name(str)
                }
            }

            impl From<$name> for String {
                fn from(id: $name) -> String {
                    id.0
                }
            }

            impl PartialEq<$name> for String {
                fn eq(&self, u: &$name) -> bool {
                    *self == *u.0
                }
            }

            impl fmt::Display for $name {
                fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                    f.write_str(&self.0)
                }
            }
        )*
    }
}

#[derive(Clone, Default, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct ParticipantSid(pub String);

#[derive(Clone, Default, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct ParticipantIdentity(pub String);

#[derive(Clone, Default, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct TrackSid(pub String);

#[derive(Clone, Default, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct RoomSid(pub String);

id_str! {
    ParticipantSid;
    ParticipantIdentity;
    TrackSid;
    RoomSid;
}
