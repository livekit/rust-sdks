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

use std::{error::Error, future::{ready, Future}};
use livekit_protocol::{TokenSourceRequest, TokenSourceResponse};

use crate::token_source::TokenSourceFetchOptions;


/// A Fixed TokenSource is a token source that takes no parameters and returns a completely
/// independently derived value on each fetch() call.
///
/// The most common downstream implementer is TokenSourceLiteral.
pub trait TokenSourceFixed {
    // FIXME: what should the error type of the result be?
    fn fetch(&self) -> impl Future<Output = Result<TokenSourceResponse, Box<dyn Error>>>;
}

/// A helper trait to more easily implement a TokenSourceFixed which not async.
pub trait TokenSourceFixedSynchronous {
    // FIXME: what should the error type of the result be?
    fn fetch_synchronous(&self) -> Result<TokenSourceResponse, Box<dyn Error>>;
}
 
impl<T: TokenSourceFixedSynchronous> TokenSourceFixed for T {
    // FIXME: what should the error type of the result be?
    fn fetch(&self) -> impl Future<Output = Result<TokenSourceResponse, Box<dyn Error>>> {
        ready(self.fetch_synchronous())
    }
}


///  A Configurable TokenSource is a token source that takes a
/// TokenSourceFetchOptions object as input and returns a deterministic
/// TokenSourceResponseObject output based on the options specified.
///
/// For example, if options.participantName is set, it should be expected that
/// all tokens that are generated will have participant name field set to the
/// provided value.
///
/// A few common downstream implementers are TokenSourceEndpoint and TokenSourceCustom.
pub trait TokenSourceConfigurable {
    // FIXME: what should the error type of the result be?
    fn fetch(&self, options: &TokenSourceFetchOptions) -> impl Future<Output = Result<TokenSourceResponse, Box<dyn Error>>>;
}

/// A helper trait to more easily implement a TokenSourceConfigurable which not async.
pub trait TokenSourceConfigurableSynchronous {
    // FIXME: what should the error type of the result be?
    fn fetch_synchronous(&self, options: &TokenSourceFetchOptions) -> Result<TokenSourceResponse, Box<dyn Error>>;
}
 
impl<T: TokenSourceConfigurableSynchronous> TokenSourceConfigurable for T {
    // FIXME: what should the error type of the result be?
    fn fetch(&self, options: &TokenSourceFetchOptions) -> impl Future<Output = Result<TokenSourceResponse, Box<dyn Error>>> {
        ready(self.fetch_synchronous(options))
    }
}
