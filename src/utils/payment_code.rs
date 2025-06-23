/// Normalizes user input to valid Crockford Base32
/// Handles common user input errors
pub fn normalize_payment_code(input: &str) -> String {
    input
        .to_uppercase()
        .chars()
        .map(|c| match c {
            'O' => '0',  // Letter O to number 0
            'I' => '1',  // Letter I to number 1  
            'L' => '1',  // Letter L to number 1
            _ => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_payment_code() {
        assert_eq!(normalize_payment_code("ABC0O"), "ABC00");
        assert_eq!(normalize_payment_code("abcde"), "ABCDE");
        assert_eq!(normalize_payment_code("O0I1L"), "00111");
        assert_eq!(normalize_payment_code("valid"), "VA11D");
    }
}