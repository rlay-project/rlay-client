use std::collections::HashMap;
use std::sync::Mutex;
use web3::types::U256;

use crate::aggregation::{detect_valued_pools, WeightedMedianBooleanPropositionPool};
use crate::payout::Payout;
use crate::sync_ontology::EntityMap;
use crate::sync_proposition_ledger::{EthProposition, PropositionLedger};

// TODO: U256 and get from RlayToken contract
const TOKENS_PER_BLOCK: f64 = 25000000000000000000f64;

/// Calculate the payouts for a completed epoch.
///
/// When calling this you need to make sure that the ledger for the epoch has been completed, and
/// that the local mirror of the ledger has been synced accordingly.
pub fn payouts_for_epoch(
    epoch: u64,
    epoch_start_block: U256,
    epoch_length: U256,
    ledger_mtx: &Mutex<PropositionLedger>,
    entity_map_mtx: &Mutex<EntityMap>,
) -> Vec<Payout> {
    let ledger = ledger_mtx
        .lock()
        .expect("Could not gain lock for ledger mutex");
    let entity_map = entity_map_mtx
        .lock()
        .expect("Could not gain lock for entity_map mutex");
    let epoch_end = (epoch * epoch_length.as_u64()) + epoch_start_block.as_u64();

    let relevant_propositions: Vec<_> = ledger
        .iter()
        .filter(|n| n.block_number <= epoch_end) // Filter out propositions that me might have already synced of a future epoch
        .collect();

    debug!(
        "Number of relevant propositions for epoch {} payout calculation: {}",
        epoch,
        relevant_propositions.len()
    );

    let ontology_entities: Vec<_> = entity_map.values().collect();
    let pools = detect_valued_pools(&ontology_entities, &relevant_propositions);

    for pool in &pools {
        trace!("-----POOL START-----");
        trace!("Values: {:?}", pool.fmt_values());
        trace!("Proposition: {:?}", pool.propositions);
        trace!("-----POOL END-----");
    }

    let per_proposition_payouts = calculate_payouts(&pools);
    let payouts = Payout::compact_payouts(per_proposition_payouts);

    payouts
}

/// Calculate the payouts for the supplied propositions.
///
/// Returns the payouts for each individual proposition,
/// which means that there might be two payouts for the same address.
fn calculate_payouts(pools: &[WeightedMedianBooleanPropositionPool]) -> Vec<Payout> {
    let pool_rank_map = build_pool_rank_map(pools);

    let mut payouts: Vec<_> = Vec::new();
    for pool in pools {
        let pool_factor = geometric_series_u64(*pool_rank_map.get(&pool.hash()).unwrap());

        let rewarded_propositions_factors = calculate_proposition_in_pool_factors(pool);
        for (proposition, factor) in rewarded_propositions_factors {
            // HACK: *2 since for some reason the sum of all only comes down
            // HACK: *0.999 so that floating point inaccuracies don't push us over the limit of
            // mined tokens. See issue #2.
            let reward: f64 = TOKENS_PER_BLOCK as f64 * pool_factor * factor * 2f64 * 0.999f64;

            let mut payout = Payout::empty_for_address(proposition.sender);
            payout.amount = payout.amount + Into::<U256>::into(reward as u64);
            payouts.push(payout);
        }
    }

    payouts
}

fn geometric_series(rank: f64) -> f64 {
    0.5f64.powi(rank as i32 + 1 as i32)
}

fn geometric_series_u64(rank: u64) -> f64 {
    0.5f64.powi(rank as i32 + 1 as i32)
}

/// Part of payout calculation (see [calculate_payouts])
fn build_pool_rank_map(pools: &[WeightedMedianBooleanPropositionPool]) -> HashMap<Vec<u8>, u64> {
    let mut pool_sizes = HashMap::new();
    for pool in pools {
        let size = pool.total_weight();
        pool_sizes.insert(pool.hash(), size);
    }

    let mut pool_ranks: Vec<(Vec<u8>, U256)> = pool_sizes
        .into_iter()
        .map(|(id, size): (Vec<u8>, U256)| (id, size))
        .collect();
    pool_ranks.sort_by_key(|&(_, size)| size);

    let pool_rank_map: HashMap<Vec<u8>, u64> = pool_ranks
        .into_iter()
        .enumerate()
        .map(|(i, (hash, _))| (hash, (i + 1) as u64))
        .collect();

    pool_rank_map
}

/// Calculate the factors for all the propositions inside one pool.
///
/// The sum of all factors should sum up to 1 (= the full reward paid out to the pool).
fn calculate_proposition_in_pool_factors(
    pool: &WeightedMedianBooleanPropositionPool,
) -> Vec<(&EthProposition, f64)> {
    let rewarded_propositions = build_rewarded_propositions(pool);

    let propositions_rank_age_map =
        build_propositions_rank_chronology_map(rewarded_propositions.clone());
    let propositions_weight_percentage_map =
        build_propositions_weight_percentage_map(rewarded_propositions.clone());

    let rewarded_propositions_factors = rewarded_propositions
        .into_iter()
        .map(|n| {
            let mut factor = 1f64;
            let age_rank_factor =
                geometric_series(*propositions_rank_age_map.get(&n).unwrap() as f64);
            factor *= age_rank_factor;
            factor *= propositions_weight_percentage_map.get(&n).unwrap();

            return (n, factor);
        })
        .collect::<Vec<_>>();
    let factors_sum: f64 = rewarded_propositions_factors
        .iter()
        .map(|(_, factor)| factor)
        .sum();
    let normalization = 1f64 / factors_sum;

    let rewarded_propositions_factors_normalized = rewarded_propositions_factors
        .into_iter()
        .map(|(n, factor)| (n, factor * normalization))
        .collect::<Vec<_>>();

    rewarded_propositions_factors_normalized
}

/// Build a list of stakes inside a pool that are elligable for rewards.
///
/// This is a simplified version of the `Distance` factor for boolean statments.
fn build_rewarded_propositions(
    pool: &WeightedMedianBooleanPropositionPool,
) -> Vec<&EthProposition> {
    pool.propositions
        .iter()
        .filter(|n| pool.is_aggregated_value(n))
        .collect::<Vec<_>>()
}

/// Build a ranking of propositions based on their age (= block of inclusion).
///
/// This is the `Chronology` factor for the payment function.
fn build_propositions_rank_chronology_map(
    propositions: Vec<&EthProposition>,
) -> HashMap<&EthProposition, usize> {
    let mut stakes_rank_age = propositions;

    stakes_rank_age.sort_by_key(|n| n.block_number);
    let stakes_rank_age_map: HashMap<_, _> = stakes_rank_age
        .into_iter()
        .enumerate()
        .map(|(i, stake)| (stake, i))
        .collect();
    stakes_rank_age_map
}

/// Build a mapping of stakes to the percentage of weight they represent in a pool.
///
/// This is the `Weight` factor for the payment function.
fn build_propositions_weight_percentage_map(
    propositions: Vec<&EthProposition>,
) -> HashMap<&EthProposition, f64> {
    let rewarded_stakes_total_weight: f64 = propositions
        .iter()
        .map(|n| n.amount)
        .fold(U256::zero(), |acc, val| acc + val)
        .as_u64() as f64;
    let stakes_weight_percentage_map: HashMap<_, _> = propositions
        .into_iter()
        .map(|n| (n, (n.amount.as_u64() as f64) / rewarded_stakes_total_weight))
        .collect();

    stakes_weight_percentage_map
}
