use delta_executor_sdk::base::crypto::Ed25519PrivKey;

fn main() {
    let central = Ed25519PrivKey::generate();
    let network = Ed25519PrivKey::generate();
    
    println!("Central vault pubkey: {}", central.pub_key());
    println!("Network goods vault pubkey: {}", network.pub_key());
    
    // Print in the format expected by read_keypair (base58)
    println!("\nFor JSON files:");
    println!("Central: \"{}\"", central);
    println!("Network: \"{}\"", network);
}