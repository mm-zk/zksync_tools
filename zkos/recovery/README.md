# Recovery tool

Small tool, to recover state just from L1.

```shell
# Read state from 8545, and save to file.
# Address has to be diamond proxy address of the given chain.
cargo run -- recover --rpc http://localhost:8545 --address 0x8FdB49aBc1E2B891D91f64B15aE6A3616c8d8d1e --output some_file.json
```



## More info
Running against local (addresses can change):


Getting bridgehub:

```shell
curl --request POST \                                                                        
  --url localhost:3050 \
  --header 'Content-Type: application/json' \
  --data '{
      "jsonrpc": "2.0",
      "id": 1,
      "method": "zks_getBridgehubContract",
      "params": []
    }'
```

getting executor:

```shell
cast call 0xec68e2cfe53b183125bcaf2888ae5a94bbcc7a4e 'getZKChain(uint256)(address)' 270
```



Actually running the tool:

```shell
cargo run -- recover --rpc http://localhost:8545 --address 0x8FdB49aBc1E2B891D91f64B15aE6A3616c8d8d1e
```

Checking testnet (specifying to & from blocks):

```shell
cargo run -- recover --rpc SEPOLIA_RPC --address 0x02b1ac1cf0a592aefd3c2246b2431388365db272 --from 9210280 --to 9318536
```


## TODO


- [DONE] state diff unpacking from L1
- [DONE] creating state, and verifying hash
- [DONE] Need to clean up bytecode hashes / AccountProperties computation and verify. 
- applying the state to rocksDB
- executing next batch
