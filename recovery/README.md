# Adhoc EraVM L1 recovery tools
**Not intended as a "generic" tooling.**

Written for:
* Protocol: 0.28.1
* Server: v29.6.0
* DA: Rollup, Blobs

# Test Plan
Notes: omit full `tree.json` creation steps from below, if `storage_logs` is too big.

## Preparation
* Deploy ecosystem, start the server, start the block explorer
* Commit, prove and execute the first 3 batches
* Save the database to `data/test/db.snapshot.sql`
* Commit, prove and execute another 3 batches
* Save the database to `data/test/db.latest.sql`
* Build latest "tree": `python3 build_tree.py --db_url $DATABASE_URL --output data/test/tree.latest.json`
* Create:
  * `zksync_snapshot` - database with the snapshot data
  * `zksync_latest` - database with the latest data
  * `zksync_sandbox` - working database

## Recovery
Now the goal is: with the database rolled back to `db.snapshot.sql` state, using only data from block explorer and L1, make sequencer operational again.
In other words, with the database having data for batch 0-3, we need to recover data for batches 4-6 to make server runnable.

[1] Recover data from block explorer using `data/test/explorer/fetcher.py`

[2] Assuming, Rollup mode and Blobs are used for commits, download blob data to `data/test/blobs/N.txt`

[3] Using `blob_decoder` decode pubdata inputs to `data/test/pubdata_inputs/N.txt`

[4] Initialize `zksync_sandbox` with snapshot data:
```bash
psql $SANDBOX_DB_URL < ./data/test/db.snapshot.sql > /dev/null
```

[5] Build `tree.init.json`<br/>
```bash
# Find all repeated indices from batches we need to recover
ls data/test/pubdata_inputs/*.txt | xargs -I {} python3 pubdata_parser.py --json --input {} | jq -s '[ .[].state_diff.repeated_writes ] | add | map(.index) | unique' > data/test/indices.json
# Build tree
python3 build_tree.py --db_url $SANDBOX_DB_URL --indices=data/test/indices.json --output data/test/tree.init.json
```

[6] Run `query_generator.py` for each batch you need to recover.
Each run will create:
* Updated tree file: `--tree_file_output data/test/tree_after_N.json`
* SQL query to backfill data: `--tree_file_output data/test/sql/N.sql`
```bash
python3 query_generator.py \
  --rpc $L1_RPC_URL \
  --commit_tx_hash 0xbc7dc7c7ff26ae02831aba70da18d66c47a54a33608a19dd42f56c886cf83de0 \
  --prove_tx_hash 0xc775c74f12ec188deca16900cf3e27ca78a5bad585dcba2b18691aecde19268a \
  --execute_tx_hash 0x99652c15d32ad20ba69c254d8904dc72f6bdfd060ef777e6230fd7e19b60d8e5 \
  --tree_file data/test/tree.init.json \
  --pubdata_input_file data/test/pubdata_inputs/4.txt \
  --explorer_data_dir data/test/explorer/batches \
  --explorer_data_format postgres \
  --tree_file_output data/test/tree_after_4.json \
  --sql_output data/test/sql/4.sql

python3 query_generator.py \
  --rpc $L1_RPC_URL \
  --commit_tx_hash 0x0d2ac129f6f6d2778a24d238fed2d3d9365bc976b315f6a2e87b280547bf4cda \
  --prove_tx_hash 0xd1287df15e48b0490e4a712ad736ab101ebce199c2514b120ca9058acc53c0be \
  --execute_tx_hash 0x76475a207a8cc94f9e0ba4b06b22272f00c520d0805d6241cc027b2d547176ec \
  --tree_file data/test/tree_after_4.json \
  --pubdata_input_file data/test/pubdata_inputs/5.txt \
  --explorer_data_dir data/test/explorer/batches \
  --explorer_data_format postgres \
  --tree_file_output data/test/tree_after_5.json \
  --sql_output data/test/sql/5.sql

python3 query_generator.py \
  --rpc $L1_RPC_URL \
  --commit_tx_hash 0xa37e981fa04b106c9e75b1a3772f1e8e818031824d6cb7e99d883e3ab64d7ae2 \
  --prove_tx_hash 0xf1ca36d7e03d7a2dede017fd430e59685120195115bdbfd8829233f5b12dc1ea \
  --execute_tx_hash 0xa3e44eb66f9556b3c27925a4a90ad00068e6acb48d52948f05b09dbe8a17b256 \
  --tree_file data/test/tree_after_5.json \
  --pubdata_input_file data/test/pubdata_inputs/6.txt \
  --explorer_data_dir data/test/explorer/batches \
  --explorer_data_format postgres \
  --tree_file_output data/test/tree_after_6.json \
  --sql_output data/test/sql/6.sql
```

[7] Apply SQL queries for each batch to `zksync_sandbox`:
```bash
psql $SANDBOX_DB_URL -f data/test/sql/4.sql > /dev/null
psql $SANDBOX_DB_URL -f data/test/sql/5.sql > /dev/null
psql $SANDBOX_DB_URL -f data/test/sql/6.sql > /dev/null
```
[8] (Test) Recreate `tree.recovered.json` from "patched" sandbox database and compare with `tree.latest.json`.<br/>
There shouldn't be any difference in the number of elements, "value", "hashed_key", however "address" and "key" entries for new writes will be `null` in `tree.recovered.json`.
```bash
# Build recovered tree
python3 build_tree.py --db_url $SANDBOX_DB_URL --output data/test/tree.recovered.json > /dev/null
# Compare recovered and latest tree
TREE_DIFF=$(diff -U0 data/test/tree.recovered.json data/test/tree.latest.json \
  | grep -vE '^(---|\+\+\+|@@)' \
  | grep -vE '"(address|key)":' || true)
if [ -z "$TREE_DIFF" ]; then
  echo "✅ All tree values match"
else
  diff -u data/test/tree.recovered.json data/test/tree.latest.json
  echo "❌ Tree values mismatch"
  exit 1
fi
```

[9] (Test) Compare the content of `zksync_sandbox` and `zksync_latest`.<br/>
Use `db_tool.py` to simplify comparison, e.g.:
```bash
python3 db_tool.py compare
# python3 db_tool.py compare l1_batches
# python3 db_tool.py compare l1_batches --col local_root

# (Dev) You can also "copy" columns, tables from "latest" to "sandbox"
# python3 db_tool.py copy l1_batches --col local_root
```

[10] Ultimate test: restart the server
* Remove all local cache directories used by server (`db/`)
* Use `zksync_sandbox` database in `secrets.yaml`
* Use Anvil L1 fork as L1 RPC URL in `secrets.yaml` (so that new batches won't be sent on L1 during testing)
* Restart the server

See `example.sh` for reference.

## Notes
Fee operator top off events are NOT recovered (one event per the last miniblock in the batch (with 0 txs)). To recover these: you can check `storage_logs` for the latest miniblock in the latest batch, derive its `index` and then refer to `state diffs` from pubdata inputs - most often it'll be endoded with operation "1" i.e. the value would represent the exact value the balance was increased by.<br/>
You can then add these manually and update `logs_blooms` for the corresponding miniblocks.<br/>
Example:
```bash
python3 pubdata_parser.py --input data/zero_test/pubdata_inputs/1768.txt --json | jq '.state_diff.repeated_writes' | grep 'index": 73' -A 3
#    "index": 73,
#    "operation": 1,
#    "value": 7215969600000000
#  },
```
