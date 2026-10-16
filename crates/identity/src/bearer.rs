//! Reading the `Authorization: Bearer` header.

use http::HeaderMap;
use http::header::AUTHORIZATION;

use crate::api_key;

/// A credential found in the `Authorization` header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Credential<'a> {
    /// A signed token.
    Jwt(&'a str),
    /// A tenant API key.
    ApiKey(&'a str),
}

/// The bearer credential of a request, if any. A header with another scheme or an empty
/// token counts as absent.
pub fn credential(headers: &HeaderMap) -> Option<Credential<'_>> {
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?.trim();
    let (scheme, token) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    Some(if api_key::is_api_key(token) {
        Credential::ApiKey(token)
    } else {
        Credential::Jwt(token)
    })
}

#[cfg(test)]
mod tests {
    use http::HeaderValue;

    use super::*;

    fn headers(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_str(value).unwrap());
        headers
    }

    #[test]
    fn reads_jwts_and_api_keys() {
        assert_eq!(
            credential(&headers("Bearer abc.def.ghi")),
            Some(Credential::Jwt("abc.def.ghi"))
        );
        assert_eq!(
            credential(&headers("bearer  taut_live_xyz ")),
            Some(Credential::ApiKey("taut_live_xyz"))
        );
    }

    #[test]
    fn other_schemes_and_empty_tokens_are_absent() {
        assert_eq!(credential(&headers("Basic abc")), None);
        assert_eq!(credential(&headers("Bearer ")), None);
        assert_eq!(credential(&headers("Bearer")), None);
        assert_eq!(credential(&HeaderMap::new()), None);
    }
}
