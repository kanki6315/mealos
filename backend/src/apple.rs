use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::Error;

#[derive(Debug, Deserialize)]
struct Claims {
    sub: String,
    nonce: String,
    iat: u64,
    exp: u64,
}

pub type ParsedKeys = HashMap<String, DecodingKey>;

struct Cache {
    keys: Option<Arc<ParsedKeys>>,
    fetched: Option<Instant>,
}

pub struct AppleVerifier {
    audience: String,
    client: reqwest::Client,
    cache: RwLock<Cache>,
}

impl AppleVerifier {
    pub fn new(audience: String) -> Result<Self, reqwest::Error> {
        Ok(Self {
            audience,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()?,
            cache: RwLock::new(Cache {
                keys: None,
                fetched: None,
            }),
        })
    }
    #[cfg(feature = "test-fixtures")]
    pub fn with_test_keys(audience: String, keys: JwkSet) -> Self {
        Self {
            audience,
            client: reqwest::Client::new(),
            cache: RwLock::new(Cache {
                keys: Some(keys),
                fetched: Some(Instant::now()),
            }),
        }
    }
    pub async fn verify(&self, token: &str, nonce: &str) -> Result<String, Error> {
        let header = decode_header(token).map_err(|_| Error::unauthorized())?;
        if header.alg != Algorithm::RS256 {
            return Err(Error::unauthorized());
        }
        let kid = header.kid.ok_or_else(Error::unauthorized)?;

        let keys = self.get_or_refresh_keys(&kid).await?;
        let key = keys.get(&kid).ok_or_else(Error::unauthorized)?;

        verify_with_key(token, nonce, &self.audience, key)
    }

    async fn get_or_refresh_keys(&self, kid: &str) -> Result<Arc<ParsedKeys>, Error> {
        {
            let cache = self.cache.read().await;
            if !Self::needs_refresh(&cache, kid)
                && let Some(keys) = &cache.keys
            {
                return Ok(Arc::clone(keys));
            }
        }

        let mut cache = self.cache.write().await;

        if Self::needs_refresh(&cache, kid) {
            let response = self
                .client
                .get("https://appleid.apple.com/auth/keys")
                .send()
                .await
                .map_err(|_| Error::unavailable())?
                .error_for_status()
                .map_err(|_| Error::unavailable())?;

            let jwks = response
                .json::<JwkSet>()
                .await
                .map_err(|_| Error::unavailable())?;
            cache.keys = Some(Arc::new(Self::parse_jwk_set(&jwks)));
            cache.fetched = Some(Instant::now());
        }

        cache.keys.as_ref().cloned().ok_or_else(Error::unavailable)
    }

    fn needs_refresh(cache: &Cache, kid: &str) -> bool {
        let age = cache.fetched.map(|t| t.elapsed());
        let missing = cache
            .keys
            .as_ref()
            .is_none_or(|keys| !keys.contains_key(kid));

        age.is_none()
            || age.is_some_and(|a| a >= Duration::from_secs(3600))
            || (missing && age.is_some_and(|a| a >= Duration::from_secs(60)))
    }

    fn parse_jwk_set(jwks: &JwkSet) -> ParsedKeys {
        jwks.keys
            .iter()
            .filter_map(|jwk| {
                let kid = jwk.common.key_id.as_ref()?;
                let key = DecodingKey::from_jwk(jwk).ok()?;
                Some((kid.clone(), key))
            })
            .collect()
    }
}

pub fn verify_with_key(
    token: &str,
    nonce: &str,
    audience: &str,
    key: &DecodingKey,
) -> Result<String, Error> {
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&["https://appleid.apple.com"]);
    validation.set_audience(&[audience]);
    validation.set_required_spec_claims(&["exp", "iat", "sub", "iss", "aud", "nonce"]);
    validation.leeway = 5;

    let claims = decode::<Claims>(token, key, &validation)
        .map_err(|_| Error::unauthorized())?
        .claims;

    let now = chrono::Utc::now().timestamp().max(0) as u64;
    if claims.nonce != nonce
        || claims.sub.is_empty()
        || claims.sub.len() > 255
        || claims.iat > now + 5
        || now > claims.iat.saturating_add(605)
        || claims.exp <= claims.iat
    {
        return Err(Error::unauthorized());
    }

    Ok(claims.sub)
}

pub fn verify_with_keys(
    token: &str,
    nonce: &str,
    audience: &str,
    keys: &JwkSet,
) -> Result<String, Error> {
    let header = decode_header(token).map_err(|_| Error::unauthorized())?;
    if header.alg != Algorithm::RS256 {
        return Err(Error::unauthorized());
    }

    let kid = header.kid.as_deref().ok_or_else(Error::unauthorized)?;

    let jwk = keys.find(kid).ok_or_else(Error::unauthorized)?;
    let key = DecodingKey::from_jwk(jwk).map_err(|_| Error::unauthorized())?;

    verify_with_key(token, nonce, audience, &key)
}
