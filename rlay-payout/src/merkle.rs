use merkle_light::hash::{Algorithm, Hashable};
use merkle_light::merkle2::MerkleTree;
use merkle_light::proof2::Proof;
use std::hash::Hasher;
use tiny_keccak::keccak256;

pub struct Keccak256Algorithm {
    buffer: Vec<u8>,
}

impl Keccak256Algorithm {
    pub fn new() -> Keccak256Algorithm {
        Keccak256Algorithm { buffer: Vec::new() }
    }
}

impl Default for Keccak256Algorithm {
    fn default() -> Keccak256Algorithm {
        Keccak256Algorithm::new()
    }
}

impl Hasher for Keccak256Algorithm {
    #[inline]
    fn write(&mut self, msg: &[u8]) {
        for byte in msg {
            self.buffer.push(*byte);
        }
    }

    #[inline]
    fn finish(&self) -> u64 {
        unimplemented!()
    }
}

impl Algorithm<[u8; 32]> for Keccak256Algorithm {
    #[inline]
    fn hash(&mut self) -> [u8; 32] {
        keccak256(&self.buffer)
    }

    #[inline]
    fn reset(&mut self) {
        self.buffer = Vec::new();
    }

    #[inline]
    fn leaf(&mut self, leaf: [u8; 32]) -> [u8; 32] {
        self.write(leaf.as_ref());
        self.hash()
    }

    #[inline]
    fn node(&mut self, left: [u8; 32], right: [u8; 32], _height: usize) -> [u8; 32] {
        let mut elements = vec![left.as_ref(), right.as_ref()];
        elements.sort();

        self.write(elements[0]);
        self.write(elements[1]);
        let result = self.hash();

        result
    }
}

pub fn gen_proof_for_data<T: Ord + Eq + Clone + AsRef<[u8]>, A: Algorithm<T>, D: Hashable<A>>(
    tree: &MerkleTree<T, A>,
    data: &D,
) -> Proof<T> {
    let mut a = A::default();
    data.hash(&mut a);
    let item = a.hash();
    a.reset();
    let leaf_hash = a.leaf(item);

    let index = tree
        .as_slice()
        .iter()
        .position(|n| *n == leaf_hash)
        .unwrap();
    tree.gen_proof(index)
}
