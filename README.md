# near-contract

near-contract

# near env

```text
1) env install:  
	curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    	rustup target add wasm32-unknown-unknown
2) near-cli install:
	npm install -g near-cli
3) create near account:
	https://wallet.testnet.near.org/

```

# contract env

```text
1)  test:
	cargo test -- --nocapture
2)  build:
	env 'RUSTFLAGS=-C link-arg=-s' cargo build --target wasm32-unknown-unknown --release
```

# deploy

```text
1) login:
	near login （pair storage at ~/.near-credentials）
   or:
	near generate-key example.testnet --seedPhrase="xxx"
2) create account:
	near create-account CONTRACT_NAME.ACCOUNT_ID --masterAccount ACCOUNT_ID --initialBalance 10
3) contract deploy:
	near deploy --wasmFile *.wasm --accountId CONTRACT_ID
```

# call

```text
1) call contract function:
	near call CONTRACT_ID func_name '{"key": "value"}' --accountId ACCOUNT_ID
2) view contract function:
	near view CONTRACT_ID read '{"key": "value"}' --accountId ACCOUNT_ID
```

# question

```text
1) linker `cc` not found
   	sudo apt install build-essential
2) near command not found
   	node config ls // query global path
   	export PATH="[global path]:$PATH"
```

# underlying

```text
1) don't need to deploy contract
2）call ft_transfer，memo=>bindaddr:chainId
```

# anytoken

```text
1) deploy anytoken contract
	near deploy --wasmFile xxx.wasm --accountId contractId
	near call contractId new_default_meta '{"mpc_id": "","total_supply": "0",check_tx_hash: true,"name": "","symbol": "","decimals": 24} --accountId xxx
2）call swap_out function
	receiver_id: AccountId, amount: U128, to_chain_id: U128
	near call contractId swap_out '{"receiver_id": "","amount": "0","to_chain_id": "4"} --accountId xxx
```

# native

```text
1) mpc deploy mpcPool contract
	near deploy --wasmFile xxx.wasm --accountId contractId
	near call contractId new '{} --accountId xxx
2）call swap_out
	receiver_id: AccountId, to_chain_id: U128
	near call contractId swap_out '{"receiver_id":"","to_chain_id":"4"} --accountId xxx --depositYocto xxxxxxx
```

# docs

```text
1) near-cli:  https://docs.near.org/docs/tools/near-cli
2) near-sdk-rs:  https://www.near-sdk.io/
```
