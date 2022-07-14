use std::any;

use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    env, ext_contract, log, near_bindgen, serde_json, AccountId, Balance, BorshStorageKey, Gas,
    PanicOnDefault, Promise, PromiseOrValue, PromiseResult,
};

near_sdk::setup_alloc!();

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Router {
    pending_mpc_id: Option<AccountId>,
    mpc_id: AccountId,
    chain_id: String,
    txs: UnorderedMap<(String, u8), bool>,
    any_near: AccountId,
    underlying_to_anytoken: UnorderedMap<AccountId, AccountId>,
    anytoken_to_underlying: UnorderedMap<AccountId, AccountId>,
    base_gas: Gas,
    pause_in: bool,
    pause_out: bool,
    pause_all: bool,
}
const NOT_DEPOSIT: Balance = 0;
const ONE_YOCTO_DEPOSIT: Balance = 1;
const BASE_GAS: Gas = 5_000_000_000_000;

#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
    Txs,
    Underlying,
    AnyToken,
}

/// Message parameters to receive via token function call.
#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
#[serde(untagged)]
enum Actions {
    AnySwapOut {
        bind_id: AccountId,
        to_chain_id: U128,
    },
    DepositUnderlying {
        receiver_id: AccountId,
    },
    Withdraw {
        to: AccountId,
    },
}

#[near_bindgen]
impl Router {
    #[init]
    pub fn new(mpc_id: AccountId, any_near: AccountId, chain_id: String) -> Self {
        assert!(!env::state_exists(), "Already initialized");
        Self {
            pending_mpc_id: None,
            mpc_id,
            any_near,
            chain_id,
            txs: UnorderedMap::new(StorageKey::Txs),
            underlying_to_anytoken: UnorderedMap::new(StorageKey::Underlying),
            anytoken_to_underlying: UnorderedMap::new(StorageKey::AnyToken),
            base_gas: BASE_GAS,
            pause_in: false,
            pause_out: false,
            pause_all: false,
        }
    }

    #[private]
    fn valid_mpc_id(&self) {
        assert_eq!(
            env::predecessor_account_id(),
            self.mpc_id,
            "Router: only mpc"
        );
    }

    #[private]
    fn log_swap_out(
        &self,
        token: AccountId,
        from: AccountId,
        to: AccountId,
        amount: U128,
        to_chain_id: U128,
    ) {
        log!(
            "LogSwapOut token {} from {} to {} amount {} fromChainId {} toChainId {}",
            token,
            from,
            to,
            amount.0,
            self.chain_id,
            to_chain_id.0
        );
    }

    pub fn mpc_id(&self) -> AccountId {
        self.mpc_id.to_string()
    }

    pub fn pending_mpc_id(&self) -> AccountId {
        match &self.pending_mpc_id {
            Some(pending_mpc_id) => pending_mpc_id.to_string(),
            _ => String::from(""),
        }
    }

    pub fn change_mpc_id(&mut self, new_mpc_id: AccountId) {
        self.pending_mpc_id = Some(new_mpc_id);
    }

    pub fn apply_mpc_id(&mut self) {
        self.valid_mpc_id();
        let new_mpc_id = self.pending_mpc_id();
        assert!(
            new_mpc_id != String::from(""),
            "Router: must call change_mpc_id before this"
        );
        log!(
            "Router: Change Mpc Id from {} to {}",
            self.mpc_id,
            new_mpc_id
        );
        self.pending_mpc_id = None;
        self.mpc_id = new_mpc_id;
    }

    pub fn set_pause_in(&mut self, pause_in: bool) {
        self.valid_mpc_id();
        self.pause_in = pause_in;
    }

    pub fn set_pause_out(&mut self, pause_out: bool) {
        self.valid_mpc_id();
        self.pause_out = pause_out;
    }

    pub fn set_pause_all(&mut self, pause_all: bool) {
        self.valid_mpc_id();
        self.pause_all = pause_all;
    }

    pub fn set_underlying_and_anytoken(&mut self, underlying: AccountId, any_token: AccountId) {
        self.valid_mpc_id();
        self.underlying_to_anytoken.insert(&underlying, &any_token);
        self.anytoken_to_underlying.insert(&any_token, &underlying);
    }

    pub fn underlying_to_anytoken(&self, underlying: AccountId) -> Option<AccountId> {
        self.underlying_to_anytoken.get(&underlying)
    }

    pub fn anytoken_to_underlying(&self, any_token: AccountId) -> Option<AccountId> {
        self.anytoken_to_underlying.get(&any_token)
    }

    pub fn any_near(&self) -> AccountId {
        self.any_near.to_string()
    }

    pub fn change_any_near(&mut self, new_any_near: AccountId) {
        self.valid_mpc_id();
        self.any_near = new_any_near;
    }

    pub fn base_gas(&self) -> Gas {
        self.base_gas
    }

    pub fn set_base_gas(&mut self, gas: Gas) {
        self.valid_mpc_id();
        self.base_gas = gas;
    }

    pub fn any_swap_in_gas(&self) -> Gas {
        self.base_gas * 4
    }

    pub fn chain_id(&self) -> String {
        self.chain_id.clone()
    }

    pub fn change_chain_id(&mut self, new_chain_id: String) {
        self.valid_mpc_id();
        self.chain_id = new_chain_id;
    }

    pub fn check_tx(&self, txhash: String, index: u8) -> bool {
        self.txs.get(&(txhash, index)) == Some(true)
    }

    pub fn all_txs(&self) -> Vec<((String, u8), bool)> {
        self.txs.to_vec()
    }

    #[private]
    fn valid_tx(&mut self, tx: String, index: u8, amount: u128) {
        self.valid_mpc_id();
        assert!(!self.pause_in && !self.pause_all, "Router: pause");
        assert!(!self.check_tx(tx.clone(), index), "Router: tx exists");
        assert!(amount > 0, "The amount should be a positive number");
        self.txs.insert(&(tx.clone(), index), &true);
    }

    pub fn any_swap_in(
        &mut self,
        tx: String,
        index: u8,
        token: AccountId,
        to: AccountId,
        amount: U128,
        from_chain_id: String,
    ) -> PromiseOrValue<U128> {
        self.valid_tx(tx.clone(), index, amount.0);
        match self.anytoken_to_underlying.get(&token) {
            Some(underlying) => ext_fungible_token::ft_transfer(
                to.to_string(),
                amount,
                None,
                &underlying,
                ONE_YOCTO_DEPOSIT,
                self.base_gas,
            )
            .then(ext_self::any_swap_in_callback(
                tx,
                index,
                token,
                to,
                amount,
                from_chain_id,
                &env::current_account_id(),
                NOT_DEPOSIT,
                self.any_swap_in_gas(),
            ))
            .into(),
            _ => {
                log!(
                    "LogSwapInAnyToken txs {} token {} to {} amount {} fromChainId {} toChainId {}",
                    tx,
                    token,
                    to,
                    amount.0,
                    from_chain_id,
                    self.chain_id
                );
                ext_any_token::mint(to.to_string(), amount, &token, NOT_DEPOSIT, self.base_gas)
                    .into()
            }
        }
    }

    pub fn swap_in_native(
        &mut self,
        tx: String,
        index: u8,
        to: AccountId,
        amount: U128,
        from_chain_id: String,
    ) -> PromiseOrValue<U128> {
        self.valid_tx(tx.clone(), index, amount.0);
        Promise::new(to.to_string())
            .transfer(amount.0)
            .then(ext_self::any_swap_in_native_callback(
                tx,
                index,
                to,
                amount,
                from_chain_id,
                &env::current_account_id(),
                NOT_DEPOSIT,
                self.any_swap_in_gas(),
            ))
            .into()
    }

    pub fn any_swap_in_all(
        &mut self,
        tx: String,
        index: u8,
        token: AccountId,
        to: AccountId,
        amount: U128,
        from_chain_id: String,
    ) -> PromiseOrValue<U128> {
        match token == self.any_near {
            true => self.swap_in_native(tx, index, to, amount, from_chain_id),
            _ => self
                .any_swap_in(tx, index, to, token, amount, from_chain_id)
                .into(),
        }
    }

    #[payable]
    pub fn swap_out_native(&mut self, to: String, to_chain_id: U128) -> PromiseOrValue<U128> {
        assert!(!self.pause_out && !self.pause_all, "Router: swapOut pause");
        let amount = env::attached_deposit();
        assert!(amount > 0, "The amount should be a positive number");
        log!(
            "LogSwapOutNative token {} from {} to {} amount {} fromChainId {} toChainId {}",
            self.any_near,
            env::predecessor_account_id(),
            to,
            amount,
            self.chain_id,
            to_chain_id.0
        );
        PromiseOrValue::Value(U128::from(0))
    }

    #[payable]
    pub fn deposit_near(&mut self, to: AccountId) -> PromiseOrValue<U128> {
        let amount = env::attached_deposit();
        assert!(amount > 0, "The amount should be a positive number");
        log!(
            "LogDepositNear token {} from {} to {} amount {}",
            self.any_near(),
            env::predecessor_account_id(),
            to.to_string(),
            amount,
        );
        ext_any_token::mint(
            to.to_string(),
            U128::from(amount),
            &self.any_near(),
            NOT_DEPOSIT,
            self.base_gas,
        )
        .into()
    }

    #[private]
    pub fn transfer_callback(&mut self, amount: U128) -> PromiseOrValue<U128> {
        assert_eq!(env::promise_results_count(), 1, "ERR_TOO_MANY_RESULTS");
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(..) => PromiseOrValue::Value(U128::from(0)),
            PromiseResult::Failed => env::panic(b"ERR_TRANSFER_TOKEN_WHEN_WITHDRAW"),
        }
    }

    #[private]
    pub fn any_swap_out_callback(
        &mut self,
        token: AccountId,
        from: AccountId,
        to: AccountId,
        amount: U128,
        to_chain_id: U128,
    ) -> PromiseOrValue<U128> {
        assert_eq!(env::promise_results_count(), 1, "ERR_TOO_MANY_RESULTS");
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(..) => {
                self.log_swap_out(token, from, to, amount, to_chain_id);
                PromiseOrValue::Value(U128::from(0))
            }
            PromiseResult::Failed => env::panic(b"ERR_TRANSFER_TOKEN_WHEN_WITHDRAW"),
        }
    }

    #[private]
    pub fn mint_callback(
        &mut self,
        tx: String,
        token: AccountId,
        index: u8,
        to: AccountId,
        amount: U128,
        from_chain_id: String,
    ) -> PromiseOrValue<U128> {
        assert_eq!(env::promise_results_count(), 1, "ERR_TOO_MANY_RESULTS");
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(..) => {
                log!(
                    "LogSwapIn txs {} index {} token {} to {} amount {} fromChainId {} toChainId {}",
                    tx,
                    index,
                    token,
                    to,
                    amount.0,
                    from_chain_id,
                    self.chain_id
                );
                PromiseOrValue::Value(U128::from(0))
            }
            PromiseResult::Failed => env::panic(b"ERR_MINT_ANYTOKEN_WHEN_SWAP_IN"),
        }
    }

    #[private]
    pub fn any_swap_in_callback(
        &mut self,
        tx: String,
        token: AccountId,
        index: u8,
        to: AccountId,
        amount: U128,
        from_chain_id: String,
    ) -> PromiseOrValue<U128> {
        assert_eq!(env::promise_results_count(), 1, "ERR_TOO_MANY_RESULTS");
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(..) => {
                log!(
                    "LogSwapIn txs {} index {} token {} to {} amount {} fromChainId {} toChainId {}",
                    tx,
                    index,
                    token,
                    to,
                    amount.0,
                    from_chain_id,
                    self.chain_id
                );
                PromiseOrValue::Value(U128::from(0))
            }
            PromiseResult::Failed => {
                ext_any_token::mint(to.to_string(), amount, &token, NOT_DEPOSIT, self.base_gas)
                    .then(ext_self::mint_callback(
                        tx,
                        index,
                        token,
                        to,
                        amount,
                        from_chain_id,
                        &env::current_account_id(),
                        NOT_DEPOSIT,
                        self.base_gas,
                    ))
                    .into()
            }
        }
    }

    #[private]
    pub fn any_swap_in_native_callback(
        &mut self,
        tx: String,
        index: u8,
        to: AccountId,
        amount: U128,
        from_chain_id: String,
    ) -> PromiseOrValue<U128> {
        assert_eq!(env::promise_results_count(), 1, "ERR_TOO_MANY_RESULTS");
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(..) => {
                log!(
                    "LogSwapInNative txs {} index {} token {} to {} amount {} fromChainId {} toChainId {}",
                    tx,
                    index,
                    self.any_near(),
                    to,
                    amount.0,
                    from_chain_id,
                    self.chain_id
                );
                PromiseOrValue::Value(U128::from(0))
            }
            PromiseResult::Failed => ext_any_token::mint(
                to.to_string(),
                amount,
                &self.any_near(),
                NOT_DEPOSIT,
                self.base_gas,
            )
            .then(ext_self::mint_callback(
                tx,
                index,
                self.any_near(),
                to,
                amount,
                from_chain_id,
                &env::current_account_id(),
                NOT_DEPOSIT,
                self.base_gas,
            ))
            .into(),
        }
    }
}

#[ext_contract(ext_self)]
pub trait ExtSelf {
    fn any_swap_in_callback(
        &mut self,
        tx: String,
        index: u8,
        token: AccountId,
        to: AccountId,
        amount: U128,
        from_chain_id: String,
    ) -> PromiseOrValue<U128>;
    fn any_swap_in_native_callback(
        &mut self,
        tx: String,
        index: u8,
        to: AccountId,
        amount: U128,
        from_chain_id: String,
    ) -> PromiseOrValue<U128>;
    fn mint_callback(
        &mut self,
        tx: String,
        index: u8,
        token: AccountId,
        to: AccountId,
        amount: U128,
        from_chain_id: String,
    ) -> PromiseOrValue<U128>;
    fn transfer_callback(&mut self, amount: U128) -> PromiseOrValue<U128>;
    fn any_swap_out_callback(
        &mut self,
        token: AccountId,
        from: AccountId,
        to: AccountId,
        amount: U128,
        to_chain_id: U128,
    ) -> PromiseOrValue<U128>;
}

#[ext_contract(ext_fungible_token)]
pub trait FungibleTokenContract {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
}

#[ext_contract(ext_any_token)]
pub trait AnyTokenTrait {
    fn burn(&mut self, account_id: AccountId, amount: U128) -> PromiseOrValue<U128>;
    fn mint(&mut self, account_id: AccountId, amount: U128) -> PromiseOrValue<U128>;
}

#[near_bindgen]
impl FungibleTokenReceiver for Router {
    fn ft_on_transfer(
        &mut self,
        sender_id: ValidAccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        let sender = sender_id.to_string();
        // let decode_msg: Vec<&str> = msg.as_str().split(" ").collect();
        let token = env::predecessor_account_id();
        let self_account = env::current_account_id();
        let message = serde_json::from_str::<Actions>(&msg).expect("Router: msg parse not match");
        match message {
            Actions::AnySwapOut {
                bind_id,
                to_chain_id,
            } => {
                assert!(!self.pause_out && !self.pause_all, "Router: swapOut pause");
                match self.underlying_to_anytoken.get(&token) {
                    Some(any_token) => ext_any_token::burn(
                        self_account.to_string(),
                        amount,
                        &any_token,
                        NOT_DEPOSIT,
                        self.base_gas,
                    )
                    .then(ext_self::any_swap_out_callback(
                        any_token,
                        sender,
                        bind_id,
                        amount,
                        to_chain_id,
                        &self_account,
                        NOT_DEPOSIT,
                        self.base_gas,
                    ))
                    .into(),
                    _ => {
                        self.log_swap_out(token, sender, bind_id, amount, to_chain_id);
                        PromiseOrValue::Value(U128::from(0))
                    }
                }
            }
            Actions::DepositUnderlying { receiver_id } => {
                match self.underlying_to_anytoken.get(&token) {
                    Some(any_token) => {
                        ext_any_token::mint(receiver_id, amount, &any_token, NOT_DEPOSIT, BASE_GAS)
                            .into()
                    }
                    _ => PromiseOrValue::Value(amount),
                }
            }
            Actions::Withdraw { to } => match token == self.any_near() {
                true => {
                    Promise::new(to).transfer(amount.0);
                    PromiseOrValue::Value(U128(0))
                }
                _ => match self.anytoken_to_underlying.get(&token) {
                    Some(underlying) => ext_fungible_token::ft_transfer(
                        to,
                        amount,
                        None,
                        &underlying,
                        ONE_YOCTO_DEPOSIT,
                        BASE_GAS,
                    )
                    .then(ext_self::transfer_callback(
                        amount,
                        &env::current_account_id(),
                        NOT_DEPOSIT,
                        self.base_gas,
                    ))
                    .into(),
                    _ => PromiseOrValue::Value(amount),
                },
            },
        }
    }
}
