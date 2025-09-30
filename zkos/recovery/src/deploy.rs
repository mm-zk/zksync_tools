use alloy::primitives::B256;
use blake2::{Blake2s256, Digest, digest::typenum::Bit};
use zk_os_basic_system::system_implementation::flat_storage_model::bytecode_padding_len;

pub struct BitMap {
    inner: Vec<usize>,
}

impl BitMap {
    pub(crate) fn allocate_for_bit_capacity(capacity: usize) -> Self {
        let u64_capacity = capacity.next_multiple_of(u64::BITS as usize) / (u64::BITS as usize);
        let word_capacity = u64_capacity * (u64::BITS as usize / usize::BITS as usize);
        let mut storage = Vec::with_capacity(word_capacity);
        storage.resize(word_capacity, 0);

        Self { inner: storage }
    }
    pub(crate) unsafe fn set_bit_on_unchecked(&mut self, pos: usize) {
        let (word_idx, bit_idx) = (pos / usize::BITS as usize, pos % usize::BITS as usize);
        let dst = unsafe { self.inner.get_unchecked_mut(word_idx) };
        *dst |= 1usize << bit_idx;
    }

    pub fn as_words(&self) -> &[usize] {
        &self.inner
    }

    pub fn as_slice(&self) -> &[u8] {
        let words = self.as_words();
        let len_bytes = core::mem::size_of_val(words);
        let ptr = words.as_ptr();
        unsafe { core::slice::from_raw_parts(ptr.cast::<u8>(), len_bytes) }
    }
}

pub const JUMPDEST: u8 = 0x5b;
pub const PUSH1: u8 = 0x60;
pub const PUSH32: u8 = 0x7f;

// copied from evm_interpreter/src/lib.rs
fn find_jumpdest(code: &[u8]) -> BitMap {
    let code_len = code.len();
    let mut jumps = BitMap::allocate_for_bit_capacity(code_len);

    let mut i = 0;
    while i < code_len {
        let op = code[i];
        if op == JUMPDEST {
            // SAFETY: `i` is always < code_len
            unsafe { jumps.set_bit_on_unchecked(i) };
            i += 1;
        } else if (PUSH1..=PUSH32).contains(&op) {
            i += 1 + (op - PUSH1 + 1) as usize;
        } else {
            i += 1;
        }
    }

    jumps
}

pub struct BytecodeAnalysisResults {
    pub bytecode_hash: B256,
    pub bytecode: Vec<u8>,
    pub artifacts: Vec<u8>,
    pub artifacts_len: usize,
    pub hash_with_artifacts: B256,
    pub bytecode_padded_with_artifacts: Vec<u8>,
}

pub fn analyze_bytecode(bytecode: &[u8]) -> BytecodeAnalysisResults {
    println!("Bytecode analysis:");
    let bytecode_hash = B256::from_slice(&Blake2s256::digest(&bytecode));

    println!(
        "original bytecoded hash: 0x{})",
        hex::encode(bytecode_hash.as_slice())
    );

    let jumpdest_map = find_jumpdest(bytecode);

    let artifacts = jumpdest_map.as_slice();

    let padding = [0u8; core::mem::size_of::<u64>() - 1];
    let mut bytecode_padded_with_artifacts = bytecode.to_vec();
    bytecode_padded_with_artifacts
        .extend_from_slice(&padding[..bytecode_padding_len(bytecode.len())]);
    bytecode_padded_with_artifacts.extend_from_slice(artifacts);

    let mut hasher = Blake2s256::new();
    hasher.update(bytecode_padded_with_artifacts.as_slice());
    let hash_with_artifacts = B256::from_slice(&hasher.finalize());
    println!(
        "padded bytecode hash: 0x{})",
        hex::encode(hash_with_artifacts.as_slice())
    );

    println!(" artifacts length: {}", artifacts.len());

    BytecodeAnalysisResults {
        bytecode_hash,
        bytecode: bytecode.to_vec(),
        artifacts: artifacts.to_vec(),
        artifacts_len: artifacts.len(),
        hash_with_artifacts,
        bytecode_padded_with_artifacts,
    }
}
