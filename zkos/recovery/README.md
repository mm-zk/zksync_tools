# Recovery tool

Small tool, to recover state just from L1.



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
cargo run -- --rpc http://localhost:8545 --address 0x8FdB49aBc1E2B891D91f64B15aE6A3616c8d8d1e
```



## TODO


- [DONE] state diff unpacking from L1
- creating state, and verifying hash 
- applying the state to rocksDB
- executing next batch
