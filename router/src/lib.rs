use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::collections::{UnorderedSet,UnorderedMap};
use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_sdk::{
    env, ext_contract, log, near_bindgen, AccountId, Balance, Gas, PanicOnDefault,
    PromiseOrValue,BorshStorageKey,Promise,PromiseResult
};

near_sdk::setup_alloc!();

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Router {
    mpc_id: AccountId,
    chain_id:String,
    txs:UnorderedSet<String>,
    wnative:AccountId,
    gas_for_anytoken:UnorderedMap<AccountId,Gas>,
    base_gas:Gas
}
const NOT_DEPOSIT: Balance = 0;
const ONE_YOCTO_DEPOSIT: Balance = 1;
const BASE_GAS: Gas = 5_000_000_000_000;
const GAS_FOR_FT_TRANSFER_CALL: Gas = 30_000_000_000_000;

#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
    Txs,
    Gas
}

#[near_bindgen]
impl Router {
    #[init]
    pub fn new(mpc_id: ValidAccountId,wnative:ValidAccountId,chain_id:String) -> Self {
        assert!(!env::state_exists(), "Already initialized");
        Self { mpc_id: mpc_id.to_string(),wnative:wnative.to_string(),chain_id:chain_id,
            txs:UnorderedSet::new(StorageKey::Txs),gas_for_anytoken:UnorderedMap::new(StorageKey::Gas),base_gas:BASE_GAS }
    }

    pub fn mpc_id(&self)->AccountId{
        self.mpc_id.clone()
    }

    pub fn change_mpc_id(&mut self,new_mpc_id:ValidAccountId){
        assert_eq!(env::predecessor_account_id(),self.mpc_id,"Router: only mpc");
        self.mpc_id=new_mpc_id.to_string();
    }

    pub fn wnative(&self)->AccountId{
        self.wnative.clone()
    }

    pub fn change_wnative(&mut self,new_wnative:ValidAccountId){
        assert_eq!(env::predecessor_account_id(),self.mpc_id,"Router: only mpc");
        self.wnative=new_wnative.to_string();
    }

    pub fn base_gas(&self)->Gas{
        self.base_gas
    }

    pub fn set_base_gas(&mut self,gas:Gas){
        assert_eq!(env::predecessor_account_id(),self.mpc_id,"Router: only mpc");
        self.base_gas=gas;
    }

    pub fn check_gas(&self,token:AccountId)->Gas{
        match self.gas_for_anytoken.get(&token){
            Some(value)=>value,
            _=>self.base_gas*3
        }
    }

    pub fn all_gas(&self)->Vec<(AccountId,Gas)>{
        self.gas_for_anytoken.to_vec()
    }

    pub fn set_gas(&mut self,token:ValidAccountId,gas:Gas){
        assert_eq!(env::predecessor_account_id(),self.mpc_id,"Router: only mpc");
        self.gas_for_anytoken.insert(&token.to_string(),&gas);
    }

    pub fn any_swap_out_gas(&self)->Gas{
        self.base_gas*9+GAS_FOR_FT_TRANSFER_CALL
    }

    pub fn any_swap_in_gas(&self,token:ValidAccountId)->Gas{
        self.check_gas(token.to_string())+self.base_gas*4
    }

    pub fn chain_id(&self)->String{
        self.chain_id.clone()
    }

    pub fn change_chain_id(&mut self,new_chain_id:ValidAccountId){
        assert_eq!(env::predecessor_account_id(),self.mpc_id,"Router: only mpc");
        self.chain_id=new_chain_id.to_string();
    }

    pub fn check_tx(&self,txhash:String)->bool{
        self.txs.contains(&txhash)
    }

    pub fn all_txs(&self)->Vec<String>{
        self.txs.to_vec()
    }

    pub fn any_swap_in(
        &mut self,
        tx: String,
        token: AccountId,
        to: AccountId,
        amount: U128,
        from_chain_id:String,
    ) ->Promise{
        assert_eq!(env::predecessor_account_id(),self.mpc_id,"Router: only mpc");
        assert!(!self.txs.contains(&tx),"Router: tx exists");
        ext_any_token::swap_in(
            to.clone(),
            amount,
            &token,
            NOT_DEPOSIT,
            self.check_gas(token.clone())
        ).then(
            ext_self::any_swap_in_callback(
                tx,token,to,amount,from_chain_id,
                &env::current_account_id(),
                NOT_DEPOSIT,
                self.base_gas,
            )
        ).into()
    }

    pub fn swap_in_native(&mut self,
        tx: String,
        to: AccountId,
        amount: U128,
        from_chain_id:String
    ) ->Promise{
        assert_eq!(env::predecessor_account_id(),self.mpc_id,"Router: only mpc");
        assert!(!self.txs.contains(&tx),"Router: tx exists");
        ext_any_token::swap_in_native(
            to.clone(),
            amount,
            &self.wnative,
            NOT_DEPOSIT,
            self.check_gas(self.wnative.clone())
        ).then(
            ext_self::swap_in_native_callback(
                tx,to,amount,from_chain_id,
                &env::current_account_id(),
                NOT_DEPOSIT,
                self.base_gas,
            )
        ).into()
    }

    #[private]
    pub fn any_swap_out(&mut self,token:AccountId,any_token:AccountId,from:AccountId,to:AccountId,amount:U128,to_chain_id:String)->PromiseOrValue<U128>{
        assert_eq!(env::promise_results_count(), 1, "ERR_TOO_MANY_RESULTS");
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(value) => {
                if let Ok(underlying) = near_sdk::serde_json::from_slice::<AccountId>(&value) {
                    assert_eq!(underlying,token,"Router: underlying in any_token not equals to token");
                    assert!(amount.0>0,"The amount should be a positive number");
                    (match token!=any_token{
                        true=>{
                            ext_fungible_token::ft_transfer(
                                any_token.clone(),
                                amount,
                                None,
                                &token,
                                ONE_YOCTO_DEPOSIT,
                                self.base_gas,
                            )
                        },
                        _=>{
                            ext_any_token::burn(
                                env::current_account_id(),
                                amount,
                                &token,
                                NOT_DEPOSIT,
                                self.base_gas
                            )
                        }
                    }).then(
                        ext_self::any_swap_out_callback(
                            any_token,
                            from,
                            to,
                            amount,
                            to_chain_id,
                            &env::current_account_id(),
                            NOT_DEPOSIT,
                            self.base_gas
                        )
                    ).into()
                } else {
                    env::panic(b"ERR_CALL_FAILED")
                }
            },
            PromiseResult::Failed => env::panic(b"ERR_CALL_FAILED"),
        }

    }

    #[payable]
    pub fn swap_out_native(&mut self,to:String,to_chain_id:String)->Promise{
        let amount=env::attached_deposit();
        assert!(amount>0,"The amount should be a positive number");
        Promise::new(self.wnative.clone()).transfer(amount).then(
            ext_self::swap_out_native_callback(
                env::predecessor_account_id(),to,amount,to_chain_id,
                &env::current_account_id(),
                NOT_DEPOSIT,
                self.base_gas,
            )
        ).into()
    }

    #[private]
    pub fn any_swap_out_callback(&mut self,token:AccountId,from:AccountId,to:AccountId,amount:U128,to_chain_id:String)->U128{
        assert_eq!(env::promise_results_count(), 1, "ERR_TOO_MANY_RESULTS");
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(..) => {
                log!("LogSwapOut token {} from {} to {} amount {} fromChainId {} toChainId {}",token,from,to,amount.0,self.chain_id,to_chain_id);
                U128::from(0)
            },
            PromiseResult::Failed => {
                amount
            }
        }
    }

    #[private]
    pub fn swap_out_native_callback(&mut self,from:AccountId,to:String,amount:Balance,to_chain_id:String) {
        assert_eq!(env::promise_results_count(), 1, "ERR_TOO_MANY_RESULTS");
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(..) => {
                log!("LogSwapOutNative token {} from {} to {} amount {} fromChainId {} toChainId {}",self.wnative,from,to,amount,self.chain_id,to_chain_id);
            },
            PromiseResult::Failed => {
                log!("Refund native {} from {} to {}", amount, env::current_account_id(), from);
                Promise::new(from).transfer(amount);
            },
        }
    }

    #[private]
    pub fn any_swap_in_callback(
        &mut self,        
        tx: String,
        token: AccountId,
        to: AccountId,
        amount: U128,
        from_chain_id:String
    ) {
        assert_eq!(env::promise_results_count(), 1, "ERR_TOO_MANY_RESULTS");
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(..) => {
                log!("LogSwapIn txs {} token {} to {} amount {} fromChainId {} toChainId {}", tx,token,to,amount.0,from_chain_id,self.chain_id);
                self.txs.insert(&tx);
            },
            PromiseResult::Failed => env::panic(b"ERR_CALL_FAILED"),
        }
    }

    #[private]
    pub fn swap_in_native_callback(
        &mut self,        
        tx: String,
        to: AccountId,
        amount: U128,
        from_chain_id:String
    ) {
        assert_eq!(env::promise_results_count(), 1, "ERR_TOO_MANY_RESULTS");
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(..) => {
                log!("LogSwapInNative txs {} to {} amount {} fromChainId {} toChainId {}", tx,to,amount.0,from_chain_id,self.chain_id);
                self.txs.insert(&tx);
            },
            PromiseResult::Failed => env::panic(b"ERR_CALL_FAILED"),
        }
    }
}

#[ext_contract(ext_self)]
pub trait ExtSelf {
    fn any_swap_out(&mut self,token:AccountId,any_token:AccountId,from:AccountId,to:AccountId,amount:U128,to_chain_id:String)->PromiseOrValue<U128>;
    fn any_swap_out_callback(&mut self,token:AccountId,from:AccountId,to:AccountId,amount:U128,to_chain_id:String)->U128;
    fn swap_out_native_callback(&mut self,from:AccountId,to:String,amount:Balance,to_chain_id:String);
    fn any_swap_in_callback(        
        &mut self,        
        tx: String,
        token: AccountId,
        to: AccountId,
        amount: U128,
        from_chain_id:String
    );
    fn swap_in_native_callback(        
        &mut self,        
        tx: String,
        to: AccountId,
        amount: U128,
        from_chain_id:String
    );
}

#[ext_contract(ext_fungible_token)]
pub trait FungibleTokenContract {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
}

#[ext_contract(ext_any_token)]
pub trait AnyTokenTrait {
    fn burn(&mut self,account_id:AccountId,amount:U128);
    fn underlying(&self)->AccountId;
    fn swap_out(&self);
    fn swap_in(&mut self, receiver_id: AccountId, amount: U128);
    fn swap_in_native(&mut self, receiver_id: AccountId, amount: U128);
}

#[near_bindgen]
impl FungibleTokenReceiver for Router {
    /// If given `msg: "take-my-money", immediately returns U128::From(0)
    /// Otherwise, makes a cross-contract call to own `value_please` function, passing `msg`
    /// value_please will attempt to parse `msg` as an integer and return a U128 version of it
    fn ft_on_transfer(
        &mut self,
        sender_id: ValidAccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        let sender = sender_id.to_string();
        let decode_msg:Vec<&str>=msg.as_str().split(" ").collect();
        match decode_msg[0]{
            "any_swap_out"=>{                     
                // any_swap_out anytoken bindaddr chain_id
                assert!(decode_msg.len()==4,"decode swap_out msg error!"); 
                ext_any_token::underlying(
                    &decode_msg[1].to_string(),
                    NOT_DEPOSIT,
                    self.base_gas
                ).then(
                    ext_self::any_swap_out(
                        env::predecessor_account_id(),
                        decode_msg[1].to_string(),
                        sender,
                        decode_msg[2].to_string(),
                        amount,
                        decode_msg[3].to_string(),
                        &env::current_account_id(),
                        NOT_DEPOSIT,
                        self.base_gas*5
                    )
                ).into()
            },
            _=>{
                log!("Router: msg parse not match");
                PromiseOrValue::Value(amount)
            }
        }
    }
}
