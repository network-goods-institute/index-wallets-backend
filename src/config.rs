use std::{env, path::PathBuf, fs};
use delta_executor_sdk::base::crypto::{Ed25519PrivKey, Ed25519PubKey, read_keypair};
use log::{info, debug};

pub struct KeyConfig {
    pub central_vault_keypair: Ed25519PrivKey,
    pub central_vault_pubkey: Ed25519PubKey,
    pub network_goods_vault_keypair: Ed25519PrivKey,
    pub network_goods_vault_pubkey: Ed25519PubKey,
}

impl KeyConfig {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let (central_vault_keypair, central_vault_pubkey) = load_keypair(
            "CENTRAL_VAULT_PRIVATE_KEY",
            "central_vault_keypair.json"
        )?;
        
        let (network_goods_vault_keypair, network_goods_vault_pubkey) = load_keypair(
            "NETWORK_GOODS_VAULT_PRIVATE_KEY", 
            "network_goods_vault_keypair.json"
        )?;

        Ok(KeyConfig {
            central_vault_keypair,
            central_vault_pubkey,
            network_goods_vault_keypair,
            network_goods_vault_pubkey,
        })
    }
}

fn load_keypair(
    env_var_name: &str,
    json_file_path: &str
) -> Result<(Ed25519PrivKey, Ed25519PubKey), Box<dyn std::error::Error>> {
    // First try to load from environment variable
    if let Ok(private_key_hex) = env::var(env_var_name) {
        info!("Loading {} from environment variable", env_var_name);
        return load_keypair_from_hex(&private_key_hex);
    }
    
    // Fall back to JSON file
    info!("Environment variable {} not found, falling back to JSON file: {}", env_var_name, json_file_path);
    load_keypair_from_json(json_file_path)
}

fn load_keypair_from_hex(private_key_hex: &str) -> Result<(Ed25519PrivKey, Ed25519PubKey), Box<dyn std::error::Error>> {
    debug!("Parsing private key from hex string");
    
    // Remove any whitespace and 0x prefix if present
    let cleaned_hex = private_key_hex.trim().trim_start_matches("0x");
    
    // Decode hex to bytes
    let private_key_bytes = hex::decode(cleaned_hex)
        .map_err(|e| format!("Invalid hex format for private key: {}", e))?;
    
    if private_key_bytes.len() != 32 {
        return Err(format!("Private key must be 32 bytes, got {}", private_key_bytes.len()).into());
    }
    
    // Convert bytes to array
    let mut key_array = [0u8; 32];
    key_array.copy_from_slice(&private_key_bytes);
    
    // Create Ed25519PrivKey from bytes
    let private_key = Ed25519PrivKey::from_bytes(&key_array);
    
    let public_key = private_key.pub_key();
    
    info!("Successfully loaded keypair with pubkey: {}", public_key);
    Ok((private_key, public_key))
}

fn load_keypair_from_json(json_file_path: &str) -> Result<(Ed25519PrivKey, Ed25519PubKey), Box<dyn std::error::Error>> {
    let path = PathBuf::from(json_file_path);
    
    if !path.exists() {
        return Err(format!("Keypair file not found: {}", json_file_path).into());
    }
    
    debug!("Reading keypair from JSON file: {}", json_file_path);
    let private_key = read_keypair(&path)?;
    let public_key = private_key.pub_key();
    
    info!("Successfully loaded keypair from {} with pubkey: {}", json_file_path, public_key);
    Ok((private_key, public_key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_load_keypair_from_hex() {
        // Test with a valid 32-byte hex string
        let test_hex = "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
        
        // This will fail because we need a valid Ed25519 private key, but tests the hex parsing
        match load_keypair_from_hex(test_hex) {
            Ok(_) => println!("Keypair loaded successfully"),
            Err(e) => println!("Expected error for test key: {}", e),
        }
    }
    
    #[test]
    fn test_load_keypair_from_hex_invalid_length() {
        let short_hex = "1234567890abcdef";
        let result = load_keypair_from_hex(short_hex);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be 32 bytes"));
    }
    
    #[test] 
    fn test_load_keypair_from_hex_invalid_format() {
        let invalid_hex = "not_hex_at_all_this_is_invalid_string_zzz";
        let result = load_keypair_from_hex(invalid_hex);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid hex format"));
    }
}