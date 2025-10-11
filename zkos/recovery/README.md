# Recovery tool

Tool to recover state from just L1, and then to create a new DB for the sequencer to start from.

## Commands

### `recover`
Scans L1 and outputs a JSON file with all necessary state information.

### `write-to-db`
Creates a new database from the JSON file for starting a sequencer.

## Usage

### Basic Recovery

```shell
# Method 1: Auto-discover diamond proxy from Bridgehub
cargo run -- recover \
  --rpc http://localhost:8545 \
  --bridgehub <BRIDGEHUB_ADDRESS> \
  --chain-id <CHAIN_ID> \
  --output recovery.json

# Method 2: Direct diamond proxy address
cargo run -- recover \
  --rpc http://localhost:8545 \
  --address <DIAMOND_PROXY_ADDRESS> \
  --output recovery.json
```

### Performance Tuning

**`--tx-batch-size`** (default: 1)
- Number of transactions to fetch concurrently

**`--concurrency`** (default: 1)
- Number of block chunks to scan in parallel

**Example with performance tuning:**
```shell
cargo run -- recover \
  --rpc http://localhost:8545 \
  --bridgehub <BRIDGEHUB_ADDRESS> \
  --chain-id <CHAIN_ID> \
  --tx-batch-size 20 \
  --concurrency 5 \
  --output recovery.json
```

### Write to DB

```shell
cargo run -- write-to-db \
  --input recovery.json \
  --db-path <DB_PATH>
```

The command will output the starting block number for the sequencer:

```shell
general_min_blocks_to_replay=0 \
general_force_starting_block_number=<START_BLOCK> \
general_state_backend=Compacted \
cargo run
```

## Finding Required Parameters

### Get Bridgehub Address (from L2 RPC when available)

```shell
curl -X POST <L2_RPC_URL> \
  -H 'Content-Type: application/json' \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "zks_getBridgehubContract",
    "params": []
  }'
```

### Get Chain ID (from L2 RPC when available)

```shell
curl -X POST <L2_RPC_URL> \
  -H 'Content-Type: application/json' \
  -d '{
    "jsonrpc": "2.0",
    "method": "eth_chainId",
    "params": [],
    "id": 1
  }'
```

### Get Diamond Proxy (from L1 if you have bridgehub + chain ID)

The tool does this automatically with `--bridgehub` and `--chain-id` flags.
It calls `getZKChain(uint256)` on the bridgehub contract.

## Examples

### Local Development
```shell
cargo run -- recover \
  --rpc http://localhost:8545 \
  --address <DIAMOND_PROXY_ADDRESS>
```

### With specific block range
```shell
cargo run -- recover \
  --rpc http://localhost:8545 \
  --address <DIAMOND_PROXY_ADDRESS> \
  --from <START_BLOCK> \
  --to <END_BLOCK>
```

## TODO

* L1 deposit transactions not testnet
