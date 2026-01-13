import json
import os
from abc import ABC, abstractmethod
from stub_data import StubData
from datetime import datetime, timezone

class ExplorerData:
    @staticmethod
    def get_batch_details(batch_number: int, data_dir: str, data_format: str) -> BatchDetails:
        raw_data = RawBatchData(data_dir, batch_number)
        if data_format == "postgres":
            return PostgresBatchDetails(batch_number, raw_data)
        elif data_format == "blockscout":
            return BlockscoutBatchDetails(batch_number, raw_data)
        else:
            raise ValueError(f"Invalid data format: {data_format}")

class BatchDetails(ABC):
    def __init__(self, batch_number: int, raw: RawBatchData):
        self.number = batch_number
        self.raw = raw
        self.raw_batch = raw.batch
    def batch_number(self) -> int:
        return self.number
    @abstractmethod
    def l1_gas_price(self) -> int:
        pass
    @abstractmethod
    def l2_fair_gas_price(self) -> int:
        pass
    @abstractmethod
    def l2_tx_count(self) -> int:
        pass
    # Returns ordered blocks
    @abstractmethod
    def get_blocks(self) -> list:
        pass
    # Returns ordered transactions
    @abstractmethod
    def get_txs(self, block_number: int = None) -> list:
        pass
    # Returns ordered logs
    @abstractmethod
    def get_logs(self, block_number: int = None, tx_hash: str = None) -> list:
        pass

class BlockDetails(ABC):
    def __init__(self, block_number: int, raw: RawBatchData):
        self.number = block_number
        self.raw = raw
        self.raw_block = raw.blocks[block_number]
    def block_number(self) -> int:
        return self.number
    @abstractmethod
    def hash(self) -> str:
        pass
    @abstractmethod
    def timestamp(self) -> int:
        pass
    @abstractmethod
    def l1_tx_count(self) -> int:
        pass
    @abstractmethod
    def l2_tx_count(self) -> int:
        pass
    @abstractmethod
    def base_fee_per_gas(self) -> int:
        pass

class TxDetails(ABC):
    def __init__(self, tx_hash: str, raw: RawBatchData):
        self.tx_hash = tx_hash
        self.raw = raw
        self.raw_tx = raw.txs[tx_hash]
    def hash(self) -> str:
        return self.tx_hash
    @abstractmethod
    def block_number(self) -> int:
        pass
    @abstractmethod
    def is_l1_tx(self) -> bool:
        pass
    @abstractmethod
    def initiator_address(self) -> str:
        pass
    @abstractmethod
    def to(self) -> str:
        pass
    @abstractmethod
    def nonce(self) -> int:
        pass
    @abstractmethod
    def value(self) -> int:
        pass
    @abstractmethod
    def calldata(self) -> str:
        pass
    @abstractmethod
    def factory_deps(self) -> list:
        pass
    @abstractmethod
    def timestamp(self) -> int:
        pass
    @abstractmethod
    def index_in_block(self) -> int:
        pass
    @abstractmethod
    def is_error(self) -> bool:
        pass
    @abstractmethod
    def error(self) -> str | None:
        pass
    @abstractmethod
    def gas_limit(self) -> int:
        pass
    @abstractmethod
    def gas_used(self) -> int:
        pass
    @abstractmethod
    def tx_format(self) -> int:
        pass
    @abstractmethod
    def max_fee_per_gas(self) -> int:
        pass
    @abstractmethod
    def max_priority_fee_per_gas(self) -> int:
        pass
    @abstractmethod
    def effective_gas_price(self) -> int | None:
        pass

class LogDetails(ABC):
    def __init__(self, tx: TxDetails, log_index: int, raw: RawBatchData):
        self.tx = tx
        self.log_index_in_tx = log_index
        self.raw = raw
    def tx_details(self) -> TxDetails:
        return self.tx
    def log_index(self) -> int:
        return self.log_index_in_tx
    @abstractmethod
    def address(self) -> str: pass
    @abstractmethod
    def topics(self) -> list: pass
    @abstractmethod
    def data(self) -> str: pass
    @abstractmethod
    def timestamp(self) -> int: pass
        
class RawBatchData:
    def __init__(self, data_dir: str, batch_number: int):
        batch_dir = os.path.join(data_dir, str(batch_number))
        blocks_dir = os.path.join(batch_dir, "blocks")
        blocks_files = os.listdir(blocks_dir)
        txs_dir = os.path.join(batch_dir, "txs")
        txs_files = os.listdir(txs_dir)
        logs_dir = os.path.join(batch_dir, "logs")
        logs_files = os.listdir(logs_dir)
        self.batch = json.load(open(os.path.join(batch_dir, "details.json")))
        self.blocks = {int(f.split(".")[0]): json.load(open(os.path.join(blocks_dir, f))) for f in blocks_files}
        self.txs = {f.split(".")[0]: json.load(open(os.path.join(txs_dir, f))) for f in txs_files}
        self.logs = {f.split(".")[0]: json.load(open(os.path.join(logs_dir, f))) for f in logs_files}

# =========================================
# Implementations for data fetched from Postgres Explorer DB (by Matter Labs)
# =========================================

class PostgresBatchDetails(BatchDetails):
    def __init__(self, batch_number: int, raw: RawBatchData):
        super().__init__(batch_number, raw)
        
    def l1_gas_price(self) -> int:
        return int(self.raw_batch["l1GasPrice"])

    def l2_fair_gas_price(self) -> int:
        return int(self.raw_batch["l2FairGasPrice"])

    def l2_tx_count(self) -> int:
        return int(self.raw_batch["l2TxCount"])

    def get_blocks(self) -> list:
        blocks = sorted(self.raw.blocks.values(), key=lambda x: int(x["number"]))
        return [PostgresBlockDetails(int(block["number"]), self.raw) for block in blocks]
    
    def get_txs(self, block_number: int = None) -> list:
        txs = self.raw.txs.values()
        if block_number is not None:
            txs = [tx for tx in txs if int(tx["blockNumber"]) == block_number]
        txs = sorted(txs, key=lambda x: (int(x["blockNumber"]), int(x["transactionIndex"])))
        return [PostgresTxDetails(tx["hash"], self.raw) for tx in txs]

    def get_logs(self, block_number: int = None, tx_hash: str = None) -> list:
        txs = self.get_txs(block_number)
        logs = []
        for tx in txs:
            if tx_hash is not None and tx.hash() != tx_hash:
                continue
            for i, log in enumerate(self.raw.logs[tx.hash()]):
                logs.append(PostgresLogDetails(tx, int(i), self.raw))
        return logs


class PostgresBlockDetails(BlockDetails):
    def hash(self) -> str:
        return self.raw_block["hash"]

    def timestamp(self) -> int:
        dt = datetime.fromisoformat(self.raw_block["timestamp"])
        dt = dt.replace(tzinfo=timezone.utc)
        return int(dt.timestamp())
    
    def l1_tx_count(self) -> int:
        return int(self.raw_block["l1TxCount"])

    def l2_tx_count(self) -> int:
        return int(self.raw_block["l2TxCount"])

    def base_fee_per_gas(self) -> int:
        return int(self.raw_block["baseFeePerGas"])


class PostgresTxDetails(TxDetails):
    def block_number(self) -> int:
        return int(self.raw_tx["blockNumber"])

    def is_l1_tx(self) -> bool:
        return self.raw_tx["isL1Originated"]

    def initiator_address(self) -> str:
        return self.raw_tx["from"]

    def to(self) -> str:
        return self.raw_tx["to"]

    def nonce(self) -> int:
        return int(self.raw_tx["nonce"])

    def value(self) -> int:
        return int(self.raw_tx["value"])

    def calldata(self) -> str:
        return self.raw_tx["data"]

    def factory_deps(self) -> list:
        return []
    
    def timestamp(self) -> str:
        dt = datetime.fromisoformat(self.raw_tx["receivedAt"])
        dt = dt.replace(tzinfo=timezone.utc)
        return int(dt.timestamp())

    def index_in_block(self) -> int:
        return int(self.raw_tx["transactionIndex"])

    def error(self) -> str | None:
        return self.raw_tx["error"]

    def is_error(self) -> bool:
        return self.raw_tx["receiptStatus"] != 1

    def gas_limit(self) -> int:
        return int(self.raw_tx["gasLimit"])

    def gas_used(self) -> int:
        return int(self.raw_tx["gasLimit"])

    def gas_per_pubdata_limit(self) -> int:
        return int(self.raw_tx["gasPerPubdata"], 16)

    def tx_format(self) -> int:
        return int(self.raw_tx["type"])

    def max_fee_per_gas(self) -> int:
        return int(self.raw_tx["maxFeePerGas"])

    def max_priority_fee_per_gas(self) -> int:
        return int(self.raw_tx["maxPriorityFeePerGas"])

    def effective_gas_price(self) -> int:
        return int(self.raw_tx["effectiveGasPrice"])

class PostgresLogDetails(LogDetails):
    def __init__(self, tx: TxDetails, log_index: int, raw: RawBatchData):
        super().__init__(tx, log_index, raw)
        self.raw_log = raw.logs[tx.hash()][log_index]

    def address(self) -> str:
        return self.raw_log["address"].lower()

    def topics(self) -> list:
        return self.raw_log["topics"]

    def data(self) -> str:
        return self.raw_log["data"]

    def timestamp(self) -> int:
        return self.tx.timestamp()

# =========================================
# Implementations for data fetched from Blockscout API
# =========================================

class BlockscoutBatchDetails(BatchDetails):
    def l1_gas_price(self) -> int:
        return int(self.raw_batch["l1_gas_price"])

    def l2_fair_gas_price(self) -> int:
        return int(self.raw_batch["l2_fair_gas_price"])

    def l2_tx_count(self) -> int:
        return int(self.raw_batch["l2_tx_count"])

    def get_blocks(self) -> list:
        blocks = sorted(self.raw.blocks.values(), key=lambda x: int(x["height"]))
        return [BlockscoutBlockDetails(int(block["height"]), self.raw) for block in blocks]
    
    def get_txs(self, block_number: int = None) -> list:
        txs = self.raw.txs.values()
        if block_number is not None:
            txs = [tx for tx in txs if int(tx["block_number"]) == block_number]
        txs = sorted(txs, key=lambda x: (int(x["block_number"]), int(x["position"])))
        return [BlockscoutTxDetails(tx["hash"], self.raw) for tx in txs]

    def get_logs(self, block_number: int = None, tx_hash: str = None) -> list:
        txs = self.get_txs(block_number)
        logs = []
        for tx in txs:
            if tx_hash is not None and tx.hash() != tx_hash:
                continue
            for i, log in enumerate(self.raw.logs[tx.hash()]["items"]):
                logs.append(BlockscoutLogDetails(tx, i, self.raw))
        return logs

class BlockscoutBlockDetails(BlockDetails):
    def hash(self) -> str:
        return self.raw_block["hash"]

    def timestamp(self) -> int:
        dt = datetime.fromisoformat(self.raw_block["timestamp"])
        dt = dt.replace(tzinfo=timezone.utc)
        return int(dt.timestamp())

    def l1_tx_count(self) -> int:
        # IMPORTANT!: This is generally incorrect, but Zero missing batches do not have L1 txs
        return 0

    def l2_tx_count(self) -> int:
        # IMPORTANT!: This is generally incorrect, but Zero missing batches do not have L1 txs
        return int(self.raw_block["tx_count"])

    def base_fee_per_gas(self) -> int:
        return int(self.raw_block["base_fee_per_gas"])


class BlockscoutTxDetails(TxDetails):
    def block_number(self) -> int:
        return int(self.raw_tx["block_number"])

    def is_l1_tx(self) -> bool:
        # IMPORTANT!: This is generally incorrect, but Zero missing batches do not have L1 txs
        return False

    def initiator_address(self) -> str:
        # Assuming the same as "from"
        return self.raw_tx["from"]["hash"].lower()

    def to(self) -> str:
        return self.raw_tx["to"]["hash"].lower()

    def nonce(self) -> int:
        return int(self.raw_tx["nonce"])

    def value(self) -> int:
        return int(self.raw_tx["value"])

    def calldata(self) -> str:
        return self.raw_tx["raw_input"]

    def factory_deps(self) -> list:
        # IMPORTANT!: This is generally incorrect, but Zero missing batches do not have any new factory deps
        return []
    
    def timestamp(self) -> int:
        # Format: 2025-12-17T17:12:06.000000Z
        dt = datetime.fromisoformat(self.raw_tx["timestamp"])
        dt = dt.replace(tzinfo=timezone.utc)
        return int(dt.timestamp())

    def index_in_block(self) -> int:
        return int(self.raw_tx["position"])

    def is_error(self) -> bool:
        return self.raw_tx["status"] == "error"

    def error(self) -> str | None:
        if self.is_error() and self.raw_tx["result"] is not None:
            return self.raw_tx["result"]
        return None

    def gas_limit(self) -> int:
        return int(self.raw_tx["gas_limit"])

    def gas_used(self) -> int:
        return int(self.raw_tx["gas_used"])

    def gas_per_pubdata_limit(self) -> int:
        return 50000

    def tx_format(self) -> int:
        return int(self.raw_tx["type"])

    def max_fee_per_gas(self) -> int:
        return int(self.raw_tx["max_fee_per_gas"])

    def max_priority_fee_per_gas(self) -> int:
        return int(self.raw_tx["max_priority_fee_per_gas"])

    def effective_gas_price(self) -> int:
        return int(self.raw_tx["gas_price"])


class BlockscoutLogDetails(LogDetails):
    def __init__(self, tx: TxDetails, log_index: int, raw: RawBatchData):
        super().__init__(tx, log_index, raw)
        self.raw_log = raw.logs[tx.hash()]["items"][log_index]

    def address(self) -> str:
        return self.raw_log["address"]["hash"].lower()

    def topics(self) -> list:
        return self.raw_log["topics"]

    def data(self) -> str:
        return self.raw_log["data"]

    def timestamp(self) -> int:
        return self.tx.timestamp()
