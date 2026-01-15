import json
import os

# IMPORTANT!: if reading from file, the assumption is that the data contains
# the entry for the last index. This is so that newly added entries would have the
# correct index assigned.
class Tree:
    def __init__(self, filepath=None):
        self.filepath = filepath
        self.data = {} # hashed_key -> { "address": "...", "key": "...", "value": "...", "index": int }
        self.index_to_hashed_key = {} # index -> hashed_key
        self._max_index = 0
        
        if filepath:
            with open(filepath, 'r') as f:
                self.data = json.load(f)
                for hashed_key, entry in self.data.items():
                    self.index_to_hashed_key[entry["index"]] = hashed_key
                    self._max_index = max(self._max_index, entry["index"])

    def save(self, filepath=None):
        if filepath is None:
            if self.filepath:
                filepath = self.filepath
            else:
                filepath = "tree.json"
        with open(filepath, 'w') as f:
            sorted_data = dict(sorted(self.data.items(), key=lambda x: int(x[1]['index'])))
            json.dump(sorted_data, f, indent=2, sort_keys=False)

    def get_address(self, hashed_key: str):
        return self.get(hashed_key).get("address")

    def get_key(self, hashed_key: str):
        return self.get(hashed_key).get("key")

    def get_value(self, hashed_key: str):
        return self.get(hashed_key)["value"]
    
    def get_hashed_key(self, index: int):
        return self.index_to_hashed_key[index]

    def get(self, hashed_key: str) -> dict:
        hashed_key = self._normalize(hashed_key)
        return self.data.get(hashed_key, {})

    def set(self, hashed_key: str, value: str, address = None, key = None):
        hashed_key = self._normalize(hashed_key)
        value = self._normalize(value)
        address = self._normalize(address) if address else None
        key = self._normalize(key) if key else None

        if hashed_key in self.data:
            self.data[hashed_key]["value"] = value
        else:
            new_index = self._max_index + 1
            self._max_index = new_index
            self.data[hashed_key] = {
                "address": address,
                "key": key,
                "value": value,
                "index": new_index
            }
            self.index_to_hashed_key[new_index] = hashed_key
    
    def size(self):
        return len(self.data)

    def _normalize(self, s: str) -> str:
        if not s.startswith("0x"):
            s = f"0x{s}"
        if len(s) != 66:
            raise ValueError(f"Must be exactly 64 hex characters long: {s}")
        try:
            int(s, 16)
        except ValueError:
            raise ValueError(f"Contains non-hex characters: {s}")
        return s
