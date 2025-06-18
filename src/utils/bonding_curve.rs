pub struct BondingCurve {
    pub base_price: f64,
    pub slope: f64,
}

impl BondingCurve {
    pub fn new() -> Self {
        Self {
            base_price: 0.01,      // $0.01 per token (1 cent)
            slope: 0.0000001,      // Doubles after 100,000 tokens (~$1,000 raised)
        }
    }

    pub fn calculate_price(&self, tokens_purchased: f64) -> f64 {
        self.base_price + (self.slope * tokens_purchased)
    }

    pub fn calculate_tokens_for_amount(&self, amount: f64, current_tokens_purchased: f64) -> f64 {
        // Simple linear approximation - works well for small slopes
        // Current price at this point
        let current_price = self.base_price + (self.slope * current_tokens_purchased);
        
        // Estimate tokens using current price (good approximation for small slopes)
        let tokens = amount / current_price;
        
        // For better accuracy, use average of start and end price
        let end_price = current_price + (self.slope * tokens);
        let avg_price = (current_price + end_price) / 2.0;
        
        let result = amount / avg_price;
        
        log::info!("Bonding curve calc: amount=${}, current_tokens={}, current_price=${}, avg_price=${}, result={} tokens", 
                  amount, current_tokens_purchased, current_price, avg_price, result);
        
        result
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_price() {
        let curve = BondingCurve::new();
        
        assert_eq!(curve.calculate_price(0.0), 0.01);
        assert_eq!(curve.calculate_price(1000.0), 0.0101);
        assert_eq!(curve.calculate_price(10000.0), 0.011);
    }

    #[test]
    fn test_calculate_tokens_for_amount() {
        let curve = BondingCurve::new();
        
        // At starting price of $0.01, $1 should buy approximately 99.995 tokens
        let tokens = curve.calculate_tokens_for_amount(1.0, 0.0);
        assert!((tokens - 99.995).abs() < 0.01);
        
        // $10 should buy approximately 999.5 tokens
        let tokens = curve.calculate_tokens_for_amount(10.0, 0.0);
        assert!((tokens - 999.5).abs() < 0.1);
    }

}