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

/// Generates methods on an enum that forward each call to the inner value of every variant.
///
/// Given a list of variants and a set of method signatures, this expands to a `match` over `self`
/// that dispatches to the identically-named method on each variant's inner type, saving the
/// boilerplate of writing one `match` arm per variant per method.
///
/// ```ignore
/// impl AnyStreamInfo {
///     enum_dispatch!(
///         [Byte, Text];
///         pub fn id(self: &Self) -> &str;
///         pub fn total_length(self: &Self) -> Option<u64>;
///     );
/// }
/// ```
// TODO(theomonnom): Async methods
#[macro_export]
macro_rules! enum_dispatch {
    // This arm is used to avoid nested loops with the arguments
    // The arguments are transformed to $combined_args tt
    (@match [$($variant:ident),+]: $fnc:ident, $self:ident, $combined_args:tt) => {
        match $self {
            $(
                Self::$variant(inner) => inner.$fnc$combined_args,
            )+
        }
    };

    // Create the function and extract self from the $args tt (little hack)
    (@fnc [$($variant:ident),+]: $vis:vis fn $fnc:ident($self:ident: $sty:ty $(, $arg:ident: $t:ty)*) -> $ret:ty) => {
        #[inline]
        $vis fn $fnc($self: $sty, $($arg: $t),*) -> $ret {
            $crate::enum_dispatch!(@match [$($variant),+]: $fnc, $self, ($($arg,)*))
        }
    };

    ($variants:tt; $($vis:vis fn $fnc:ident$args:tt -> $ret:ty;)+) => {
        $(
            $crate::enum_dispatch!(@fnc $variants: $vis fn $fnc$args -> $ret);
        )+
    };
}
