use anyhow::Result;

pub trait KeyPair {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>>;
    fn verify(&self, message: &[u8], signature: &[u8]) -> Result<bool>;
    fn public_key(&self) -> Vec<u8>;
} 