use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, EncodingKey, Header, jwk::JwkSet};
use rsa::{
    RsaPrivateKey,
    pkcs1::{EncodeRsaPrivateKey, LineEnding},
    traits::PublicKeyParts,
};
use serde_json::{Value, json};
pub const AUDIENCE: &str = "com.arjunakankipati.forkware";
pub struct Signing {
    pub key: EncodingKey,
    pub keys: JwkSet,
}
impl Signing {
    pub fn new() -> Self {
        let key = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).unwrap();
        let keys=serde_json::from_value(json!({"keys":[{"kty":"RSA","kid":"test-key","use":"sig","alg":"RS256","n":URL_SAFE_NO_PAD.encode(key.n().to_bytes_be()),"e":URL_SAFE_NO_PAD.encode(key.e().to_bytes_be())}]})).unwrap();
        Self {
            key: EncodingKey::from_rsa_pem(key.to_pkcs1_pem(LineEnding::LF).unwrap().as_bytes())
                .unwrap(),
            keys,
        }
    }
    pub fn claims(&self, subject: &str, nonce: &str) -> Value {
        let now = chrono::Utc::now().timestamp();
        json!({"iss":"https://appleid.apple.com","aud":AUDIENCE,"sub":subject,"nonce":nonce,"iat":now,"exp":now+300})
    }
    pub fn sign(&self, claims: &Value) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("test-key".into());
        jsonwebtoken::encode(&header, claims, &self.key).unwrap()
    }
}
