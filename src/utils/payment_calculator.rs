use crate::models::{TokenBalance, TokenValuation, DiscountConsumption, TokenPayment};
use mongodb::bson::Document;

const LAMBDA: f64 = 0.2;

pub fn calculate_vendor_valuations(
    user_preferences: &Document,
    available_tokens: &[TokenBalance],
    payment_amount: f64,
) -> (Vec<TokenValuation>, Vec<DiscountConsumption>) {
    let mut valuations = Vec::new();
    let mut consumptions = Vec::new();
    
    // Calculate how payment will be distributed across tokens
    let total_balance: f64 = available_tokens.iter()
        .map(|t| t.balance * t.average_valuation)
        .sum();
    
    if total_balance == 0.0 {
        return (valuations, consumptions);
    }
    
    for token in available_tokens {
        let token_value = token.balance * token.average_valuation;
        let payment_proportion = token_value / total_balance;
        let token_payment_value = payment_amount * payment_proportion;
        
        // Look up vendor's discount budget for this token (stored in USD)
        let preference_amount = user_preferences
            .get(&token.symbol)
            .or_else(|| user_preferences.get(&token.name))
            .or_else(|| user_preferences.get(&token.symbol.to_lowercase()))
            .or_else(|| user_preferences.get(&token.name.to_lowercase()))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        
        // Get vendor's historical valuation for this token
        let valuation_key = format!("{}_valuation", token.symbol);
        let vendor_valuation = user_preferences
            .get(&valuation_key)
            .and_then(|v| v.as_f64())
            .unwrap_or(token.average_valuation);
        
        log::info!("Token: {} -> Preference: {}, Vendor valuation: {}", 
            token.symbol, preference_amount, vendor_valuation);
        
        // Discount = min(λ * payment_value, preference_budget)
        let discount_amount = if preference_amount != 0.0 {
            let max_consumption = LAMBDA * token_payment_value;
            
            if preference_amount > 0.0 {
                max_consumption.min(preference_amount)
            } else {
                // Negative preference means premium
                -(max_consumption.min(preference_amount.abs()))
            }
        } else {
            0.0
        };
        
        valuations.push(TokenValuation {
            token_key: token.token_key.clone(),
            symbol: token.symbol.clone(),
            valuation: vendor_valuation,
        });
        
        consumptions.push(DiscountConsumption {
            token_key: token.token_key.clone(),
            symbol: token.symbol.clone(),
            amount_used: discount_amount,
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
    
    let total_wallet_value: f64 = payer_balances.iter()
        .map(|b| b.balance * b.average_valuation)
        .sum();
    
    if total_wallet_value == 0.0 {
        return Err("Portfolio has no value".to_string());
    }
    
    // Skip the insufficient funds check here - we'll check after discounts/premiums
    
    // Pay proportionally based on value to maintain portfolio allocation
    for balance in payer_balances {
        let token_value = balance.balance * balance.average_valuation;
        let payment_proportion = token_value / total_wallet_value;
        let payment_value = total_price * payment_proportion;
        
        let tokens_to_pay = if balance.average_valuation > 0.0 {
            payment_value / balance.average_valuation
        } else {
            0.0
        };
        
        if balance.balance == 0.0 {
            continue;
        }
        
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

pub fn apply_discounts_to_payment(
    payments: &mut Vec<TokenPayment>,
    discount_consumptions: &[DiscountConsumption],
    payer_balances: &[TokenBalance],
) -> Result<(), String> {
    for payment in payments.iter_mut() {
        if let Some(discount) = discount_consumptions.iter()
            .find(|d| d.token_key == payment.token_key) {
            
            let market_value = payer_balances.iter()
                .find(|b| b.token_key == payment.token_key)
                .map(|b| b.average_valuation)
                .unwrap_or(1.0);
            
            if market_value > 0.0 && discount.amount_used != 0.0 {
                // Convert USD discount to token units
                let token_discount = discount.amount_used / market_value;
                // Subtract discount (positive discount reduces payment, negative increases)
                payment.amount_to_pay = payment.amount_to_pay - token_discount;
                // Ensure payment doesn't go negative
                if payment.amount_to_pay < 0.0 {
                    payment.amount_to_pay = 0.0;
                }
            }
        }
    }
    
    Ok(())
}

pub fn verify_sufficient_funds_after_discounts(
    final_payments: &[TokenPayment],
    payer_balances: &[TokenBalance],
    original_price: f64,
) -> Result<f64, String> {
    // Calculate actual total cost after discounts/premiums
    let actual_total_cost: f64 = final_payments.iter()
        .map(|payment| {
            let market_value = payer_balances.iter()
                .find(|b| b.token_key == payment.token_key)
                .map(|b| b.average_valuation)
                .unwrap_or(0.0);
            payment.amount_to_pay * market_value
        })
        .sum();
    
    // Calculate available funds
    let total_wallet_value: f64 = payer_balances.iter()
        .map(|b| b.balance * b.average_valuation)
        .sum();
    
    if actual_total_cost > total_wallet_value {
        return Err(format!(
            "Insufficient funds after vendor adjustments: Need ${:.2} but have ${:.2}",
            actual_total_cost,
            total_wallet_value
        ));
    }
    
    // Also check individual token sufficiency
    for payment in final_payments {
        if let Some(balance) = payer_balances.iter()
            .find(|b| b.token_key == payment.token_key) {
            if payment.amount_to_pay > balance.balance {
                return Err(format!(
                    "Insufficient {}: need {:.6} but have {:.6}",
                    balance.symbol,
                    payment.amount_to_pay,
                    balance.balance
                ));
            }
        }
    }
    
    Ok(actual_total_cost)
}

#[allow(dead_code)]
pub fn calculate_post_payment_valuations(
    initial_payments: &[TokenPayment],
    final_payments: &[TokenPayment],
    market_valuations: &[TokenBalance],
) -> Vec<(String, f64, f64)> {
    let mut implied_valuations = Vec::new();
    
    for final_payment in final_payments {
        if final_payment.amount_to_pay == 0.0 {
            continue;
        }
        
        let initial_amount = initial_payments.iter()
            .find(|p| p.token_key == final_payment.token_key)
            .map(|p| p.amount_to_pay)
            .unwrap_or(final_payment.amount_to_pay);
        
        let market_val = market_valuations.iter()
            .find(|b| b.token_key == final_payment.token_key)
            .map(|b| b.average_valuation)
            .unwrap_or(1.0);
        
        if initial_amount > 0.0 {
            // initial/final ratio shows what % of market value vendor accepts
            let effective_valuation = initial_amount / final_payment.amount_to_pay;
            
            // Weight by payment value for averaging
            let weight = final_payment.amount_to_pay * market_val;
            
            implied_valuations.push((final_payment.symbol.clone(), effective_valuation, weight));
        }
    }
    
    implied_valuations
}

#[cfg(test)]
mod tests {
    use super::*;
    use mongodb::bson::{doc, Document};

    fn create_test_balance(symbol: &str, balance: f64, valuation: f64) -> TokenBalance {
        TokenBalance {
            token_key: format!("test_{}", symbol),
            symbol: symbol.to_string(),
            name: format!("{} Token", symbol),
            balance,
            average_valuation: valuation,
            token_image_url: None,
        }
    }

    #[test]
    fn test_proportional_payment_calculation() {
        let balances = vec![
            create_test_balance("BTC", 1.0, 50000.0),  // $50k
            create_test_balance("ETH", 10.0, 3000.0),  // $30k
            create_test_balance("USD", 20000.0, 1.0),  // $20k
        ];

        let vendor_valuations = vec![];
        let total_price = 1000.0;

        let result = calculate_payment_bundle(&balances, &vendor_valuations, total_price).unwrap();

        assert_eq!(result.len(), 3);
        
        // Should pay 1% of each holding to maintain portfolio ratios
        let btc_payment = result.iter().find(|p| p.symbol == "BTC").unwrap();
        assert!((btc_payment.amount_to_pay - 0.01).abs() < 0.0001);
        
        let eth_payment = result.iter().find(|p| p.symbol == "ETH").unwrap();
        assert!((eth_payment.amount_to_pay - 0.1).abs() < 0.0001);
        
        let usd_payment = result.iter().find(|p| p.symbol == "USD").unwrap();
        assert!((usd_payment.amount_to_pay - 200.0).abs() < 0.01);
    }

    #[test]
    fn test_discount_application_with_lambda() {
        let balances = vec![
            create_test_balance("BTC", 1.0, 50000.0),
            create_test_balance("ETH", 10.0, 3000.0),
        ];

        let mut preferences = Document::new();
        preferences.insert("BTC", 100.0); // $100 discount budget
        preferences.insert("ETH", 50.0);  // $50 discount budget

        let payment_amount = 1000.0;
        
        let (_valuations, consumptions) = calculate_vendor_valuations(&preferences, &balances, payment_amount);

        // λ=0.2 caps discount at 20% of payment value
        // BTC gets $625 of payment, max discount $125, budget $100 -> uses $100
        let btc_consumption = consumptions.iter().find(|c| c.symbol == "BTC").unwrap();
        assert!((btc_consumption.amount_used - 100.0).abs() < 0.01);
        
        // ETH gets $375 of payment, max discount $75, budget $50 -> uses $50
        let eth_consumption = consumptions.iter().find(|c| c.symbol == "ETH").unwrap();
        assert!((eth_consumption.amount_used - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_apply_discounts_to_payment() {
        let balances = vec![
            create_test_balance("BTC", 1.0, 50000.0),
            create_test_balance("ETH", 10.0, 3000.0),
        ];

        let mut payments = vec![
            TokenPayment {
                token_key: "test_BTC".to_string(),
                symbol: "BTC".to_string(),
                amount_to_pay: 0.01, // 0.01 BTC = $500
                token_image_url: None,
            },
            TokenPayment {
                token_key: "test_ETH".to_string(),
                symbol: "ETH".to_string(),
                amount_to_pay: 0.1, // 0.1 ETH = $300
                token_image_url: None,
            },
        ];

        let consumptions = vec![
            DiscountConsumption {
                token_key: "test_BTC".to_string(),
                symbol: "BTC".to_string(),
                amount_used: 100.0, // $100 discount
            },
            DiscountConsumption {
                token_key: "test_ETH".to_string(),
                symbol: "ETH".to_string(),
                amount_used: 50.0, // $50 discount
            },
        ];

        apply_discounts_to_payment(&mut payments, &consumptions, &balances).unwrap();

        // BTC: 0.01 - (100/50000) = 0.01 - 0.002 = 0.008
        let btc_payment = payments.iter().find(|p| p.symbol == "BTC").unwrap();
        assert!((btc_payment.amount_to_pay - 0.008).abs() < 0.0001);

        // ETH: 0.1 - (50/3000) = 0.1 - 0.0167 = 0.0833
        let eth_payment = payments.iter().find(|p| p.symbol == "ETH").unwrap();
        assert!((eth_payment.amount_to_pay - 0.0833).abs() < 0.001);
    }

    #[test]
    fn test_zero_balance_tokens_skipped() {
        let balances = vec![
            create_test_balance("BTC", 1.0, 50000.0),
            create_test_balance("ETH", 0.0, 3000.0), // Zero balance
            create_test_balance("USD", 1000.0, 1.0),
        ];

        let vendor_valuations = vec![];
        let total_price = 100.0;

        let result = calculate_payment_bundle(&balances, &vendor_valuations, total_price).unwrap();

        // Should only have 2 payments (skip ETH with 0 balance)
        assert_eq!(result.len(), 2);
        assert!(result.iter().find(|p| p.symbol == "ETH").is_none());
    }

    #[test]
    fn test_insufficient_individual_token() {
        let balances = vec![
            create_test_balance("BTC", 0.0001, 50000.0), // $5
            create_test_balance("USD", 10.0, 1.0),       // $10
        ];

        let vendor_valuations = vec![];
        let total_price = 100.0;

        // Should fail on individual token check, not total value
        let result = calculate_payment_bundle(&balances, &vendor_valuations, total_price);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Insufficient BTC"));
    }

    #[test]
    fn test_effective_valuation_calculation() {
        let balances = vec![
            create_test_balance("BTC", 1.0, 50000.0),
            create_test_balance("ETH", 10.0, 3000.0),
        ];

        let mut preferences = Document::new();
        preferences.insert("BTC", 5000.0);
        preferences.insert("ETH", 600.0);

        let payment_amount = 1000.0;
        
        let initial_payments = calculate_payment_bundle(&balances, &vec![], payment_amount).unwrap();
        let (_valuations, consumptions) = calculate_vendor_valuations(&preferences, &balances, payment_amount);
        
        let mut final_payments = initial_payments.clone();
        apply_discounts_to_payment(&mut final_payments, &consumptions, &balances).unwrap();
        
        // BTC: 20% discount -> effective valuation = 0.8
        let btc_initial = initial_payments.iter().find(|p| p.symbol == "BTC").unwrap();
        let btc_final = final_payments.iter().find(|p| p.symbol == "BTC").unwrap();
        let btc_effective = btc_final.amount_to_pay / btc_initial.amount_to_pay;
        assert!((btc_effective - 0.8).abs() < 0.01);
        
        // ETH: 20% discount -> effective valuation = 0.8
        let eth_initial = initial_payments.iter().find(|p| p.symbol == "ETH").unwrap();
        let eth_final = final_payments.iter().find(|p| p.symbol == "ETH").unwrap();
        let eth_effective = eth_final.amount_to_pay / eth_initial.amount_to_pay;
        assert!((eth_effective - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_premium_causes_insufficient_funds() {
        let balances = vec![
            create_test_balance("MEME", 1000.0, 0.1), // $100 worth
            create_test_balance("USD", 30.0, 1.0),    // $30
        ];
        // Total: $130

        let mut preferences = Document::new();
        preferences.insert("MEME", -50.0); // Premium budget
        preferences.insert("USD", -50.0);  // Premium budget

        let payment_amount = 120.0; // Close to wallet value

        let initial_payments = calculate_payment_bundle(&balances, &vec![], payment_amount).unwrap();
        let (_valuations, consumptions) = calculate_vendor_valuations(&preferences, &balances, payment_amount);
        
        let mut final_payments = initial_payments.clone();
        apply_discounts_to_payment(&mut final_payments, &consumptions, &balances).unwrap();
        
        // With λ=0.2, max premium is 20% of payment = $24
        // So actual cost should be ~$144, which exceeds $130 wallet
        let result = verify_sufficient_funds_after_discounts(&final_payments, &balances, payment_amount);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Insufficient funds after vendor adjustments"));
    }

    #[test]
    fn test_discount_enables_payment() {
        let balances = vec![
            create_test_balance("BTC", 0.002, 50000.0), // $100
        ];

        let mut preferences = Document::new();
        preferences.insert("BTC", 30.0); // $30 discount budget

        let payment_amount = 100.0;

        // Calculate everything
        let initial_payments = calculate_payment_bundle(&balances, &vec![], payment_amount).unwrap();
        let (_valuations, consumptions) = calculate_vendor_valuations(&preferences, &balances, payment_amount);
        
        let mut final_payments = initial_payments.clone();
        apply_discounts_to_payment(&mut final_payments, &consumptions, &balances).unwrap();
        
        // With $30 discount on $100 payment, actual cost should be ~$80
        let result = verify_sufficient_funds_after_discounts(&final_payments, &balances, payment_amount);
        assert!(result.is_ok());
        let actual_cost = result.unwrap();
        assert!((actual_cost - 80.0).abs() < 1.0); // Should be around $80
    }
}

