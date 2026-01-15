import os
import json
from collections import defaultdict
import psycopg2
import psycopg2.extras
from datetime import datetime, date
from psycopg2.extras import RealDictCursor
import argparse
import sys
from typing import List, Dict, Any

# ==========================================
# Configuration
# ==========================================
OUTPUT_DIR = f"test/explorer/batches"
DB_CONFIG = {
    "host": "127.0.0.1",
    "port": "5432",
    "user": "postgres",
    "password": "notsecurepassword",
    "dbname": "zksync_explorer"
}

# ==========================================
# Helpers
# ==========================================
class PostgresEncoder(json.JSONEncoder):
    def default(self, obj):
        # Handle Datetime -> ISO String
        if isinstance(obj, (datetime, date)):
            return obj.isoformat()
        if isinstance(obj, (bytes, bytearray, memoryview)):
            return "0x" + bytes(obj).hex()
        return super().default(obj)

#                                                        Table "public.batches"
#      Column     |            Type             | Collation | Nullable | Default | Storage  | Compression | Stats target | Description 
# ----------------+-----------------------------+-----------+----------+---------+----------+-------------+--------------+-------------
#  createdAt      | timestamp without time zone |           | not null | now()   | plain    |             |              | 
#  updatedAt      | timestamp without time zone |           | not null | now()   | plain    |             |              | 
#  number         | bigint                      |           | not null |         | plain    |             |              | 
#  rootHash       | bytea                       |           |          |         | extended |             |              | 
#  l1GasPrice     | character varying(128)      |           | not null |         | extended |             |              | 
#  l2FairGasPrice | character varying(128)      |           | not null |         | extended |             |              | 
#  commitTxHash   | bytea                       |           |          |         | extended |             |              | 
#  committedAt    | timestamp without time zone |           |          |         | plain    |             |              | 
#  proveTxHash    | bytea                       |           |          |         | extended |             |              | 
#  provenAt       | timestamp without time zone |           |          |         | plain    |             |              | 
#  executeTxHash  | bytea                       |           |          |         | extended |             |              | 
#  executedAt     | timestamp without time zone |           |          |         | plain    |             |              | 
#  l1TxCount      | integer                     |           | not null |         | plain    |             |              | 
#  l2TxCount      | integer                     |           | not null |         | plain    |             |              | 
#  timestamp      | timestamp without time zone |           | not null |         | plain    |             |              | 
def get_batches(cur, batch_from: int):
    query = 'SELECT * FROM batches WHERE "number" >= %s'
    cur.execute(query, (batch_from,))
    return cur.fetchall()
#                                                        Table "public.blocks"
#     Column     |            Type             | Collation | Nullable | Default | Storage  | Compression | Stats target | Description 
# ---------------+-----------------------------+-----------+----------+---------+----------+-------------+--------------+-------------
#  createdAt     | timestamp without time zone |           | not null | now()   | plain    |             |              | 
#  updatedAt     | timestamp without time zone |           | not null | now()   | plain    |             |              | 
#  number        | bigint                      |           | not null |         | plain    |             |              | 
#  nonce         | character varying           |           | not null |         | extended |             |              | 
#  difficulty    | integer                     |           | not null |         | plain    |             |              | 
#  gasLimit      | character varying(128)      |           | not null |         | extended |             |              | 
#  gasUsed       | character varying(128)      |           | not null |         | extended |             |              | 
#  baseFeePerGas | character varying(128)      |           | not null |         | extended |             |              | 
#  l1BatchNumber | bigint                      |           | not null |         | plain    |             |              | 
#  l1TxCount     | integer                     |           | not null |         | plain    |             |              | 
#  l2TxCount     | integer                     |           | not null |         | plain    |             |              | 
#  hash          | bytea                       |           | not null |         | extended |             |              | 
#  parentHash    | bytea                       |           |          |         | extended |             |              | 
#  miner         | bytea                       |           | not null |         | extended |             |              | 
#  extraData     | bytea                       |           | not null |         | extended |             |              | 
#  timestamp     | timestamp without time zone |           | not null |         | plain    |             |              | 
def get_blocks(cur, batch_number: int):
    query = 'SELECT * FROM blocks WHERE "l1BatchNumber" = %s'
    cur.execute(query, (batch_number,))
    return cur.fetchall()

#                                                                           Table "public.transactions"
#         Column        |            Type             | Collation | Nullable |                   Default                    | Storage  | Compression | Stats target | Description 
# ----------------------+-----------------------------+-----------+----------+----------------------------------------------+----------+-------------+--------------+-------------
#  createdAt            | timestamp without time zone |           | not null | now()                                        | plain    |             |              | 
#  updatedAt            | timestamp without time zone |           | not null | now()                                        | plain    |             |              | 
#  nonce                | bigint                      |           | not null |                                              | plain    |             |              | 
#  transactionIndex     | integer                     |           | not null |                                              | plain    |             |              | 
#  gasLimit             | character varying(128)      |           | not null |                                              | extended |             |              | 
#  gasPrice             | character varying(128)      |           | not null |                                              | extended |             |              | 
#  maxFeePerGas         | character varying(128)      |           |          |                                              | extended |             |              | 
#  maxPriorityFeePerGas | character varying(128)      |           |          |                                              | extended |             |              | 
#  value                | character varying(128)      |           | not null |                                              | extended |             |              | 
#  chainId              | integer                     |           | not null |                                              | plain    |             |              | 
#  blockNumber          | bigint                      |           | not null |                                              | plain    |             |              | 
#  type                 | integer                     |           | not null |                                              | plain    |             |              | 
#  accessList           | jsonb                       |           |          |                                              | extended |             |              | 
#  l1BatchNumber        | bigint                      |           | not null |                                              | plain    |             |              | 
#  fee                  | character varying           |           | not null |                                              | extended |             |              | 
#  isL1Originated       | boolean                     |           | not null |                                              | plain    |             |              | 
#  receivedAt           | timestamp without time zone |           | not null |                                              | plain    |             |              | 
#  number               | bigint                      |           | not null | nextval('transactions_number_seq'::regclass) | plain    |             |              | 
#  hash                 | bytea                       |           | not null |                                              | extended |             |              | 
#  to                   | bytea                       |           |          |                                              | extended |             |              | 
#  from                 | bytea                       |           | not null |                                              | extended |             |              | 
#  data                 | bytea                       |           | not null |                                              | extended |             |              | 
#  blockHash            | bytea                       |           | not null |                                              | extended |             |              | 
#  receiptStatus        | integer                     |           | not null | 1                                            | plain    |             |              | 
#  gasPerPubdata        | character varying           |           |          |                                              | extended |             |              | 
#  error                | character varying           |           |          |                                              | extended |             |              | 
#  revertReason         | character varying           |           |          |                                              | extended |             |              | 
#
#                                                                           Table "public.transactionReceipts"
#       Column       |            Type             | Collation | Nullable |                        Default                        | Storage  | Compression | Stats target | Description 
# -------------------+-----------------------------+-----------+----------+-------------------------------------------------------+----------+-------------+--------------+-------------
#  createdAt         | timestamp without time zone |           | not null | now()                                                 | plain    |             |              | 
#  updatedAt         | timestamp without time zone |           | not null | now()                                                 | plain    |             |              | 
#  transactionIndex  | integer                     |           | not null |                                                       | plain    |             |              | 
#  type              | integer                     |           | not null |                                                       | plain    |             |              | 
#  gasUsed           | character varying(128)      |           | not null |                                                       | extended |             |              | 
#  effectiveGasPrice | character varying(128)      |           | not null |                                                       | extended |             |              | 
#  blockNumber       | bigint                      |           | not null |                                                       | plain    |             |              | 
#  cumulativeGasUsed | character varying(128)      |           | not null |                                                       | extended |             |              | 
#  byzantium         | boolean                     |           | not null | true                                                  | plain    |             |              | 
#  status            | integer                     |           | not null |                                                       | plain    |             |              | 
#  transactionHash   | bytea                       |           | not null |                                                       | extended |             |              | 
#  to                | bytea                       |           |          |                                                       | extended |             |              | 
#  from              | bytea                       |           | not null |                                                       | extended |             |              | 
#  contractAddress   | bytea                       |           |          |                                                       | extended |             |              | 
#  root              | bytea                       |           |          |                                                       | extended |             |              | 
#  logsBloom         | bytea                       |           | not null |                                                       | extended |             |              | 
#  blockHash         | bytea                       |           | not null |                                                       | extended |             |              | 
#  number            | bigint                      |           | not null | nextval('"transactionReceipts_number_seq"'::regclass) | plain    |             |              | 
#
def get_transactions(cur, batch_number: int):
    query = """
        SELECT
          t.*,
          tr.type,
          tr."gasUsed",
          tr."effectiveGasPrice",
          tr."cumulativeGasUsed",
          tr."contractAddress",
          tr."root",
          tr."logsBloom",
          tr."blockHash"
        FROM transactions t
        LEFT JOIN "transactionReceipts" tr
          ON t.hash = tr."transactionHash"
        WHERE t."l1BatchNumber" = %s
    """
    cur.execute(query, (batch_number,))
    return cur.fetchall()

#                                                                         Table "public.logs"
#       Column      |            Type             | Collation | Nullable |               Default                | Storage  | Compression | Stats target | Description 
# ------------------+-----------------------------+-----------+----------+--------------------------------------+----------+-------------+--------------+-------------
#  createdAt        | timestamp without time zone |           | not null | now()                                | plain    |             |              | 
#  updatedAt        | timestamp without time zone |           | not null | now()                                | plain    |             |              | 
#  number           | bigint                      |           | not null | nextval('logs_number_seq'::regclass) | plain    |             |              | 
#  blockNumber      | bigint                      |           | not null |                                      | plain    |             |              | 
#  transactionIndex | integer                     |           | not null |                                      | plain    |             |              | 
#  removed          | boolean                     |           |          |                                      | plain    |             |              | 
#  logIndex         | integer                     |           | not null |                                      | plain    |             |              | 
#  transactionHash  | bytea                       |           |          |                                      | extended |             |              | 
#  address          | bytea                       |           | not null |                                      | extended |             |              | 
#  data             | bytea                       |           | not null |                                      | extended |             |              | 
#  topics           | bytea[]                     |           | not null |                                      | extended |             |              | 
#  timestamp        | timestamp without time zone |           | not null |                                      | plain    |             |              | 
def get_logs(cur, block_from: int, block_to: int):
    query = """
        SELECT
          logs."createdAt",
          logs."updatedAt",
          logs."number",
          logs."blockNumber",
          logs."transactionIndex",
          logs."removed",
          logs."logIndex",
          txs."hash" as "transactionHash",
          logs."address",
          logs."data",
          logs."topics",
          logs."timestamp"
        FROM logs
        JOIN transactions txs
          ON logs."transactionIndex" = txs."transactionIndex"
          AND logs."blockNumber" = txs."blockNumber"
        WHERE logs."blockNumber" >= %s AND logs."blockNumber" <= %s
        ORDER BY logs."blockNumber" ASC, logs."transactionIndex" ASC, logs."logIndex" ASC
    """
    cur.execute(query, (block_from, block_to))
    return cur.fetchall()

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--batch_from", help="Batch from which to start fetching data", default=0)
    args = parser.parse_args()

    # Connect to DB
    try:
        conn = psycopg2.connect(**DB_CONFIG)
        cur = conn.cursor(cursor_factory=RealDictCursor)
    except Exception as e:
        print(f"Database connection error: {e}")
        sys.exit(1)

    try:
        os.makedirs(OUTPUT_DIR, exist_ok=True)
        
        # Save batches
        batch_rows = get_batches(cur, args.batch_from)
        for batch_row in batch_rows:
            batch_number = batch_row['number']
            batch_dir = f"{OUTPUT_DIR}/{batch_number}"
            os.makedirs(batch_dir, exist_ok=True)
            # Save batch details
            with open(f"{batch_dir}/details.json", "w") as f:
                json.dump(batch_row, f, cls=PostgresEncoder, indent=2)
        
            # Save blocks
            block_dir = f"{batch_dir}/blocks"
            os.makedirs(block_dir, exist_ok=True)
            block_rows = get_blocks(cur, batch_number)
            for block_row in block_rows:
                block_number = block_row['number']
                with open(f"{block_dir}/{block_number}.json", "w") as f:
                    json.dump(block_row, f, cls=PostgresEncoder, indent=2)

            # Save txs
            tx_dir = f"{batch_dir}/txs"
            os.makedirs(tx_dir, exist_ok=True)
            tx_rows = get_transactions(cur, batch_number)
            for tx_row in tx_rows:
                tx_hash = "0x" + tx_row['hash'].hex()
                with open(f"{tx_dir}/{tx_hash}.json", "w") as f:
                    json.dump(tx_row, f, cls=PostgresEncoder, indent=2)

            # Group logs by transaction
            if len(block_rows) == 0:
                continue
            block_from = min(block_row['number'] for block_row in block_rows)
            block_to = max(block_row['number'] for block_row in block_rows)
            log_rows = get_logs(cur, block_from, block_to)
            logs_by_tx = defaultdict(list)
            for log_row in log_rows:
                tx_hash = "0x" + log_row['transactionHash'].hex()
                logs_by_tx[tx_hash].append(log_row)
            
            # Save logs
            logs_dir = f"{batch_dir}/logs"
            os.makedirs(logs_dir, exist_ok=True)
            for tx_hash, logs in logs_by_tx.items():
                with open(f"{logs_dir}/{tx_hash}.json", "w") as f:
                    json.dump(logs, f, cls=PostgresEncoder, indent=2)
    except Exception as e:
        print(f"Error during execution: {e}")
    finally:
        cur.close()
        conn.close()

if __name__ == "__main__":
    main()
