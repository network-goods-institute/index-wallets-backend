use crate::models::{TokenBalance, TokenValuation, DiscountConsumption, TokenPayment};
use mongodb::bson::Document;

const LAMBDA: f64 = 0.2;

fn total_portfolio_value(tokens: &[TokenBalance]) -> f64 {
    tokens.iter()
        .map(|t| t.balance * t.average_valuation)
        .sum()
}

pub fn calculate_vendor_valuations(
    user_preferences: &Document,
    available_tokens: &[TokenBalance],
    payment_amount: f64,
) -> (Vec<TokenValuation>, Vec<DiscountConsumption>) {
    let mut valuations = Vec::new();
    let mut consumptions = Vec::new();
    
    for token in available_tokens {
        // Try multiple keys to find preference for this token
        let preference_amount = user_preferences
            .get(&token.symbol)  // Try exact symbol match first
            .or_else(|| user_preferences.get(&token.name))  // Try full name
            .or_else(|| user_preferences.get(&token.symbol.to_lowercase()))  // Try lowercase
            .or_else(|| user_preferences.get(&token.name.to_lowercase()))  // Try lowercase name
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        
        log::info!("Token: {} (name: {}) -> Found preference: {}", 
            token.symbol, token.name, preference_amount);
        
        let (valuation, amount_used) = if preference_amount > 0.0 {
            // Positive = discount available (vendor likes token = higher valuation)
            let max_discount = (LAMBDA * payment_amount * token.balance / total_portfolio_value(available_tokens))
                .min(preference_amount);
            let new_valuation = token.average_valuation + max_discount / payment_amount;
            (new_valuation, max_discount)
        } else if preference_amount < 0.0 {
            // Negative = premium to pay (vendor dislikes token = lower valuation)
            let max_premium = (LAMBDA * payment_amount * token.balance / total_portfolio_value(available_tokens))
                .min(preference_amount.abs());
            let new_valuation = (token.average_valuation - max_premium / payment_amount).max(0.0);
            (new_valuation, max_premium)
        } else {
            // No preference or zero remaining
            (token.average_valuation, 0.0)
        };
        
        valuations.push(TokenValuation {
            token_key: token.token_key.clone(),
            symbol: token.symbol.clone(),
            valuation,
        });
        
        consumptions.push(DiscountConsumption {
            token_key: token.token_key.clone(),
            symbol: token.symbol.clone(),
            amount_used,
        });
    }
    
    (valuations, consumptions)
}

pub fn calculate_payment_bundle(
    payer_balances: &[TokenBalance],
    vendor_valuations: &[TokenValuation],
    total_price: f64,
) -> Result<Vec<TokenPayment>, String> {
    let mut payments = Vec::new();
    let mut token_values = Vec::new();
    
    // Calculate total portfolio value from vendor's perspective
    let mut total_portfolio_value = 0.0;
    
    for balance in payer_balances {
        // Find corresponding vendor valuation for this token
        let vendor_val = vendor_valuations.iter()
            .find(|v| v.symbol == balance.symbol)
            .map(|v| v.valuation)
            .unwrap_or(balance.average_valuation); // Use market value as fallback
        
        let token_value = balance.balance * vendor_val;
        total_portfolio_value += token_value;
        
        // Store for later use
        token_values.push((balance, vendor_val, token_value));
    }
    
    // Check if payment is feasible
    if total_portfolio_value < total_price {
        return Err(format!(
            "Insufficient funds: Portfolio value ${:.2} < Payment ${:.2}",
            total_portfolio_value,
            total_price
        ));
    }
    
    // Edge case: no value at all
    if total_portfolio_value == 0.0 {
        return Err("Portfolio has no value from vendor's perspective".to_string());
    }
    
    // Calculate payment amounts based on value proportions
    for (balance, vendor_val, token_value) in token_values {
        // Weight based on value contribution, not token count
        let weight = token_value / total_portfolio_value;
        let payment_amount = total_price * weight;
        
        // Convert payment amount to token units
        let tokens_to_pay = if vendor_val > 0.0 {
            payment_amount / vendor_val
        } else {
            0.0 // Can't pay with worthless tokens
        };
        
        // Verify we have enough tokens
        if tokens_to_pay > balance.balance {
            return Err(format!(
                "Insufficient {}: need {:.6} but have {:.6}",
                balance.symbol,
                tokens_to_pay,
                balance.balance
            ));
        }
        
        payments.push(TokenPayment {
            token_key: balance.token_key.clone(),
            symbol: balance.symbol.clone(),
            amount_to_pay: tokens_to_pay,
            token_image_url: balance.token_image_url.clone(),
        });
    }
    
    Ok(payments)
}


