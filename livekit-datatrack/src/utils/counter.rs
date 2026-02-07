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

/// A counter that increases monotonically and wraps on overflow.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub struct Counter<T>(T);

#[allow(dead_code)]
impl<T: WrappingIncrement> Counter<T> {
    pub fn new(start: T) -> Self {
        Self(start)
    }

    /// Returns the current value.
    pub fn get(self) -> T {
        self.0
    }

    /// Returns current value, then increments with wrap-around.
    pub fn get_then_increment(&mut self) -> T {
        let current = self.0;
        self.0 = self.0.wrapping_inc();
        current
    }
}

/// A type that supports incrementing with wrap-around.
pub trait WrappingIncrement: Copy {
    fn wrapping_inc(self) -> Self;
}

macro_rules! impl_increment {
    ($($t:ty),* $(,)?) => {
        $(impl WrappingIncrement for $t {
            fn wrapping_inc(self) -> Self {
                self.wrapping_add(1)
            }
        })*
    };
}

impl_increment!(u8, u16, u32, u64);
