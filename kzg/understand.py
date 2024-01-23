import json
from Crypto.Hash import keccak
import hashlib
from typing import Sequence, T

from py_ecc.bls12_381 import bls12_381_curve

with open("kzg/testdata/single_blob.json", "r") as f:
    data = json.load(f)


payload = data["data"]
commitment = data["commitment"]
proof = data["proof"]

# Blobs have 128k, here it is encoded as 'hex' bytes - so 2 chars per blob.
# 128k * 2 == 256k (2**18)
assert(len(payload) == 2**18)


print(len(data["data"]))

# verify versioned hash

VERSIONED_HASH_VERSION_KZG = "01"

def kzg_to_versioned_hash(commitment):
    k = hashlib.sha256()
    k.update(bytes.fromhex(commitment))
    return VERSIONED_HASH_VERSION_KZG + k.hexdigest()[2:]


computed_hash = kzg_to_versioned_hash(commitment)
if "versionHash" in data:
    if data["versionHash"] != computed_hash:
        raise Exception(f"Versioned hash differ: {data['versionHash']} vs {computed_hash}")
    print("Versioned hash ok.")
else:
    print(f"Versioned hash: {computed_hash}")



## Now let's try to compute kzg commitment part.
    
# The polynomial-commitments.md says:
#     return g1_lincomb(bit_reversal_permutation(KZG_SETUP_LAGRANGE), blob_to_polynomial(blob))

# lincomb is simply a linear combination:
# we start at 'z1' - and we keep adding (p1[x] * p2[x])


BLS_MODULUS = 52435875175126190479447740508185965837690552500527637822603658699938581184513

def reverse_bits(n: int, order: int) -> int:
    """
    Reverse the bit order of an integer ``n``.
    """
    # Convert n to binary with the same number of bits as "order" - 1, then reverse its bit order
    return int(('{:0' + str(order.bit_length() - 1) + 'b}').format(n)[::-1], 2)

def bit_reversal_permutation(sequence: Sequence[T]) -> Sequence[T]:
    """
    Return a copy with bit-reversed permutation. The permutation is an involution (inverts itself).

    The input and output are a sequence of generic type ``T`` objects.
    """
    return [sequence[reverse_bits(i, len(sequence))] for i in range(len(sequence))]



def load_trusted_setup():
    with open("kzg/trusted_setup.txt") as f:
        trusted_setup = f.readlines()
    
    assert(int(trusted_setup[0]) == 4096)
    assert(int(trusted_setup[1]) == 65)

    # These should be in lagrange form
    g1_points = []
    for i in range(4096):
        # TODO - check the endianness
        g1_points.append(int.from_bytes(bytes.fromhex(trusted_setup[i + 2]), 'little'))
    
    # And these in monomial form 
    g2_points = []
    for i in range(65):
        g2_points.append(trusted_setup[i + 2 + 4096])

    return (g1_points, g2_points)


print(bit_reversal_permutation([0, 1, 2, 3, 4, 5, 6, 7]))

# now 'blob_to_polynomial' part
FIELD_ELEMENTS_PER_BLOB	= 4096
BYTES_PER_FIELD_ELEMENT = 32
# 4096 of 32 bytes elements.
payload_as_bytes = bytes.fromhex(payload)
assert(len(payload_as_bytes) == FIELD_ELEMENTS_PER_BLOB * BYTES_PER_FIELD_ELEMENT)
polynomial = []
for i in range(FIELD_ELEMENTS_PER_BLOB):
    # TODO - not sure about endianness
    polynomial.append(int.from_bytes(payload_as_bytes[i*BYTES_PER_FIELD_ELEMENT: (i+1)*BYTES_PER_FIELD_ELEMENT], 'big'))

# great - now bytes are a in polynomial 'format' (so 4096 32-byte entries).
    
# TODO: load real values
kzg_setup = [0] * 4096

# TODO: should be a 'z1' point
result = 0
for i in range(FIELD_ELEMENTS_PER_BLOB):
    result += polynomial[i] * kzg_setup[i]

print(BLS_MODULUS)
print(BLS_MODULUS* BLS_MODULUS)

(g1_points, g2_points) = load_trusted_setup()

print(f" Point 0 is: {g1_points[0]}")
print(f"             {BLS_MODULUS}")
print(f"   order     {bls12_381_curve.curve_order}")
print(f"   field mod {bls12_381_curve.field_modulus}")
print(f"   g1:       {bls12_381_curve.G1}")
print(f"   z1:       {bls12_381_curve.Z1}")




# aliased as bytes48_to_G1
#def pubkey_to_G1(pubkey: BLSPubkey) -> G1Uncompressed:
#    z = os2ip(pubkey) # bigendian
#    return decompress_G1(G1Compressed(z))


print(bls12_381_curve.is_on_curve(bls12_381_curve.add(bls12_381_curve.G1, bls12_381_curve.G1), bls12_381_curve.b))
print(bls12_381_curve.is_on_curve(bls12_381_curve.multiply(bls12_381_curve.G1, 1000000), bls12_381_curve.b))

print(bls12_381_curve.multiply(bls12_381_curve.G1, 100000000000000))
print(bls12_381_curve.G1)