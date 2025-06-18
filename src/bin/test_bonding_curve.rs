fn main() {
    // Bonding curve parameters
    let base_price = 0.01;
    let slope = 0.0000001;
    
    // Calculate tokens for $95 at different points
    let amount = 95.0; // $95
    
    // At start (0 tokens purchased)
    let tokens_at_start = calculate_tokens_for_amount(amount, 0.0, base_price, slope);
    println\!("Tokens for $95 at start: {:.2}", tokens_at_start);
    
    // After 100k tokens purchased
    let tokens_after_100k = calculate_tokens_for_amount(amount, 100000.0, base_price, slope);
    println\!("Tokens for $95 after 100k sold: {:.2}", tokens_after_100k);
    
    // Calculate platform and user split
    let platform_tokens = tokens_at_start * (5.0 / 95.0);
    let user_tokens = tokens_at_start - platform_tokens;
    
    println\!("\nFor $100 donation at start:");
    println\!("Total tokens minted: {:.2}", tokens_at_start);
    println\!("Platform tokens (5.26%): {:.2}", platform_tokens);
    println\!("User tokens (94.74%): {:.2}", user_tokens);
    
    // Check if 8609 makes sense
    println\!("\nTo get 8609 tokens for user:");
    let total_needed = 8609.0 / 0.9474; // 94.74%
    println\!("Total tokens needed: {:.2}", total_needed);
    let amount_needed = calculate_amount_for_tokens(total_needed, 0.0, base_price, slope);
    println\!("Amount needed: ${:.2}", amount_needed);
}

fn calculate_tokens_for_amount(amount: f64, current_tokens: f64, base_price: f64, slope: f64) -> f64 {
    let a = slope / 2.0;
    let b = base_price + (slope * current_tokens);
    let c = -amount;
    
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return 0.0;
    }
    
    (-b + discriminant.sqrt()) / (2.0 * a)
}

fn calculate_amount_for_tokens(tokens: f64, current_tokens: f64, base_price: f64, slope: f64) -> f64 {
    // Integral of price function
    let p0 = base_price + slope * current_tokens;
    let p1 = base_price + slope * (current_tokens + tokens);
    (p0 + p1) / 2.0 * tokens
}
EOF < /dev/null