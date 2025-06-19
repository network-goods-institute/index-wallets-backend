use mongodb::{Client, Collection};
use mongodb::bson::{self, doc, Document, oid::ObjectId};
use mongodb::options::{ClientOptions, ServerApi, ServerApiVersion, IndexOptions};
use mongodb::IndexModel;
use crate::models::{ApiError, User, Preferences, CreateUserRequest, Payment, Token, TokenValuation, DiscountConsumption, TokenPayment, PaymentStatus, TransactionRecord, CauseDraft, DraftStatus, DepositRecord};
use crate::models::cause::Cause;
use futures_util::{TryStreamExt, StreamExt};
use crate::services::cause_service::UpdateCauseRequest;
use std::env;
use rand::Rng;

#[derive(Clone)]
pub struct MongoDBService {
    users: Collection<User>,
    transactions: Collection<Payment>,
    tokens: Collection<Token>,
    causes: Collection<Cause>,
    cause_drafts: Collection<CauseDraft>,
    transaction_records: Collection<TransactionRecord>,
    deposit_records: Collection<DepositRecord>,
}

impl MongoDBService {
    pub async fn init() -> Result<Self, mongodb::error::Error> {
        // Get MongoDB URI from environment variable
        let uri = env::var("MONGODB_URI").expect("MONGODB_URI must be set");
        
        // Parse options and configure client
        let mut client_options = ClientOptions::parse(&uri).await?;
        
        // Set the server API version to V1
        let server_api = ServerApi::builder()
            .version(ServerApiVersion::V1)
            .strict(true)
            .deprecation_errors(true)
            .build();
        client_options.server_api = Some(server_api);

        // Optional: Add timeout settings
        client_options.connect_timeout = Some(std::time::Duration::from_secs(10));
        client_options.server_selection_timeout = Some(std::time::Duration::from_secs(5));

        // Create client
        let client = Client::with_options(client_options)?;
        
        // Test connection
        client
            .database("admin")
            .run_command(doc! {"ping": 1}, None)
            .await?;

        log::info!("Successfully connected to MongoDB Atlas!");
        
        // Get database and collection
        let db = client.database("index_wallets");
        let users = db.collection("users");
        let transactions = db.collection("transactions");
        let tokens = db.collection("tokens");
        let causes = db.collection("causes");
        let cause_drafts = db.collection::<CauseDraft>("cause_drafts");
        let transaction_records = db.collection("transaction_records");
        let deposit_records = db.collection::<DepositRecord>("deposit_records");
        
        // Create unique index for wallet_address only
        let options = IndexOptions::builder().unique(true).build();
        let wallet_model = IndexModel::builder()
            .keys(doc! { "wallet_address": 1 })
            .options(options)
            .build();
        users.create_index(wallet_model, None).await?;

        // Create unique index for payment_id
        let payment_options = IndexOptions::builder().unique(true).build();
        let payment_model = IndexModel::builder()
            .keys(doc! { "payment_id": 1 })
            .options(payment_options)
            .build();
        transactions.create_index(payment_model, None).await?;
        
        // Create TTL index for cause_drafts to auto-expire after 1 day
        let ttl_options = IndexOptions::builder()
            .expire_after(Some(std::time::Duration::from_secs(0))) // 0 means use the expires_at field
            .build();
        let ttl_model = IndexModel::builder()
            .keys(doc! { "expires_at": 1 })
            .options(ttl_options)
            .build();
        cause_drafts.create_index(ttl_model, None).await?;
        
        // Create unique indexes for cause_drafts with case-insensitive collation
        let collation = mongodb::options::Collation::builder()
            .locale("en")
            .strength(mongodb::options::CollationStrength::Secondary) // Case-insensitive
            .build();
        
        // Unique index for name
        let name_options = IndexOptions::builder()
            .unique(true)
            .collation(collation.clone())
            .build();
        let name_model = IndexModel::builder()
            .keys(doc! { "name": 1 })
            .options(name_options)
            .build();
        cause_drafts.create_index(name_model, None).await?;
        
        // Unique index for token_name
        let token_name_options = IndexOptions::builder()
            .unique(true)
            .collation(collation.clone())
            .build();
        let token_name_model = IndexModel::builder()
            .keys(doc! { "token_name": 1 })
            .options(token_name_options)
            .build();
        cause_drafts.create_index(token_name_model, None).await?;
        
        // Unique index for token_symbol
        let token_symbol_options = IndexOptions::builder()
            .unique(true)
            .collation(collation.clone())
            .build();
        let token_symbol_model = IndexModel::builder()
            .keys(doc! { "token_symbol": 1 })
            .options(token_symbol_options)
            .build();
        cause_drafts.create_index(token_symbol_model, None).await?;
        
        // Create indexes for causes collection
        // Index for displayed field (for filtering visible causes)
        let displayed_model = IndexModel::builder()
            .keys(doc! { "displayed": 1 })
            .build();
        causes.create_index(displayed_model, None).await?;
        
        // Index for featured field (for filtering featured causes)
        let featured_model = IndexModel::builder()
            .keys(doc! { "featured": 1 })
            .build();
        causes.create_index(featured_model, None).await?;
        
        // Compound index for common query pattern (featured and displayed)
        let compound_model = IndexModel::builder()
            .keys(doc! { "featured": -1, "displayed": 1, "created_at": -1 })
            .build();
        causes.create_index(compound_model, None).await?;
        
        Ok(Self { users, transactions, tokens, causes, cause_drafts, transaction_records, deposit_records })
    }

    pub async fn create_user(&self, user: User) -> Result<User, ApiError> {
        // Validate user data
        if user.wallet_address.trim().is_empty() {
            return Err(ApiError::ValidationError("Wallet address cannot be empty".to_string()));
        }
        if user.username.trim().is_empty() {
            return Err(ApiError::ValidationError("Username cannot be empty".to_string()));
        }

        // Check if user already exists by wallet address only
        if let Some(_) = self.users
            .find_one(doc! { "wallet_address": &user.wallet_address }, None)
            .await
            .map_err(ApiError::DatabaseError)? {
            return Err(ApiError::DuplicateUser(format!("User with wallet address {} already exists", user.wallet_address)));
        }

        // Insert the user
        self.users
            .insert_one(user.clone(), None)
            .await
            .map_err(ApiError::DatabaseError)?;

        Ok(user)
    }

    pub async fn get_user_by_wallet(&self, wallet_address: &str) -> Result<Option<User>, ApiError> {
        self.users
            .find_one(doc! { "wallet_address": wallet_address }, None)
            .await
            .map_err(ApiError::DatabaseError)
    }

    pub async fn create_payment(&self, payment_data: Payment) -> Result<Payment, ApiError> {
        // Insert the payment into transactions collection
        self.transactions
            .insert_one(payment_data.clone(), None)
            .await
            .map_err(ApiError::DatabaseError)?;

        Ok(payment_data)
    }

    pub async fn get_payment(&self, payment_id: &str) -> Result<Option<Payment>, ApiError> {
        log::info!("Querying database for payment_id: {}", payment_id);
        let result = self.transactions
            .find_one(doc! { "payment_id": payment_id }, None)
            .await;
        
        match result {
            Ok(payment) => {
                log::info!("Database query successful, payment found: {}", payment.is_some());
                Ok(payment)
            },
            Err(e) => {
                log::error!("Database query failed for payment_id {}: {}", payment_id, e);
                Err(ApiError::DatabaseError(e))
            }
        }
    }

    pub async fn update_payment_with_payer(&self, payment_id: &str, payer_address: String, payer_username: Option<String>) -> Result<Payment, ApiError> {
        // First check if payment exists
        let payment = self.get_payment(payment_id).await?
            .ok_or_else(|| ApiError::ValidationError("Payment code not found".to_string()))?;

        // Check if payment is already completed
        if matches!(payment.status, PaymentStatus::Completed) {
            return Err(ApiError::ValidationError("Transaction already fulfilled".to_string()));
        }

        // Check if payment already has a customer assigned
        if let Some(existing_customer) = &payment.customer_address {
            if existing_customer != &payer_address {
                return Err(ApiError::ValidationError("Payer already assigned".to_string()));
            }
            // If same payer, allow them to re-calculate
        }

        // Update the payment with payer information
        let mut update_doc = doc! {
            "customer_address": payer_address,
            "status": bson::to_bson(&PaymentStatus::CustomerAssigned)
                .map_err(|e| ApiError::InternalError(format!("Failed to serialize status: {}", e)))?
        };
        
        if let Some(username) = payer_username {
            update_doc.insert("customer_username", username);
        }
        
        let update = doc! {
            "$set": update_doc
        };

        let updated_payment = self.transactions
            .find_one_and_update(
                doc! { "payment_id": payment_id },
                update,
                Some(mongodb::options::FindOneAndUpdateOptions::builder()
                    .return_document(mongodb::options::ReturnDocument::After)
                    .build())
            )
            .await
            .map_err(|e| {
                log::error!("Database error during payment update: {:?}", e);
                ApiError::DatabaseError(e)
            })?
            .ok_or_else(|| ApiError::NotFound(format!("Payment with ID {} not found", payment_id)))?;

        Ok(updated_payment)
    }

    pub fn generate_payment_id(&self) -> String {
        use rand::Rng;
        
        // Generate 3 random bytes (24 bits)
        let mut rng = rand::thread_rng();
        let random_bytes: [u8; 3] = rng.gen();
        
        // Convert to u32 for base32 encoding
        let value = u32::from_be_bytes([0, random_bytes[0], random_bytes[1], random_bytes[2]]);
        
        // Use base32 crockford alphabet (excludes I, L, O, U to avoid confusion)
        // This gives us ~16.7 million unique codes with 5 characters
        base32::encode(base32::Alphabet::Crockford, &value.to_be_bytes())
            .chars()
            .skip(3) // Skip padding zeros
            .take(5) // Take 5 characters for human readability
            .collect::<String>()
            .to_uppercase()
    }

    pub async fn save_token(&self, token: Token) -> Result<Token, ApiError> {
        // Check if token already exists by token_id
        if let Some(_) = self.tokens
            .find_one(doc! { "token_id": &token.token_id }, None)
            .await
            .map_err(ApiError::DatabaseError)? {
            return Err(ApiError::ValidationError(format!("Token with ID {} already exists", token.token_id)));
        }

        // Insert the token
        self.tokens
            .insert_one(token.clone(), None)
            .await
            .map_err(ApiError::DatabaseError)?;

        Ok(token)
    }


    pub async fn get_token_by_name(&self, token_name: &str) -> Result<Option<Token>, ApiError> {
        self.tokens
            .find_one(doc! { "token_name": token_name }, None)
            .await
            .map_err(ApiError::DatabaseError)
    }

    pub async fn get_token_by_symbol(&self, token_symbol: &str) -> Result<Option<Token>, ApiError> {
        self.tokens
            .find_one(doc! { "token_symbol": token_symbol }, None)
            .await
            .map_err(ApiError::DatabaseError)
    }

    pub async fn get_tokens_by_ids(&self, token_ids: &[String]) -> Result<Vec<Token>, ApiError> {
        let filter = doc! {
            "token_id": { "$in": token_ids }
        };
        self.tokens
            .find(filter, None)
            .await
            .map_err(ApiError::DatabaseError)?
            .try_collect()
            .await
            .map_err(ApiError::DatabaseError)
    }

    /// Get all tokens from the database
    pub async fn get_all_tokens(&self) -> Result<Vec<Token>, ApiError> {
        self.tokens
            .find(None, None)
            .await
            .map_err(ApiError::DatabaseError)?
            .try_collect()
            .await
            .map_err(ApiError::DatabaseError)
    }

    /// Update a token valuation for a user
    pub async fn update_user_valuation(
        &self,
        wallet_address: &str,
        symbol: &str,
        valuation: f64
    ) -> Result<(), ApiError> {
        // First ensure user exists
        let user = match self.get_user_by_wallet(wallet_address).await? {
            Some(user) => user,
            None => return Err(ApiError::NotFound(format!("User not found: {}", wallet_address))),
        };

        // Then ensure token exists by symbol
        if let None = self.get_token_by_symbol(symbol).await? {
            return Err(ApiError::NotFound(format!("Token not found: {}", symbol)));
        }

        // Update the user document using dot notation for efficiency
        self.users
            .update_one(
                doc! { "wallet_address": wallet_address },
                doc! { "$set": { format!("preferences.{}", symbol): valuation } }, // Changed from valuations to preferences
                None
            )
            .await
            .map_err(ApiError::DatabaseError)?;

        Ok(())
    }

    // Cause-related methods
    pub async fn create_cause(&self, cause: Cause) -> Result<String, mongodb::error::Error> {
        let result = self.causes.insert_one(cause, None).await?;
        Ok(result.inserted_id.as_object_id().unwrap().to_hex())
    }

    pub async fn get_cause_by_id(&self, id: &ObjectId) -> Result<Option<Cause>, mongodb::error::Error> {
        let filter = doc! { "_id": id };
        self.causes.find_one(filter, None).await
    }

    pub async fn get_cause_by_token_name(&self, token_name: &str) -> Result<Option<Cause>, mongodb::error::Error> {
        let filter = doc! { "token_name": { "$regex": token_name, "$options": "i" } };
        self.causes.find_one(filter, None).await
    }

    pub async fn get_cause_by_name(&self, name: &str) -> Result<Option<Cause>, mongodb::error::Error> {
        let filter = doc! { "name": { "$regex": name, "$options": "i" } };
        self.causes.find_one(filter, None).await
    }

    pub async fn get_cause_by_token_symbol(&self, token_symbol: &str) -> Result<Option<Cause>, mongodb::error::Error> {
        let filter = doc! { "token_symbol": { "$regex": token_symbol, "$options": "i" } };
        self.causes.find_one(filter, None).await
    }

    pub async fn get_all_causes(&self) -> Result<Vec<Cause>, mongodb::error::Error> {
        // Only return causes that are displayed
        let filter = doc! { "displayed": true };
        let cursor = self.causes.find(filter, None).await?;
        cursor.try_collect().await
    }
    
    pub async fn get_featured_causes(&self) -> Result<Vec<Cause>, mongodb::error::Error> {
        // Get causes that are both featured and displayed, sorted by creation date
        let filter = doc! { 
            "featured": true,
            "displayed": true 
        };
        let options = mongodb::options::FindOptions::builder()
            .sort(doc! { "created_at": -1 })
            .build();
        let cursor = self.causes.find(filter, options).await?;
        cursor.try_collect().await
    }
    
    pub async fn get_all_causes_unfiltered(&self) -> Result<Vec<Cause>, mongodb::error::Error> {
        // Admin method to get all causes regardless of display status
        let cursor = self.causes.find(None, None).await?;
        cursor.try_collect().await
    }

    pub async fn update_cause(&self, id: &ObjectId, update: UpdateCauseRequest) -> Result<bool, mongodb::error::Error> {
        // Build the update document based on provided fields
        let mut update_doc = doc! {};
        
        if let Some(name) = update.name {
            update_doc.insert("name", name);
        }
        if let Some(organization) = update.organization {
            update_doc.insert("organization", organization);
        }
        if let Some(description) = update.description {
            update_doc.insert("description", description);
        }
        if let Some(long_description) = update.long_description {
            update_doc.insert("long_description", long_description);
        }
        if let Some(is_active) = update.is_active {
            update_doc.insert("is_active", is_active);
        }
        if let Some(stripe_id) = update.stripe_product_id {
            update_doc.insert("stripe_product_id", stripe_id);
        }
        if let Some(payment_link) = update.payment_link {
            update_doc.insert("payment_link", payment_link);
        }
        if let Some(status) = update.status {
            update_doc.insert("status", status.to_string());
        }
        
        if let Some(token_id) = update.token_id {
            update_doc.insert("token_id", token_id);
        }
        if let Some(token_image_url) = update.token_image_url {
            update_doc.insert("token_image_url", token_image_url);
        }
        if let Some(cause_image_url) = update.cause_image_url {
            update_doc.insert("cause_image_url", cause_image_url);
        }
        if let Some(stripe_account_id) = update.stripe_account_id {
            update_doc.insert("stripe_account_id", stripe_account_id);
        }
        if let Some(stripe_account_status) = update.stripe_account_status {
            update_doc.insert("stripe_account_status", stripe_account_status);
        }
        if let Some(displayed) = update.displayed {
            update_doc.insert("displayed", displayed);
        }
        if let Some(featured) = update.featured {
            update_doc.insert("featured", featured);
        }

        // Add updated_at timestamp
        update_doc.insert("updated_at", chrono::Utc::now());
        
        let update = doc! { "$set": update_doc };
        let filter = doc! { "_id": id };
        
        let result = self.causes.update_one(filter, update, None).await?;
        Ok(result.modified_count > 0)
    }

    pub async fn delete_cause(&self, id: &ObjectId) -> Result<bool, mongodb::error::Error> {
        let filter = doc! { "_id": id };
        let result = self.causes.delete_one(filter, None).await?;
        Ok(result.deleted_count > 0)
    }

    pub async fn update_cause_bonding_curve(
        &self,
        id: &str,
        amount_donated: f64,
        tokens_purchased: f64,
        current_price: f64,
    ) -> Result<bool, mongodb::error::Error> {
        let object_id = ObjectId::parse_str(id).map_err(|e| mongodb::error::Error::custom(e))?;
        let filter = doc! { "_id": object_id };
        let update = doc! {
            "$set": {
                "amount_donated": amount_donated,
                "tokens_purchased": tokens_purchased,
                "current_price": current_price,
                "updated_at": chrono::Utc::now()
            }
        };
        
        let result = self.causes.update_one(filter, update, None).await?;
        Ok(result.modified_count > 0)
    }
    
    // Draft operations
    pub async fn create_draft(&self, draft: CauseDraft) -> Result<String, mongodb::error::Error> {
        match self.cause_drafts.insert_one(draft, None).await {
            Ok(result) => Ok(result.inserted_id.as_object_id().unwrap().to_hex()),
            Err(e) => {
                // Parse duplicate key errors to provide specific field information
                let error_str = e.to_string();
                if error_str.contains("E11000 duplicate key error") {
                    if error_str.contains("name_1") || error_str.contains("name:") {
                        return Err(mongodb::error::Error::custom(format!(
                            "DUPLICATE_NAME: A cause with this name already exists"
                        )));
                    } else if error_str.contains("token_name_1") || error_str.contains("token_name:") {
                        return Err(mongodb::error::Error::custom(format!(
                            "DUPLICATE_TOKEN_NAME: A cause with this token name already exists"
                        )));
                    } else if error_str.contains("token_symbol_1") || error_str.contains("token_symbol:") {
                        return Err(mongodb::error::Error::custom(format!(
                            "DUPLICATE_TOKEN_SYMBOL: A cause with this token symbol already exists"
                        )));
                    }
                }
                Err(e)
            }
        }
    }
    
    pub async fn get_draft_by_id(&self, id: &ObjectId) -> Result<Option<CauseDraft>, mongodb::error::Error> {
        self.cause_drafts.find_one(doc! { "_id": id }, None).await
    }
    
    pub async fn update_draft(&self, id: &ObjectId, update: Document) -> Result<bool, mongodb::error::Error> {
        let filter = doc! { "_id": id };
        let update = doc! { "$set": update };
        let result = self.cause_drafts.update_one(filter, update, None).await?;
        Ok(result.modified_count > 0)
    }
    
    pub async fn find_drafts_by_email(&self, email: &str) -> Result<Vec<CauseDraft>, mongodb::error::Error> {
        let filter = doc! { 
            "creator_email": email,
            "expires_at": { "$gt": chrono::Utc::now() }
        };
        
        self.cause_drafts
            .find(filter, None)
            .await?
            .try_collect()
            .await
    }
    
    
    // Validation methods for individual fields
    pub fn get_causes_collection(&self) -> &Collection<Cause> {
        &self.causes
    }

    pub async fn is_cause_name_taken(&self, name: &str) -> Result<bool, mongodb::error::Error> {
        let filter = doc! { 
            "name": { "$regex": &format!("^{}$", name), "$options": "i" }
        };
        let count = self.cause_drafts.count_documents(filter, None).await?;
        Ok(count > 0)
    }
    
    pub async fn is_token_symbol_taken(&self, token_symbol: &str) -> Result<bool, mongodb::error::Error> {
        let filter = doc! { 
            "token_symbol": { "$regex": &format!("^{}$", token_symbol), "$options": "i" }
        };
        let count = self.cause_drafts.count_documents(filter, None).await?;
        Ok(count > 0)
    }
    
    pub async fn is_token_name_taken(&self, token_name: &str) -> Result<bool, mongodb::error::Error> {
        let filter = doc! { 
            "token_name": { "$regex": &format!("^{}$", token_name), "$options": "i" }
        };
        let count = self.cause_drafts.count_documents(filter, None).await?;
        Ok(count > 0)
    }

    // Get user preferences from nested Document structure
    pub async fn get_user_preferences(&self, user_address: &str) -> Result<Document, ApiError> {
        let filter = doc! { "wallet_address": user_address };
        let user = self.users.find_one(filter, None).await
            .map_err(|e| ApiError::InternalError(format!("Database error: {}", e)))?
            .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;
        
        Ok(user.preferences.0) // Return the Document containing preferences
    }

    // Update user preferences after consuming discounts
    pub async fn update_user_preferences_after_payment(
        &self,
        user_address: &str,
        discount_consumptions: &[DiscountConsumption],
        _effective_valuations: Option<&[(String, f64)]>, // Deprecated parameter, kept for compatibility
    ) -> Result<(), ApiError> {
        // Get current preferences
        let current_prefs = self.get_user_preferences(user_address).await?;
        let mut updated_prefs = current_prefs.clone();
        
        // Apply discount consumptions
        for consumption in discount_consumptions {
            if consumption.amount_used > 0.0 {
                // Use token symbol as key (matching how preferences are stored)
                let token_symbol = &consumption.symbol;
                
                // Update the preference value for this token
                if let Some(current_value) = updated_prefs.get(token_symbol) {
                    if let Some(current_float) = current_value.as_f64() {
                        // Calculate new value after consuming discount/premium
                        // Positive values (discounts) decrease towards 0
                        // Negative values (premiums) increase towards 0
                        let new_value = if current_float > 0.0 {
                            // Positive value = discount available
                            // Reduce the discount by the amount consumed
                            let new_val = current_float - consumption.amount_used;
                            new_val.max(0.0) // Don't go below 0.0
                        } else if current_float < 0.0 {
                            // Negative value = premium charged
                            // Move towards 0 by the amount consumed (premium paid)
                            let new_val = current_float + consumption.amount_used;
                            new_val.min(0.0) // Don't go above 0.0
                        } else {
                            // Already at zero
                            0.0
                        };
                        
                        updated_prefs.insert(token_symbol.clone(), new_value);
                        log::info!("Updated {} preference from {} to {} after consuming {}", 
                                  token_symbol, current_float, new_value, consumption.amount_used);
                    }
                }
            }
        }
        
        // Note: We no longer store effective valuations in preferences
        // They are now stored in transaction records for market price calculation
        
        // Update user preferences in database
        let filter = doc! { "wallet_address": user_address };
        let update = doc! {
            "$set": {
                "preferences": bson::to_bson(&updated_prefs)
                    .map_err(|e| ApiError::InternalError(format!("Failed to serialize preferences: {}", e)))?
            }
        };
        
        self.users.update_one(filter, update, None).await
            .map_err(|e| ApiError::InternalError(format!("Failed to update user preferences: {}", e)))?;
        
        Ok(())
    }

    // Update payment with all calculated data
    pub async fn update_payment_with_calculations(
        &self,
        payment_id: &str,
        vendor_valuations: Vec<TokenValuation>,
        discount_consumption: Vec<DiscountConsumption>,
        computed_payment: Vec<TokenPayment>,
        initial_payment_bundle: Vec<TokenPayment>,
    ) -> Result<(), ApiError> {
        let filter = doc! { "payment_id": payment_id };
        let update = doc! {
            "$set": {
                "vendor_valuations": bson::to_bson(&vendor_valuations)
                    .map_err(|e| ApiError::InternalError(format!("Serialization error: {}", e)))?,
                "discount_consumption": bson::to_bson(&discount_consumption)
                    .map_err(|e| ApiError::InternalError(format!("Serialization error: {}", e)))?,
                "computed_payment": bson::to_bson(&computed_payment)
                    .map_err(|e| ApiError::InternalError(format!("Serialization error: {}", e)))?,
                "initial_payment_bundle": bson::to_bson(&initial_payment_bundle)
                    .map_err(|e| ApiError::InternalError(format!("Serialization error: {}", e)))?,
                "status": bson::to_bson(&PaymentStatus::Calculated)
                    .map_err(|e| ApiError::InternalError(format!("Failed to serialize status: {}", e)))?
            }
        };
        
        self.transactions.update_one(filter, update, None).await
            .map_err(|e| ApiError::InternalError(format!("Failed to update payment: {}", e)))?;
        
        Ok(())
    }
    
    /// Get payment by ID
    pub async fn get_payment_by_id(&self, payment_id: &str) -> Result<Payment, ApiError> {
        let filter = doc! { "payment_id": payment_id };
        self.transactions.find_one(filter, None).await
            .map_err(|e| ApiError::InternalError(format!("Database error: {}", e)))?
            .ok_or_else(|| ApiError::NotFound(format!("Payment {} not found", payment_id)))
    }

    /// Delete payment by ID (vendor can cancel)
    pub async fn delete_payment(&self, payment_id: &str, vendor_address: &str) -> Result<(), ApiError> {
        // First verify the payment exists and belongs to this vendor
        let payment = self.get_payment_by_id(payment_id).await?;
        
        // Check if the requester is the vendor
        if payment.vendor_address != vendor_address {
            return Err(ApiError::ValidationError("Only the vendor can cancel this payment".to_string()));
        }
        
        // Check if payment is already completed
        if matches!(payment.status, PaymentStatus::Completed) {
            return Err(ApiError::ValidationError("Cannot cancel completed payment".to_string()));
        }
        
        // Delete the payment
        let filter = doc! { "payment_id": payment_id };
        let result = self.transactions.delete_one(filter, None).await
            .map_err(|e| ApiError::DatabaseError(e))?;
            
        if result.deleted_count == 0 {
            return Err(ApiError::NotFound("Payment not found".to_string()));
        }
        
        log::info!("Payment {} deleted by vendor {}", payment_id, vendor_address);
        Ok(())
    }

    /// Update the status of a payment
    pub async fn update_payment_status(
        &self,
        payment_id: &str,
        status: PaymentStatus,
    ) -> Result<(), ApiError> {
        log::info!("Updating payment {} status to {:?}", payment_id, status);
        
        let filter = doc! { "payment_id": payment_id };
        let update = doc! {
            "$set": {
                "status": bson::to_bson(&status)
                    .map_err(|e| ApiError::InternalError(format!("Failed to serialize status: {}", e)))?
            }
        };
        
        self.transactions.update_one(filter, update, None).await
            .map_err(|e| {
                log::error!("Failed to update payment status: {}", e);
                ApiError::DatabaseError(e)
            })?;
        
        log::info!("Successfully updated payment {} status to {:?}", payment_id, status);
        Ok(())
    }

    // Deposit Records methods
    pub async fn save_deposit_record(&self, deposit: DepositRecord) -> Result<(), ApiError> {
        self.deposit_records
            .insert_one(deposit, None)
            .await
            .map_err(|e| ApiError::DatabaseError(e))?;
        Ok(())
    }
    
    pub async fn get_user_deposits(&self, wallet_address: &str) -> Result<Vec<DepositRecord>, ApiError> {
        let filter = doc! { "wallet_address": wallet_address };
        let mut cursor = self.deposit_records
            .find(filter, None)
            .await
            .map_err(|e| ApiError::DatabaseError(e))?;
        
        let mut deposits = Vec::new();
        while let Some(deposit) = cursor.try_next().await.map_err(|e| ApiError::DatabaseError(e))? {
            deposits.push(deposit);
        }
        
        Ok(deposits)
    }

    // Transaction Records methods for market price calculations
    pub async fn create_transaction_record(&self, record: TransactionRecord) -> Result<TransactionRecord, ApiError> {
        let result = self.transaction_records
            .insert_one(record.clone(), None)
            .await
            .map_err(ApiError::DatabaseError)?;
        
        log::info!("Created transaction record with ID: {:?}", result.inserted_id);
        Ok(record)
    }

    pub async fn get_recent_transactions_for_token(&self, token_key: &str, limit: i64) -> Result<Vec<TransactionRecord>, ApiError> {
        let cursor = self.transaction_records
            .find(doc! { "token_key": token_key }, None)
            .await
            .map_err(ApiError::DatabaseError)?;
        
        let mut records: Vec<TransactionRecord> = cursor
            .try_collect()
            .await
            .map_err(ApiError::DatabaseError)?;
        
        // Sort by timestamp descending (newest first) and limit
        records.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        records.truncate(limit as usize);
        
        Ok(records)
    }

    pub async fn update_token_market_price(&self, token_key: &str, new_price: f64) -> Result<(), ApiError> {
        let result = self.tokens
            .update_one(
                doc! { "token_id": token_key },
                doc! { "$set": { "market_valuation": new_price } },
                None
            )
            .await
            .map_err(ApiError::DatabaseError)?;
        
        if result.matched_count == 0 {
            log::warn!("No token found with token_key: {}", token_key);
        } else {
            log::info!("Updated market price for token {}: {}", token_key, new_price);
        }
        
        Ok(())
    }


    /// Get transaction history for a user address (as vendor or customer)
    pub async fn get_user_transaction_history(&self, user_address: &str) -> Result<Vec<Payment>, ApiError> {
        let filter = doc! {
            "$or": [
                { "vendor_address": user_address },
                { "customer_address": user_address }
            ]
        };
        
        let mut cursor = self.transactions
            .find(filter, None)
            .await
            .map_err(ApiError::DatabaseError)?;
        
        let mut payments = Vec::new();
        while let Some(payment) = cursor.try_next().await.map_err(ApiError::DatabaseError)? {
            payments.push(payment);
        }
        
        // Sort by created_at descending (newest first)
        payments.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        
        Ok(payments)
    }
}