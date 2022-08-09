/*!
Fungible Token implementation with JSON serialization.
NOTES:
  - The maximum balance value is limited by U128 (2**128 - 1).
  - JSON calls should pass U128 as a base-10 string. E.g. "100".
  - The contract optimizes the inner trie structure by hashing account IDs. It will prevent some
    abuse of deep tries. Shouldn't be an issue, once NEAR clients implement full hashing of keys.
  - The contract tracks the change in storage before and after the call. If the storage increases,
    the contract requires the caller of the contract to attach enough deposit to the function call
    to cover the storage cost.
    This is done to prevent a denial of service attack on the contract by taking all available storage.
    If the storage decreases, the contract will issue a refund for the cost of the released storage.
    The unused tokens from the attached deposit are also refunded, so it's safe to
    attach more deposit than required.
  - To prevent the deployed contract from being modified or deleted, it should not have any access
    keys on its account.
*/
use near_contract_standards::fungible_token::metadata::{
    FungibleTokenMetadata, FungibleTokenMetadataProvider, FT_METADATA_SPEC,
};
use near_contract_standards::fungible_token::FungibleToken;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, UnorderedSet};
use near_sdk::json_types::U128;
use near_sdk::{
    env, log, near_bindgen, AccountId, Balance, BorshStorageKey, PanicOnDefault, PromiseOrValue,
};

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct AnyToken {
    token: FungibleToken,
    metadata: LazyOption<FungibleTokenMetadata>,
    txs: UnorderedSet<String>,
    mpc_id: AccountId,
    new_mpc_id: AccountId,
    check_tx_hash: bool,
}

const DATA_IMAGE_SVG_NEAR_ICON: &str = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 288 288'%3E%3Cg id='l' data-name='l'%3E%3Cpath d='M187.58,79.81l-30.1,44.69a3.2,3.2,0,0,0,4.75,4.2L191.86,103a1.2,1.2,0,0,1,2,.91v80.46a1.2,1.2,0,0,1-2.12.77L102.18,77.93A15.35,15.35,0,0,0,90.47,72.5H87.34A15.34,15.34,0,0,0,72,87.84V201.16A15.34,15.34,0,0,0,87.34,216.5h0a15.35,15.35,0,0,0,13.08-7.31l30.1-44.69a3.2,3.2,0,0,0-4.75-4.2L96.14,186a1.2,1.2,0,0,1-2-.91V104.61a1.2,1.2,0,0,1,2.12-.77l89.55,107.23a15.35,15.35,0,0,0,11.71,5.43h3.13A15.34,15.34,0,0,0,216,201.16V87.84A15.34,15.34,0,0,0,200.66,72.5h0A15.35,15.35,0,0,0,187.58,79.81Z'/%3E%3C/g%3E%3C/svg%3E";

#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
    FungibleToken,
    Metadata,
    TxHash,
}

#[near_bindgen]
impl AnyToken {
    /// Initializes the contract with the given total supply owned by the given `owner_id` with
    /// default metadata (for example purposes only).
    #[init]
    pub fn new_default_meta(
        mpc_id: AccountId,
        total_supply: U128,
        check_tx_hash: bool,
        name: String,
        symbol: String,
        decimals: u8,
    ) -> Self {
        Self::new(
            mpc_id,
            total_supply,
            FungibleTokenMetadata {
                spec: FT_METADATA_SPEC.to_string(),
                name,
                symbol,
                icon: Some(DATA_IMAGE_SVG_NEAR_ICON.to_string()),
                reference: None,
                reference_hash: None,
                decimals,
            },
            check_tx_hash,
        )
    }

    /// Initializes the contract with the given total supply owned by the given `owner_id` with
    /// the given fungible token metadata.
    #[init]
    pub fn new(
        mpc_id: AccountId,
        total_supply: U128,
        metadata: FungibleTokenMetadata,
        check_tx_hash: bool,
    ) -> Self {
        assert!(!env::state_exists(), "AnyToken: Already initialized");
        metadata.assert_valid();
        let mut this = Self {
            token: FungibleToken::new(StorageKey::FungibleToken),
            metadata: LazyOption::new(StorageKey::Metadata, Some(&metadata)),
            txs: UnorderedSet::new(StorageKey::TxHash),
            mpc_id: mpc_id.clone(),
            new_mpc_id: mpc_id.clone(),
            check_tx_hash,
        };
        this.token.internal_register_account(&mpc_id);
        this.token.internal_deposit(&mpc_id, total_supply.into());
        this
    }

    fn on_account_closed(&mut self, account_id: AccountId, balance: Balance) {
        log!("Closed @{} with {}", account_id, balance);
    }

    fn on_tokens_burned(&mut self, account_id: AccountId, amount: Balance) {
        log!("Account @{} burned {}", account_id, amount);
    }

    pub fn set_check_flag(&mut self, flag: bool) {
        assert!(env::predecessor_account_id() == self.mpc_id, "FORBIDDEN");
        self.check_tx_hash = flag;
        log!("Update check tx hash flag to {}", flag);
    }

    pub fn mpc_id(&self) -> AccountId {
        self.mpc_id.clone()
    }

    pub fn change_mpc_id(&mut self, new_mpc_id: AccountId) {
        assert!(env::predecessor_account_id() == self.mpc_id, "FORBIDDEN");
        self.new_mpc_id = new_mpc_id;
    }

    pub fn apply_mpc_id(&mut self) {
        assert!(
            env::predecessor_account_id() == self.new_mpc_id,
            "FORBIDDEN"
        );
        self.mpc_id = self.new_mpc_id.clone();
    }

    pub fn swap_in(
        &mut self,
        tx_hash: String,
        receiver_id: AccountId,
        amount: U128,
        from_chain_id: U128,
    ) {
        assert!(env::predecessor_account_id() == self.mpc_id, "FORBIDDEN");
        if self.check_tx_hash {
            assert!(!self.txs.contains(&tx_hash), "Txhash is exist");
        }
        assert!(amount.0 > 0, "The amount should be a positive number");
        self.txs.insert(&tx_hash);
        if !self.token.accounts.contains_key(&receiver_id) {
            self.token.internal_register_account(&receiver_id);
        };
        self.token.internal_deposit(&receiver_id, amount.0);
        log!(
            "SwapIn {} receiver_id {} amount {} from_chain_id {}",
            env::current_account_id(),
            receiver_id,
            amount.0,
            from_chain_id.0
        );
    }

    pub fn swap_out(&mut self, receiver_id: AccountId, amount: U128, to_chain_id: U128) {
        assert!(amount.0 > 0, "The amount should be a positive number");
        let sender: AccountId = env::predecessor_account_id().try_into().unwrap();
        self.token.internal_withdraw(&sender, amount.0);
        log!(
            "SwapOut sender_id {} receiver_id {} amount {} to_chain_id {}",
            sender,
            receiver_id,
            amount.0,
            to_chain_id.0
        );
    }
}

near_contract_standards::impl_fungible_token_core!(AnyToken, token, on_tokens_burned);
near_contract_standards::impl_fungible_token_storage!(AnyToken, token, on_account_closed);

#[near_bindgen]
impl FungibleTokenMetadataProvider for AnyToken {
    fn ft_metadata(&self) -> FungibleTokenMetadata {
        self.metadata.get().unwrap()
    }
}
