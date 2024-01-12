from Crypto.Hash import keccak

def ethereum_keccak(input_str):
    k = keccak.new(digest_bits=256)
    k.update(input_str.encode())
    return k.hexdigest()

def ethereum_keccak_bytes(input_bytes):
    k = keccak.new(digest_bits=256)
    k.update(input_bytes)
    return k.hexdigest()


# Example usage

hash_output = ethereum_keccak_bytes(bytes.fromhex("00000000000000000000000000000000000000000000000000000000000000af0000000000000000000000000000000000000000000000000000000000000008"))
print("Keccak-256:", hash_output)