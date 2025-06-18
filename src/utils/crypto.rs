
pub fn split_token_id(token_id: &str) -> Result<(String, u32), String> {
    let parts: Vec<&str> = token_id.split(',').collect();
    if parts.len() != 2 {
        return Err("Invalid token id format".to_string());
    }
    let shard = parts[1].parse::<u32>();
    if shard.is_err() {
        return Err("Invalid token id format".to_string());
    }
    Ok((parts[0].to_string(), shard.unwrap()))
}
// 




// calculate token market valuation:

// time ordered, time weighted? 

// 

// 


pub fn dollars_to_tokens(dollars: f64) -> u64 {
    (dollars * 100.0) as u64
}
