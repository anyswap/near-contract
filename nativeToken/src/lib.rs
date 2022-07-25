use near_sdk::{
    env, log, near_bindgen, AccountId, Balance, BorshStorageKey, PanicOnDefault, PromiseOrValue,
};

near_sdk::setup_alloc!();

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct MpcPool {
}

#[near_bindgen]
impl MpcPool {

    /// Initializes the contract with the given total supply owned by the given `owner_id` with
    /// the given fungible token metadata.
    #[init]
    pub fn new(
    ) -> Self {
        assert!(!env::state_exists(), "AnyToken: Already initialized");
        let mut this = Self {};
    }

    [#payable]
    pub fn swap_out(&mut self, receiver_id: AccountId, to_chain_id: U128) {
        let amount = env::attached_deposit();
        assert!(amount.0 > 0, "The amount should be a positive number");
        let sender: ValidAccountId = env::predecessor_account_id().try_into().unwrap();
        log!(
            "SwapOutNative {} sender_id {} receiver_id {} amount {} to_chain_id {}",
            env::current_account_id(),
            sender,
            receiver_id,
            amount.0,
            to_chain_id.0
        );
    }
}
