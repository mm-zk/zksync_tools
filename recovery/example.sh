#!/bin/bash
set -e

L1_RPC_URL="http://localhost:8545"
DATABASE_URL="postgres://postgres:notsecurepassword@localhost:5432/zksync_sandbox"

# ==========================================
# Start from snapshot database
# ==========================================
echo ">> Resetting snapshot database..."
PGPASSWORD=notsecurepassword dropdb -h 127.0.0.1 -U postgres --if-exists zksync_sandbox
PGPASSWORD=notsecurepassword createdb -h 127.0.0.1 -U postgres zksync_sandbox
psql $DATABASE_URL < ./data/test/db.snapshot.sql > /dev/null
echo "Done"

# ==========================================
# Generate SQL queries
# ==========================================
echo ""
echo ">> Generating SQL queries..."

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

# ==========================================
# Apply SQL queries
# ==========================================
echo ""
echo ">> Applying SQL queries..."
psql $DATABASE_URL -f data/test/sql/4.sql > /dev/null
psql $DATABASE_URL -f data/test/sql/5.sql > /dev/null
psql $DATABASE_URL -f data/test/sql/6.sql > /dev/null
echo "Done"

# ==========================================
# Comparing tree values
# ==========================================
echo ""
python3 build_tree.py --db_url $DATABASE_URL --output data/test/tree.recovered.json > /dev/null
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

# ==========================================
# Debug: Compare databases
# ==========================================
# python3 db_tool.py compare

# ==========================================
# Ultimate Test: Start the server with recovered db
# ==========================================
# zkstack server ...
