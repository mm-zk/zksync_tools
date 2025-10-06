# Recovery tool

Tool to recover state from just L1, and then to create a new DB for the sequencer to start from.

Recovery tool offers two commands:
* `recover` to scan L1, and output a json file with all the necessary state information
* `write-to-db` that creates a new database, from which you can start a sequencer.





## Recover

```shell
# Read state from 8545, and save to file.
# Address has to be diamond proxy address of the given chain.
cargo run -- recover --rpc http://localhost:8545 --address 0x8FdB49aBc1E2B891D91f64B15aE6A3616c8d8d1e --output some_file.json
```

## Write to db

```shell
 cargo run write-to-db --input some_file.json --db-path DB_PATH
```

Then you can start the sequencer with this database.

Currently you also have to pass following arguments:
```shell
general_min_blocks_to_replay=0 general_force_starting_block_number=START_BLOCK general_state_backend=Compacted cargo run
```

The value for START_BLOCK will be printed from the write-to-db command.



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

* L1 deposit transactions not testnet
