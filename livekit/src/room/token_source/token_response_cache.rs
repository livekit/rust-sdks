use crate::token_source::{TokenSourceFetchOptions, TokenSourceResponse};

pub trait TokenResponseCacheValue {}

impl TokenResponseCacheValue for TokenSourceResponse {}
impl TokenResponseCacheValue for (TokenSourceFetchOptions, TokenSourceResponse) {}

/// Represents a mechanism by which token responses can be cached
///
/// When used with a TokenSourceFixed, `Value` is `TokenSourceResponse`
/// When used with a TokenSourceConfigurable, `Value` is `(TokenSourceFetchOptions, TokenSourceResponse)`
pub trait TokenResponseCache<Value: TokenResponseCacheValue> {
    fn get(&self) -> Option<&Value>;
    fn set(&mut self, value: Value);
    fn clear(&mut self);
}

/// In-memory implementation of [TokenResponseCache]
pub struct TokenResponseInMemoryCache<Value: TokenResponseCacheValue>(Option<Value>);
impl<Value: TokenResponseCacheValue> TokenResponseInMemoryCache<Value> {
    pub fn new() -> Self {
        Self(None)
    }
}

impl<Value: TokenResponseCacheValue> TokenResponseCache<Value>
    for TokenResponseInMemoryCache<Value>
{
    fn get(&self) -> Option<&Value> {
        self.0.as_ref()
    }
    fn set(&mut self, value: Value) {
        self.0 = Some(value);
    }
    fn clear(&mut self) {
        self.0 = None;
    }
}
