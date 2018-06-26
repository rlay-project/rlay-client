use std::sync::Mutex;
use std::fmt::Write;
use std::collections::HashMap;
use merkle_light::hash::Hashable;
use merkle_light::merkle2::MerkleTree;
use web3::types::{Address, U256};
use std::hash::Hasher;
use rustc_hex::ToHex;

use sync_proposition_ledger::PropositionLedger;
use merkle::Keccak256Algorithm;

/// Number of host blockchain blocks that make up a epoch
const EPOCH_LENGTH: u64 = 20;
/// This block is the start of the first epoch
const EPOCH_START_BLOCK: u64 = 0;

pub type PayoutEpochs = HashMap<u64, Vec<Payout>>;

#[derive(Debug, Clone)]
pub struct Payout {
    pub address: Address,
    pub amount: U256,
}

impl Payout {
    fn empty_for_address(address: Address) -> Self {
        Self {
            address,
            amount: U256::zero(),
        }
    }

    /// Build a merkle tree for a list of payouts
    pub fn build_merkle_tree(payouts: &[Self]) -> MerkleTree<[u8; 32], Keccak256Algorithm> {
        // TODO: if only a single or no payout, pad with payout to zero address
        MerkleTree::from_data(payouts)
    }
}

impl<H: Hasher> Hashable<H> for Payout {
    fn hash(&self, state: &mut H) {
        self.address.hash(state);

        let mut amount_bytes = [0u8; 32];
        self.amount.to_big_endian(&mut amount_bytes);
        amount_bytes.hash(state);
    }
}

/// Calculate the payouts for a completed epoch.
///
/// When calling this you need to make sure that the ledger for the epoch has been completed, and
/// that the local mirror of the ledger has been synced accordingly.
pub fn payouts_for_epoch(epoch: u64, ledger_mtx: &Mutex<PropositionLedger>) -> Vec<Payout> {
    let ledger = ledger_mtx
        .lock()
        .expect("Could not gain lock for ledger mutex");
    let epoch_end = (epoch * EPOCH_LENGTH) + EPOCH_START_BLOCK;

    let relevant_propositions: Vec<_> = ledger
        .iter()
        .filter(|n| n.block_number <= epoch_end)
        .collect();

    debug!(
        "Number of relevant propositions for epoch {} payout calculation: {}",
        epoch,
        relevant_propositions.len()
    );
    let mut payouts: HashMap<Address, Payout> = HashMap::new();
    for proposition in relevant_propositions {
        let mut payout = payouts
            .entry(proposition.sender)
            .or_insert(Payout::empty_for_address(proposition.sender));
        payout.amount = payout.amount + U256::one();
    }

    payouts.values().cloned().collect()
}

pub fn fill_epoch_payouts(
    ledger_block_highwatermark_mtx: &Mutex<u64>,
    ledger_mtx: &Mutex<PropositionLedger>,
    payout_epochs_mtx: &Mutex<PayoutEpochs>,
) {
    let ledger_block_highwatermark = ledger_block_highwatermark_mtx.lock().unwrap();
    let mut payout_epochs = payout_epochs_mtx.lock().unwrap();

    let latest_completed_epoch = (*ledger_block_highwatermark - EPOCH_START_BLOCK) / EPOCH_LENGTH;
    debug!("Ledger sync highwatermark: {}", ledger_block_highwatermark);
    debug!("Latest completed epoch: {}", latest_completed_epoch);
    for epoch in 0..=latest_completed_epoch {
        if payout_epochs.contains_key(&epoch) {
            continue;
        }

        let payouts = payouts_for_epoch(epoch, ledger_mtx);
        debug!("Calculated payouts for epoch {}: {:?}", epoch, payouts);
        payout_epochs.insert(epoch, payouts);
    }
}

pub fn format_redeem_payout_call(
    epoch: u64,
    tree: &MerkleTree<[u8; 32], Keccak256Algorithm>,
    payout: &Payout,
) -> String {
    let proof = ::merkle::gen_proof_for_data(&tree, payout);
    let lemma = proof.lemma().to_owned();
    let mut proof_str = String::new();

    write!(&mut proof_str, "redeemPayout(").unwrap();
    write!(&mut proof_str, "{}, ", epoch).unwrap();
    write!(&mut proof_str, "[").unwrap();
    for proof_item in lemma.iter().skip(1).take(lemma.len() - 2) {
        write!(&mut proof_str, "'0x{}',", proof_item.to_hex()).unwrap();
    }
    write!(&mut proof_str, "],").unwrap();
    write!(&mut proof_str, "'0x{}'", payout.address.to_hex()).unwrap();
    write!(&mut proof_str, ",").unwrap();
    write!(&mut proof_str, "{}", payout.amount).unwrap();
    write!(&mut proof_str, ")").unwrap();

    return proof_str;
}
