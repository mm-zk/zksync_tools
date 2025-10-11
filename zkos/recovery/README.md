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

**`--concurrency`** (default: 10)
- Number of concurrent operations (used for both chunk scanning and transaction fetching)
- Higher values = faster scanning but more load on the RPC node
- Recommended values:
  - Public RPC nodes: 5-10
  - Self-hosted nodes: 20-50
  - Powerful self-hosted nodes: 50+

**`--chunk`** (default: 2000)
- Number of blocks per RPC request
- Default works well with most Ethereum clients
- For Reth with default settings, you can safely use up to 10,000
- Don't exceed your RPC node's `eth_getLogs` block range limit

**Example with performance tuning:**
```shell
cargo run -- recover \
  --rpc http://localhost:8545 \
  --bridgehub <BRIDGEHUB_ADDRESS> \
  --chain-id <CHAIN_ID> \
  --concurrency 50 \
  --chunk 10000 \
  --output recovery.json
```

### Write to DB

```shell
cargo run -- write-to-db \
  --input recovery.json \
  --db-path <DB_PATH>
```

**`--batch-size`** (default: 100,000)
- Number of entries per RocksDB WriteBatch operation
- WriteBatch groups multiple writes into atomic operations for performance improvement
- Higher values = faster writes but more memory usage

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
