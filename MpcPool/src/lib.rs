use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::{env, log, near_bindgen, AccountId};

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, Default)]
pub struct MpcPool {}

#[near_bindgen]
impl MpcPool {
    /// Initializes the contract with the given total supply owned by the given `owner_id` with
    /// the given fungible token metadata.
    #[init]
    pub fn new() -> Self {
        assert!(!env::state_exists(), "AnyToken: Already initialized");
        let this = Self {};
        this
    }

    #[payable]
    pub fn swap_out(&mut self, receiver_id: AccountId, to_chain_id: U128) {
        let amount = env::attached_deposit();
        assert!(amount > 0, "The amount should be a positive number");
        let sender = env::predecessor_account_id();
        log!(
            "SwapOutNative sender_id {} receiver_id {} amount {} to_chain_id {}",
            sender,
            receiver_id,
            amount,
            to_chain_id.0
        );
    }
}
