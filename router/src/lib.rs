use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::{
    env, ext_contract, log, near_bindgen, AccountId, Balance, BorshStorageKey, Gas, PanicOnDefault,
    Promise, PromiseOrValue, PromiseResult,
};

near_sdk::setup_alloc!();

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Router {
    pending_mpc_id: Option<AccountId>,
    mpc_id: AccountId,
    chain_id: String,
    txs: UnorderedMap<(String, u8), bool>,
    wnative: AccountId,
    gas_for_anytoken: UnorderedMap<AccountId, Gas>,
    base_gas: Gas,
    pause_in: bool,
    pause_out: bool,
    pause_all: bool,
}
const NOT_DEPOSIT: Balance = 0;
const ONE_YOCTO_DEPOSIT: Balance = 1;
const BASE_GAS: Gas = 5_000_000_000_000;
const GAS_FOR_FT_TRANSFER_CALL: Gas = 30_000_000_000_000;

#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
    Txs,
    Gas,
}

#[near_bindgen]
impl Router {
    #[init]
    pub fn new(mpc_id: AccountId, wnative: AccountId, chain_id: String) -> Self {
        assert!(!env::state_exists(), "Already initialized");
        Self {
            pending_mpc_id: None,
            mpc_id,
            wnative,
            chain_id,
            txs: UnorderedMap::new(StorageKey::Txs),
            gas_for_anytoken: UnorderedMap::new(StorageKey::Gas),
            base_gas: BASE_GAS,
            pause_in: false,
            pause_out: false,
            pause_all: false,
        }
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
        assert_eq!(
            env::predecessor_account_id(),
            self.mpc_id,
            "Router: only mpc"
        );
        self.pending_mpc_id = Some(new_mpc_id);
    }

    pub fn apply_mpc_id(&mut self) {
        assert!(
            env::predecessor_account_id() == self.mpc_id,
            "Router: FORBIDDEN"
        );
        assert!(
            self.pending_mpc_id() != String::from(""),
            "Router: must call change_mpc_id before this"
        );
        self.pending_mpc_id = None;
        self.mpc_id = self.pending_mpc_id();
    }

    pub fn set_pause_in(&mut self, pause_in: bool) {
        assert_eq!(
            env::predecessor_account_id(),
            self.mpc_id,
            "Router: only mpc"
        );
        self.pause_in = pause_in;
    }

    pub fn set_pause_out(&mut self, pause_out: bool) {
        assert_eq!(
            env::predecessor_account_id(),
            self.mpc_id,
            "Router: only mpc"
        );
        self.pause_out = pause_out;
    }

    pub fn set_pause_all(&mut self, pause_all: bool) {
        assert_eq!(
            env::predecessor_account_id(),
            self.mpc_id,
            "Router: only mpc"
        );
        self.pause_all = pause_all;
    }

    pub fn wnative(&self) -> AccountId {
        self.wnative.to_string()
    }

    pub fn change_wnative(&mut self, new_wnative: AccountId) {
        assert_eq!(
            env::predecessor_account_id(),
            self.mpc_id,
            "Router: only mpc"
        );
        self.wnative = new_wnative;
    }

    pub fn base_gas(&self) -> Gas {
        self.base_gas
    }

    pub fn set_base_gas(&mut self, gas: Gas) {
        assert_eq!(
            env::predecessor_account_id(),
            self.mpc_id,
            "Router: only mpc"
        );
        self.base_gas = gas;
    }

    pub fn check_gas(&self, token: AccountId) -> Gas {
        match self.gas_for_anytoken.get(&token) {
            Some(value) => value,
            _ => self.base_gas * 7,
        }
    }

    pub fn all_gas(&self) -> Vec<(AccountId, Gas)> {
        self.gas_for_anytoken.to_vec()
    }

    pub fn set_gas(&mut self, token: AccountId, gas: Gas) {
        assert_eq!(
            env::predecessor_account_id(),
            self.mpc_id,
            "Router: only mpc"
        );
        self.gas_for_anytoken.insert(&token, &gas);
    }

    pub fn any_swap_out_gas(&self) -> Gas {
        self.base_gas * 9 + GAS_FOR_FT_TRANSFER_CALL
    }

    pub fn any_swap_in_gas(&self, token: AccountId) -> Gas {
        self.check_gas(token) + self.base_gas * 4
    }

    pub fn chain_id(&self) -> String {
        self.chain_id.clone()
    }

    pub fn change_chain_id(&mut self, new_chain_id: String) {
        assert_eq!(
            env::predecessor_account_id(),
            self.mpc_id,
            "Router: only mpc"
        );
        self.chain_id = new_chain_id;
    }

    pub fn check_tx(&self, txhash: String, index: u8) -> bool {
        self.txs.get(&(txhash, index)) == Some(true)
    }

    pub fn all_txs(&self) -> Vec<((String, u8), bool)> {
        self.txs.to_vec()
    }

    fn valid_tx(&mut self, tx: String, index: u8, amount: u128) {
        assert!(!self.pause_in && !self.pause_all, "Router: pause");
        assert_eq!(
            env::predecessor_account_id(),
            self.mpc_id,
            "Router: only mpc"
        );
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
        ext_any_token::swap_in(
            to.clone(),
            amount,
            &token,
            NOT_DEPOSIT,
            self.check_gas(token.clone()),
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
            self.base_gas,
        ))
        .into()
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
        ext_any_token::swap_in_native(
            to.clone(),
            amount,
            &self.wnative,
            NOT_DEPOSIT,
            self.check_gas(self.wnative.clone()),
        )
        .then(ext_self::any_swap_in_callback(
            tx.clone(),
            index,
            String::from(""),
            to.clone(),
            amount,
            from_chain_id,
            &env::current_account_id(),
            NOT_DEPOSIT,
            self.base_gas,
        ))
        .into()
    }

    pub fn any_swap_in_all(
        &mut self,
        tx: String,
        token: AccountId,
        index: u8,
        to: AccountId,
        amount: U128,
        from_chain_id: String,
    ) -> PromiseOrValue<U128> {
        self.valid_tx(tx.clone(), index, amount.0);
        match token == self.wnative {
            true => ext_any_token::swap_in_native(
                to.clone(),
                amount,
                &self.wnative,
                NOT_DEPOSIT,
                self.check_gas(self.wnative.clone()),
            )
            .then(ext_self::any_swap_in_callback(
                tx.clone(),
                index,
                String::from(""),
                to.clone(),
                amount,
                from_chain_id,
                &env::current_account_id(),
                NOT_DEPOSIT,
                self.base_gas,
            ))
            .into(),
            _ => ext_any_token::swap_in(
                to.clone(),
                amount,
                &token,
                NOT_DEPOSIT,
                self.check_gas(token.clone()),
            )
            .then(ext_self::any_swap_in_callback(
                tx,
                index,
                token,
                to.clone(),
                amount,
                from_chain_id,
                &env::current_account_id(),
                NOT_DEPOSIT,
                self.base_gas,
            ))
            .into(),
        }
    }

    #[private]
    pub fn any_swap_out(
        &mut self,
        token: AccountId,
        any_token: AccountId,
        from: AccountId,
        to: AccountId,
        amount: U128,
        to_chain_id: String,
    ) -> PromiseOrValue<U128> {
        assert!(!self.pause_out && !self.pause_all, "Router: pause");
        assert_eq!(env::promise_results_count(), 1, "ERR_TOO_MANY_RESULTS");
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(value) => {
                if let Ok(underlying) = near_sdk::serde_json::from_slice::<AccountId>(&value) {
                    if underlying == "" {
                        assert_eq!(
                            token, any_token,
                            "Router: anytoken must equals to underlying when underlying is none"
                        );
                    } else {
                        assert_eq!(
                            underlying, token,
                            "Router: underlying in any_token not equals to token"
                        );
                    }
                    (match token != any_token {
                        true => ext_fungible_token::ft_transfer(
                            any_token.clone(),
                            amount,
                            None,
                            &token,
                            ONE_YOCTO_DEPOSIT,
                            self.base_gas,
                        ),
                        _ => ext_any_token::burn(
                            env::current_account_id(),
                            amount,
                            &token,
                            NOT_DEPOSIT,
                            self.base_gas,
                        ),
                    })
                    .then(ext_self::any_swap_out_callback(
                        any_token,
                        from,
                        to,
                        U128::from(amount),
                        to_chain_id,
                        &env::current_account_id(),
                        NOT_DEPOSIT,
                        self.base_gas,
                    ))
                    .into()
                } else {
                    env::panic(b"ERR_UNDERLYING_NOT_MATCH")
                }
            }
            PromiseResult::Failed => env::panic(b"ERR_QUERY_UNDERLYING_FAILED"),
        }
    }

    #[payable]
    pub fn swap_out_native(&mut self, to: String, to_chain_id: String) -> PromiseOrValue<U128> {
        assert!(!self.pause_out && !self.pause_all, "Router: pause");
        let amount = env::attached_deposit();
        assert!(amount > 0, "The amount should be a positive number");
        Promise::new(self.wnative.clone())
            .transfer(amount)
            .then(ext_self::any_swap_out_callback(
                String::from(""),
                env::predecessor_account_id(),
                to,
                U128::from(amount),
                to_chain_id,
                &env::current_account_id(),
                NOT_DEPOSIT,
                self.base_gas,
            ))
            .into()
    }

    #[private]
    pub fn any_swap_out_callback(
        &mut self,
        token: AccountId,
        from: AccountId,
        to: AccountId,
        amount: U128,
        to_chain_id: String,
    ) -> U128 {
        assert_eq!(env::promise_results_count(), 1, "ERR_TOO_MANY_RESULTS");
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(..) => match token == String::from("") {
                true => {
                    log!(
                        "LogSwapOutNative token {} from {} to {} amount {} fromChainId {} toChainId {}",
                        self.wnative,
                        from,
                        to,
                        amount.0,
                        self.chain_id,
                        to_chain_id
                    );
                    U128::from(amount)
                }
                false => {
                    log!(
                        "LogSwapOut token {} from {} to {} amount {} fromChainId {} toChainId {}",
                        token,
                        from,
                        to,
                        amount.0,
                        self.chain_id,
                        to_chain_id
                    );
                    U128::from(0)
                }
            },
            PromiseResult::Failed => match token == String::from("") {
                true => {
                    Promise::new(from).transfer(amount.0);
                    U128::from(0)
                }
                false => amount,
            },
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
    ) -> U128 {
        assert_eq!(env::promise_results_count(), 1, "ERR_TOO_MANY_RESULTS");
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(..) => {
                match token == String::from("") {
                    true => {
                        log!(
                            "LogSwapInNative txs {} to {} amount {} fromChainId {} toChainId {}",
                            tx,
                            to,
                            amount.0,
                            from_chain_id,
                            self.chain_id
                        );
                    }
                    false => {
                        log!(
                            "LogSwapIn txs {} token {} to {} amount {} fromChainId {} toChainId {}",
                            tx,
                            token,
                            to,
                            amount.0,
                            from_chain_id,
                            self.chain_id
                        );
                    }
                }
                U128::from(amount)
            }
            PromiseResult::Failed => {
                self.txs.remove(&(tx, index));
                U128::from(0)
            }
        }
    }
}

#[ext_contract(ext_self)]
pub trait ExtSelf {
    fn any_swap_out(
        &mut self,
        token: AccountId,
        any_token: AccountId,
        from: AccountId,
        to: AccountId,
        amount: U128,
        to_chain_id: String,
    ) -> PromiseOrValue<U128>;
    fn any_swap_out_callback(
        &mut self,
        token: AccountId,
        from: AccountId,
        to: AccountId,
        amount: U128,
        to_chain_id: String,
    ) -> U128;
    fn any_swap_in_callback(
        &mut self,
        tx: String,
        index: u8,
        token: AccountId,
        to: AccountId,
        amount: U128,
        from_chain_id: String,
    ) -> U128;
}

#[ext_contract(ext_fungible_token)]
pub trait FungibleTokenContract {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
}

#[ext_contract(ext_any_token)]
pub trait AnyTokenTrait {
    fn burn(&mut self, account_id: AccountId, amount: U128);
    fn underlying(&self) -> AccountId;
    fn swap_out(&self);
    fn swap_in(&mut self, receiver_id: AccountId, amount: U128);
    fn swap_in_native(&mut self, receiver_id: AccountId, amount: U128);
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
        let decode_msg: Vec<&str> = msg.as_str().split(" ").collect();
        match decode_msg[0] {
            "any_swap_out" => {
                // any_swap_out anytoken bindaddr chain_id
                assert!(decode_msg.len() == 4, "decode swap_out msg error!");
                ext_any_token::underlying(&decode_msg[1].to_string(), NOT_DEPOSIT, self.base_gas)
                    .then(ext_self::any_swap_out(
                        env::predecessor_account_id(),
                        decode_msg[1].to_string(),
                        sender,
                        decode_msg[2].to_string(),
                        amount,
                        decode_msg[3].to_string(),
                        &env::current_account_id(),
                        NOT_DEPOSIT,
                        self.base_gas * 5,
                    ))
                    .into()
            }
            _ => {
                log!("Router: msg parse not match");
                PromiseOrValue::Value(amount)
            }
        }
    }
}
