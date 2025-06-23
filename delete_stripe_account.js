#!/usr/bin/env node

// Usage: node delete_stripe_account.js <stripe_secret_key> <account_id>

const args = process.argv.slice(2);

if (args.length !== 2) {
    console.error('Usage: node delete_stripe_account.js <stripe_secret_key> <account_id>');
    console.error('Example: node delete_stripe_account.js sk_test_... acct_...');
    process.exit(1);
}

const [secretKey, accountId] = args;

// Validate inputs
if (!secretKey.startsWith('sk_')) {
    console.error('Error: Secret key should start with "sk_"');
    process.exit(1);
}

if (!accountId.startsWith('acct_')) {
    console.error('Error: Account ID should start with "acct_"');
    process.exit(1);
}

const stripe = require('stripe')(secretKey);

async function deleteAccount() {
    try {
        console.log(`Attempting to delete account: ${accountId}`);
        
        const deleted = await stripe.accounts.del(accountId);
        
        console.log('✅ Account deleted successfully!');
        console.log(JSON.stringify(deleted, null, 2));
    } catch (error) {
        console.error('❌ Error deleting account:');
        console.error(`Error type: ${error.type}`);
        console.error(`Error message: ${error.message}`);
        
        if (error.raw) {
            console.error('Raw error:', JSON.stringify(error.raw, null, 2));
        }
    }
}

deleteAccount();