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
use near_sdk::collections::{LazyOption,UnorderedSet};
use near_sdk::json_types::{ValidAccountId, U128};
use near_sdk::{
    env, log, near_bindgen,ext_contract, AccountId, Balance,Gas, BorshStorageKey, PanicOnDefault, PromiseOrValue,Promise
};

near_sdk::setup_alloc!();

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct AnyToken {
    token: FungibleToken,
    metadata: LazyOption<FungibleTokenMetadata>,
    minters:UnorderedSet<AccountId>,
    txs:UnorderedSet<String>,
    mpc_id:AccountId,
    router_id:AccountId,
    underlying:Option<AccountId>
}

const DATA_IMAGE_SVG_NEAR_ICON: &str = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 288 288'%3E%3Cg id='l' data-name='l'%3E%3Cpath d='M187.58,79.81l-30.1,44.69a3.2,3.2,0,0,0,4.75,4.2L191.86,103a1.2,1.2,0,0,1,2,.91v80.46a1.2,1.2,0,0,1-2.12.77L102.18,77.93A15.35,15.35,0,0,0,90.47,72.5H87.34A15.34,15.34,0,0,0,72,87.84V201.16A15.34,15.34,0,0,0,87.34,216.5h0a15.35,15.35,0,0,0,13.08-7.31l30.1-44.69a3.2,3.2,0,0,0-4.75-4.2L96.14,186a1.2,1.2,0,0,1-2-.91V104.61a1.2,1.2,0,0,1,2.12-.77l89.55,107.23a15.35,15.35,0,0,0,11.71,5.43h3.13A15.34,15.34,0,0,0,216,201.16V87.84A15.34,15.34,0,0,0,200.66,72.5h0A15.35,15.35,0,0,0,187.58,79.81Z'/%3E%3C/g%3E%3C/svg%3E";
const ONE_YOCTO_DEPOSIT: Balance = 1;
const BASE_GAS: Gas = 5_000_000_000_000;
const PROMISE_CALL: Gas = 5_000_000_000_000;
const GAS_FOR_FT_TRANSFER: Gas = BASE_GAS + PROMISE_CALL;

#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
    FungibleToken,
    Metadata,
    Minter,
    TxHash
}

#[near_bindgen]
impl AnyToken {
    /// Initializes the contract with the given total supply owned by the given `owner_id` with
    /// default metadata (for example purposes only).
    #[init]
    pub fn new_default_meta(mpc_id:ValidAccountId,router_id: ValidAccountId,underlying:Option<AccountId>,total_supply: U128) -> Self {
        Self::new(
            mpc_id,
            router_id,
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
            underlying
        )
    }

    /// Initializes the contract with the given total supply owned by the given `owner_id` with
    /// the given fungible token metadata.
    #[init]
    pub fn new(
        mpc_id: ValidAccountId,
        router_id: ValidAccountId,
        total_supply: U128,
        metadata: FungibleTokenMetadata,
        underlying:Option<AccountId>
    ) -> Self {
        assert!(!env::state_exists(), "AnyToken: Already initialized");
        metadata.assert_valid();
        let mut this = Self {
            token: FungibleToken::new(StorageKey::FungibleToken),
            metadata: LazyOption::new(StorageKey::Metadata, Some(&metadata)),
            minters:UnorderedSet::new(StorageKey::Minter),
            txs:UnorderedSet::new(StorageKey::TxHash),
            mpc_id:mpc_id.to_string(),
            router_id:router_id.to_string(),
            underlying:underlying
        };
        this.internal_insert_minter(router_id.as_ref());
        this.token.internal_register_account(router_id.as_ref());
        this.token.internal_deposit(router_id.as_ref(), total_supply.into());
        this
    }

    fn on_account_closed(&mut self, account_id: AccountId, balance: Balance) {
        log!("Closed @{} with {}", account_id, balance);
    }

    fn on_tokens_burned(&mut self, account_id: AccountId, amount: Balance) {
        log!("Account @{} burned {}", account_id, amount);
    }

    pub fn get_router_id(&self)->AccountId{
        self.router_id.clone()
    }

    pub fn change_router_id(&mut self,account_id:AccountId){
        assert!(env::predecessor_account_id()==self.mpc_id,"AnyToken: FORBIDDEN");
        self.router_id=account_id;
    }

    pub fn underlying(&self)->AccountId{
        match &self.underlying{
            Some(underlying)=>underlying.to_string(),
            _=>String::from("")
        }
    }

    pub fn set_underlying(&mut self,account_id:AccountId){
        assert!(env::predecessor_account_id()==self.mpc_id,"AnyToken: FORBIDDEN");
        assert!(self.underlying()==String::from(""),"AnyToken: underlying exists");
        self.underlying=Some(account_id);
    }

    pub fn all_minters(&self)->Vec<AccountId>{
        self.minters.to_vec()
    }

    pub fn is_minter(&self,account_id:AccountId)->bool{
        self.minters.contains(&account_id)
    }

    pub fn set_minter(&mut self,account_id:AccountId,flag:bool)->AccountId{
        assert!(env::predecessor_account_id()==self.mpc_id,"FORBIDDEN");
        match flag{
            true=>self.internal_insert_minter(&account_id),
            _=>self.internal_delete_minter(&account_id),
        };
        account_id
    }

    fn internal_insert_minter(&mut self,account_id:&AccountId){
        assert!(!self.minters.contains(account_id),"account_id exists minters list");
        self.minters.insert(account_id);
    }
    
    fn internal_delete_minter(&mut self,account_id:&AccountId){
        assert!(self.minters.contains(account_id),"account_id not exists minters list");
        self.minters.remove(account_id);
    }

    pub fn mint(&mut self,account_id:AccountId,amount:U128){
        assert!(self.minters.contains(&env::predecessor_account_id())||env::predecessor_account_id()==env::current_account_id(),"FORBIDDEN");
        assert!(amount.0 > 0, "The amount should be a positive number");
        self.token.internal_deposit(&account_id,amount.0);
        log!("Transfer {} from {} to {}", amount.0, env::current_account_id(), account_id);
    }

    pub fn burn(&mut self,account_id:AccountId,amount:U128){
        assert!(self.minters.contains(&env::predecessor_account_id())||env::predecessor_account_id()==env::current_account_id(),"FORBIDDEN");
        assert!(amount.0 > 0, "The amount should be a positive number");
        self.token.internal_withdraw(&account_id,amount.0);
        log!("Transfer {} from {} to {}", amount.0,account_id, env::current_account_id());
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
        // Verifying that we were called by fungible token contract that we expect.
        assert_eq!(
            &env::predecessor_account_id(),
            &self.underlying(),
            "Only supports the one fungible token contract"
        );
        log!("Transfer {} from {} to {} msg = {}", amount.0, sender_id.as_ref(),env::current_account_id(),msg);
        PromiseOrValue::Value(U128::from(0))
    }
}

pub trait AnyTokenTrait {
    fn swap_in(&mut self, receiver_id: AccountId, amount: U128)->Promise;
    fn swap_in_native(&mut self, receiver_id: AccountId, amount: U128)->Promise;
}

#[near_bindgen]
impl AnyTokenTrait for AnyToken {

    fn swap_in_native(&mut self, receiver_id: AccountId, amount: U128)->Promise{
        assert_eq!(
            &env::predecessor_account_id(),
            &self.router_id,
            "Only supports the router contract"
        );
        Promise::new(receiver_id).transfer(amount.0)
    }
    
    fn swap_in(&mut self,receiver_id:AccountId,amount:U128)->Promise{
        assert_eq!(
            &env::predecessor_account_id(),
            &self.router_id,
            "Only supports the router contract"
        );
        match &self.underlying{
            Some(underlying)=>{
                ext_fungible_token::ft_transfer(
                    receiver_id.clone(),
                    amount,
                    None,
                    &underlying,
                    ONE_YOCTO_DEPOSIT,
                    env::prepaid_gas() - GAS_FOR_FT_TRANSFER,
                )
            },
            _=>{
                self.mint(receiver_id,amount);
                Promise::new("success".to_string())
            }
        }
    }
}

#[ext_contract(ext_fungible_token)]
pub trait FungibleTokenContract {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
}



