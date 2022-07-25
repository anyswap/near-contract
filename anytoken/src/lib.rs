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
use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_contract_standards::fungible_token::FungibleToken;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, UnorderedSet};
use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::{
    assert_one_yocto, env, ext_contract, log, near_bindgen, AccountId, Balance, BorshStorageKey,
    Gas, PanicOnDefault, Promise, PromiseOrValue, PromiseResult,
};

near_sdk::setup_alloc!();

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct AnyToken {
    token: FungibleToken,
    metadata: LazyOption<FungibleTokenMetadata>,
    txs: UnorderedSet<String>,
    mpc_id: AccountId,
    underlying: Option<AccountId>,
}

const DATA_IMAGE_SVG_NEAR_ICON: &str = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 288 288'%3E%3Cg id='l' data-name='l'%3E%3Cpath d='M187.58,79.81l-30.1,44.69a3.2,3.2,0,0,0,4.75,4.2L191.86,103a1.2,1.2,0,0,1,2,.91v80.46a1.2,1.2,0,0,1-2.12.77L102.18,77.93A15.35,15.35,0,0,0,90.47,72.5H87.34A15.34,15.34,0,0,0,72,87.84V201.16A15.34,15.34,0,0,0,87.34,216.5h0a15.35,15.35,0,0,0,13.08-7.31l30.1-44.69a3.2,3.2,0,0,0-4.75-4.2L96.14,186a1.2,1.2,0,0,1-2-.91V104.61a1.2,1.2,0,0,1,2.12-.77l89.55,107.23a15.35,15.35,0,0,0,11.71,5.43h3.13A15.34,15.34,0,0,0,216,201.16V87.84A15.34,15.34,0,0,0,200.66,72.5h0A15.35,15.35,0,0,0,187.58,79.81Z'/%3E%3C/g%3E%3C/svg%3E";
const ONE_YOCTO_DEPOSIT: Balance = 1;
const BASE_GAS: Gas = 5_000_000_000_000;
const NOT_DEPOSIT: Balance = 0;

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
        mpc_id: ValidAccountId,
        underlying: Option<AccountId>,
        total_supply: U128,
    ) -> Self {
        Self::new(
            mpc_id,
            total_supply,
            FungibleTokenMetadata {
                spec: FT_METADATA_SPEC.to_string(),
                name: "Example AnyToken".to_string(),
                symbol: "AnyToken".to_string(),
                icon: Some(DATA_IMAGE_SVG_NEAR_ICON.to_string()),
                reference: None,
                reference_hash: None,
                decimals: 24,
            },
            underlying,
        )
    }

    /// Initializes the contract with the given total supply owned by the given `owner_id` with
    /// the given fungible token metadata.
    #[init]
    pub fn new(
        mpc_id: ValidAccountId,
        total_supply: U128,
        metadata: FungibleTokenMetadata,
        underlying: Option<AccountId>,
    ) -> Self {
        assert!(!env::state_exists(), "AnyToken: Already initialized");
        metadata.assert_valid();
        let mut this = Self {
            token: FungibleToken::new(StorageKey::FungibleToken),
            metadata: LazyOption::new(StorageKey::Metadata, Some(&metadata)),
            txs: UnorderedSet::new(StorageKey::TxHash),
            mpc_id: mpc_id.to_string(),
            underlying: underlying,
        };
        this.token.internal_register_account(mpc_id.as_ref());
        this.token
            .internal_deposit(mpc_id.as_ref(), total_supply.into());
        this
    }

    fn on_account_closed(&mut self, account_id: AccountId, balance: Balance) {
        log!("Closed @{} with {}", account_id, balance);
    }

    fn on_tokens_burned(&mut self, account_id: AccountId, amount: Balance) {
        log!("Account @{} burned {}", account_id, amount);
    }

    pub fn underlying(&self) -> AccountId {
        match &self.underlying {
            Some(underlying) => underlying.to_string(),
            _ => String::from(""),
        }
    }

    pub fn set_underlying(&mut self, account_id: AccountId) {
        assert!(
            env::predecessor_account_id() == self.mpc_id,
            "AnyToken: FORBIDDEN"
        );
        assert!(
            self.underlying() == String::from(""),
            "AnyToken: underlying exists"
        );
        self.underlying = Some(account_id);
    }

    #[payable]
    pub fn withdraw_underlying(&mut self, receiver_id: ValidAccountId, amount: U128) {
        assert_one_yocto();
        let sender: ValidAccountId = env::predecessor_account_id().try_into().unwrap();
        assert!(
            self.ft_balance_of(sender.clone()).0 >= amount.0,
            "The withdraw amount must less than balances"
        );
        self.token.internal_withdraw(&sender.to_string(), amount.0);
        match self.underlying() == "" {
            true => {
                self.token.internal_deposit(&sender.to_string(), amount.0);
            }
            false => {
                ext_fungible_token::ft_transfer(
                    receiver_id.to_string(),
                    amount,
                    None,
                    &self.underlying(),
                    ONE_YOCTO_DEPOSIT,
                    BASE_GAS,
                )
                .then(ext_self::ft_transfer_callback(
                    sender.to_string(),
                    amount,
                    &env::current_account_id(),
                    NOT_DEPOSIT,
                    BASE_GAS,
                ));
            }
        }
    }

    #[payable]
    pub fn swap_out_native(&mut self, receiver_id: ValidAccountId, to_chain_id: String) -> U128 {
        assert!(
            self.underlying() == "near",
            "This contract can't deposit native"
        );
        let amount = env::attached_deposit();
        assert!(amount > 0, "The amount should be a positive number");
        log!(
            "SwapOutNative {} sender_id {} receiver_id {} amount {} to_chain_id {}",
            self.underlying(),
            env::predecessor_account_id(),
            receiver_id,
            amount,
            to_chain_id
        );
        U128::from(amount)
    }

    pub fn swap_in(
        &mut self,
        tx_hash: String,
        receiver_id: AccountId,
        amount: U128,
        from_chain_id: String,
    ) {
        assert!(env::predecessor_account_id() == self.mpc_id, "FORBIDDEN");
        assert!(!self.txs.contains(&tx_hash), "Txhash is exist");
        assert!(amount.0 > 0, "The amount should be a positive number");
        self.txs.insert(&tx_hash);
        match self.underlying() == "" {
            true => {
                if !self.token.accounts.contains_key(&receiver_id) {
                    self.token.internal_register_account(&receiver_id);
                };
                self.token.internal_deposit(&receiver_id, amount.0);
                log!(
                    "SwapIn {} receiver_id {} amount {} from_chain_id {}",
                    env::current_account_id(),
                    receiver_id,
                    amount.0,
                    from_chain_id
                );
            }
            false => {
                ext_fungible_token::ft_transfer(
                    receiver_id.to_string(),
                    amount,
                    None,
                    &self.underlying(),
                    ONE_YOCTO_DEPOSIT,
                    BASE_GAS,
                )
                .then(ext_self::ft_transfer_callback(
                    receiver_id.to_string(),
                    amount,
                    &env::current_account_id(),
                    NOT_DEPOSIT,
                    BASE_GAS,
                ));
                log!(
                    "SwapIn {} receiver_id {} amount {} from_chain_id {}",
                    self.underlying(),
                    receiver_id,
                    amount.0,
                    from_chain_id
                );
            }
        }
    }

    pub fn swap_in_native(
        &mut self,
        tx_hash: String,
        receiver_id: AccountId,
        amount: U128,
        from_chain_id: String,
    ) {
        assert!(env::predecessor_account_id() == self.mpc_id, "FORBIDDEN");
        assert!(
            self.underlying() == "near",
            "This contract can't withdraw native"
        );
        assert!(!self.txs.contains(&tx_hash), "Txhash is exist");
        assert!(amount.0 > 0, "The amount should be a positive number");
        self.txs.insert(&tx_hash);
        Promise::new(receiver_id.clone()).transfer(amount.0).then(
            ext_self::native_transfer_callback(
                receiver_id.clone(),
                amount,
                &env::current_account_id(),
                NOT_DEPOSIT,
                BASE_GAS,
            ),
        );
        log!(
            "SwapInNative {} receiver_id {} amount {} from_chain_id {}",
            self.underlying(),
            receiver_id,
            amount.0,
            from_chain_id
        );
    }

    #[private]
    pub fn ft_transfer_callback(&mut self, sender: AccountId, amount: U128) {
        assert_eq!(env::promise_results_count(), 1, "ERR_TOO_MANY_RESULTS");
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(..) => {}
            PromiseResult::Failed => {
                self.token.internal_deposit(&sender.to_string(), amount.0);
            }
        }
    }

    #[private]
    pub fn deposit_underlying(
        &mut self,
        sender_id: ValidAccountId,
        account_id: AccountId,
        amount: U128,
    ) {
        if !self.token.accounts.contains_key(&account_id) {
            self.token.internal_register_account(&account_id);
        };
        self.token.internal_deposit(&account_id, amount.0);
        log!(
            "Deposit Underlying {} sender_id {} receiver_id {}",
            amount.0,
            sender_id,
            account_id
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

#[near_bindgen]
impl FungibleTokenReceiver for AnyToken {
    /// If given `msg: "take-my-money", immediately returns U128::From(0)
    /// Otherwise, makes a cross-contract call to own `value_please` function, passing `msg`
    /// value_please will attempt to parse `msg` as an integer and return a U128 version of it
    fn ft_on_transfer(
        &mut self,
        sender_id: ValidAccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        let receipt_token = env::predecessor_account_id();
        // Verifying that we were called by fungible token contract that we expect.
        assert!(
            receipt_token == self.underlying() || receipt_token == env::current_account_id(),
            "Only supports current contract or underlying contract"
        );
        let decode_msg: Vec<&str> = msg.as_str().split(":").collect();
        match decode_msg[0] {
            "deposit_underlying" => {
                // deposit_underlying:receiver_id
                assert_eq!(
                    receipt_token,
                    self.underlying(),
                    "Only supports the one fungible token contract"
                );
                assert!(
                    decode_msg.len() == 2,
                    "decode deposit_underlying msg error!"
                );
                self.deposit_underlying(sender_id, decode_msg[1].to_string(), amount);
                PromiseOrValue::Value(U128::from(0))
            }
            "swap_out" => {
                // swap_out:receiver_id:toChainID
                assert!(decode_msg.len() == 3, "decode swap_out msg error!");
                match receipt_token == self.underlying() {
                    true => {}
                    false => {
                        self.token
                            .internal_withdraw(&env::current_account_id(), amount.0);
                    }
                }
                log!(
                    "SwapOut {} sender_id {} receiver_id {} amount {} to_chain_id {}",
                    receipt_token,
                    amount.0,
                    sender_id,
                    decode_msg[1],
                    decode_msg[2]
                );
                PromiseOrValue::Value(U128::from(0))
            }
            _ => {
                log!("Router: msg parse not match");
                PromiseOrValue::Value(amount)
            }
        }
    }
}

#[ext_contract(ext_fungible_token)]
pub trait FungibleTokenContract {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
}

#[ext_contract(ext_self)]
pub trait ExtSelf {
    fn native_transfer_callback(&mut self, sender: AccountId, amount: U128);

    fn ft_transfer_callback(&mut self, sender: AccountId, amount: U128);
}
