# near-contract
near-contract

# near env
```text
1) 安装rust环境  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   添加工具链 rustup target add wasm32-unknown-unknown
2) 安装near-cli  npm install -g near-cli
3) 创建near账户 https://wallet.testnet.near.org/

```

# contract env
```text
1)  测试 cargo test -- --nocapture
2)  编译 env 'RUSTFLAGS=-C link-arg=-s' cargo build --target wasm32-unknown-unknown --release
```

# deploy
```text
1) 登录 near login （~/.near-credentials文件夹下生成密钥对文件）
   或者 near generate-key example.testnet --seedPhrase="xxx"
2) 创建合约账户 near create-account CONTRACT_NAME.ACCOUNT_ID --masterAccount ACCOUNT_ID --initialBalance 10
3) 合约部署 near deploy --wasmFile *.wasm --accountId CONTRACT_ID
```

# call
```text
1) 合约调用 near call CONTRACT_ID func_name '{"key": "value"}' --accountId ACCOUNT_ID
2) 合约读取 near view CONTRACT_ID read '{"key": "value"}' --accountId ACCOUNT_ID
```

# question
```text
1) linker `cc` not found
   运行 sudo apt install build-essential
2) near command not found
   运行 node config ls 查询全局路径
   配置export PATH="全局路径:$PATH"
```

# underlying
```text
1) near无须部署
2）调用ft_transfer方法，memo方式跨出
```

# anytoken
```text
1) near部署anytoken合约
near deploy --wasmFile xxx.wasm --accountId contractId
near call contractId new_default_meta '{"mpc_id": "","total_supply": "0",check_tx_hash: true,"name": "","symbol": "","decimals": 24} --accountId xxx
2）调用swap_out方法跨出  receiver_id: AccountId, amount: U128, to_chain_id: U128
near call contractId swap_out '{"receiver_id": "","amount": "0","to_chain_id": "4"} --accountId xxx
```

# native
```text
1) mpc部署mpcPool合约
near deploy --wasmFile xxx.wasm --accountId contractId
near call contractId new '{} --accountId xxx
2）调用swap_out方法跨出 receiver_id: AccountId, to_chain_id: U128
near call contractId swap_out '{"receiver_id":"","to_chain_id":"4"} --accountId xxx --depositYocto xxxxxxx
```

# docs
```text
1) near-cli:  https://docs.near.org/docs/tools/near-cli
2) near-sdk-rs:  https://www.near-sdk.io/
```
