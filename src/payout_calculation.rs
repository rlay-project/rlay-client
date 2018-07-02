use std::collections::HashMap;
use std::sync::Mutex;
use web3::types::{Address, U256};

use payout::{Payout, EPOCH_LENGTH, EPOCH_START_BLOCK};
use sync_proposition_ledger::PropositionLedger;

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
