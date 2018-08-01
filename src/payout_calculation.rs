use cid::ToCid;
use multibase::{encode as base_encode, Base};
use rlay_ontology::ontology::Individual;
use rlay_ontology::ontology;
use rquantiles::*;
use serde::Serializer;
use serde::ser::SerializeSeq;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use tiny_keccak::keccak256;
use web3::types::U256;

use payout::Payout;
use sync_proposition_ledger::{Proposition, PropositionLedger};
use sync_ontology::{entity_map_individuals, EntityMap};

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

    let ontology_individuals = entity_map_individuals(&entity_map);
    let pools = detect_pools(&ontology_individuals, &relevant_propositions);

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

pub type PropositionSubject<'a> = &'a [Vec<u8>];

#[derive(Debug, Clone)]
pub struct PropositionPool {
    pub values: Vec<ontology::Individual>,
    pub propositions: Vec<Proposition>,
    cached_quantiles: Option<Quantiles>,
}

impl ::serde::Serialize for PropositionPool {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::Serializer,
    {
        #[derive(Serialize)]
        #[allow(non_snake_case)]
        struct PropositionPoolSerialize {
            pub values: Vec<PropositionPoolValuesSerialize>,
            #[serde(serialize_with = "PropositionPool::serialize_subject")]
            pub subject: Vec<Vec<u8>>,
            pub totalWeight: U256,
        }

        #[derive(Serialize)]
        #[allow(non_snake_case)]
        struct PropositionPoolValuesSerialize {
            pub cid: String,
            pub totalWeight: U256,
            pub isAggregatedValue: bool,
        }

        let formatted_values = self.values
            .iter()
            .map(|individual| PropositionPoolValuesSerialize {
                cid: individual.to_cid().unwrap().to_string(),
                totalWeight: self.weights_for_value(individual),
                isAggregatedValue: self.is_aggregated_value_individual(individual),
            })
            .collect();

        let ext = PropositionPoolSerialize {
            values: formatted_values,
            subject: self.subject().to_owned(),
            totalWeight: self.total_weight(),
        };

        Ok(try!(ext.serialize(serializer)))
    }
}

impl PropositionPool {
    pub fn from_values(mut values: Vec<ontology::Individual>) -> PropositionPool {
        values.sort_by_key(|n| n.to_cid().unwrap().to_bytes());
        PropositionPool {
            values,
            propositions: Vec::new(),

            cached_quantiles: None,
        }
    }

    pub fn subject(&self) -> PropositionSubject {
        &self.values.get(0).unwrap().annotations
    }

    pub fn contains_value(&self, individual: &ontology::Individual) -> bool {
        self.values.contains(individual)
    }

    pub fn contains_value_cid(&self, cid: Vec<u8>) -> bool {
        self.values
            .iter()
            .map(|n| n.to_cid().unwrap().to_bytes())
            .collect::<Vec<Vec<u8>>>()
            .contains(&cid)
    }

    /// Checks if the provided values are equal to all the possible values for this pool.
    pub fn is_complete(&self) -> bool {
        // for boolean pools (the only supported ones at the moment) the check is pretty simple
        self.values.len() == 2
    }

    /// Helper for printing the values of a PropositionPool.
    pub fn fmt_values(&self) -> Vec<String> {
        self.values
            .iter()
            .map(|n| n.to_cid().unwrap().to_string())
            .collect()
    }

    /// Sum of all the weights of the propositions in this pool
    pub fn total_weight(&self) -> U256 {
        self.propositions
            .iter()
            .map(|n| n.amount)
            .fold(U256::zero(), |acc, val| acc + val)
    }

    // This only works if we have a complete pool; Might need another solution for the future
    pub fn hash(&self) -> Vec<u8> {
        debug_assert!(self.is_complete());
        let hash_data = self.values
            .iter()
            .map(|n| n.to_cid().unwrap().to_bytes())
            .fold(Vec::new(), |mut acc, mut val| {
                acc.append(&mut val);
                val
            });
        keccak256(&hash_data).to_vec()
    }

    /// Sum up the weights of all propositions for a single value.
    fn weights_for_value(&self, value: &ontology::Individual) -> U256 {
        let cid = value.to_cid().unwrap().to_bytes();
        self.propositions
            .iter()
            .filter(|n| n.proposition_cid == cid)
            .map(|n| n.amount)
            .fold(U256::zero(), |acc, val| acc + val)
    }

    /// Calculate the weighted quantiles of the propositions in this pool.
    // Currently a speced down version that works with boolean statements
    fn calculate_quantiles(&self) -> Quantiles {
        let false_weight = self.weights_for_value(&self.values[0]).as_u32();
        let true_weight = self.weights_for_value(&self.values[1]).as_u32();

        let values = vec![0, 1];
        let weights = vec![false_weight, true_weight];
        calculate_quantiles(values, weights)
    }

    /// Returns the weighted quantiles of the propositions in this pool.
    ///
    /// Internally caches the computation result, as the current way we compute them by calling out
    /// to a R program is very slow.
    fn quantiles(&self) -> Quantiles {
        if let Some(ref quantiles) = self.cached_quantiles {
            return quantiles.clone();
        }
        self.calculate_quantiles()
    }

    /// Returns the weighted median of the propositions in this pool.
    pub fn aggregated_value(&self) -> Option<bool> {
        match self.quantiles().median as i32 {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }

    pub fn is_aggregated_value_individual(&self, val: &Individual) -> bool {
        let aggregated = match self.aggregated_value() {
            None => return false,
            Some(val) => val,
        };
        let false_value_cid = self.values[0].to_cid().unwrap().to_bytes();
        let true_value_cid = self.values[1].to_cid().unwrap().to_bytes();

        let val_cid = val.to_cid().unwrap().to_bytes();

        if val_cid == false_value_cid && aggregated == false {
            return true;
        }
        if val_cid == true_value_cid && aggregated == true {
            return true;
        }
        return false;
    }

    pub fn is_aggregated_value(&self, val: &Proposition) -> bool {
        let aggregated = match self.aggregated_value() {
            None => return false,
            Some(val) => val,
        };
        let false_value_cid = self.values[0].to_cid().unwrap().to_bytes();
        let true_value_cid = self.values[1].to_cid().unwrap().to_bytes();

        if val.proposition_cid == false_value_cid && aggregated == false {
            return true;
        }
        if val.proposition_cid == true_value_cid && aggregated == true {
            return true;
        }
        return false;
    }

    pub fn serialize_subject<S>(vals: &Vec<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(vals.len()))?;
        for val in vals.iter() {
            seq.serialize_element(&base_encode(Base::Base58btc, val))?;
        }
        seq.end()
    }
}

fn debug_unsupported_individual(individual: &ontology::Individual, msg: &str) {
    let cid = individual.to_cid().unwrap();
    debug!("Can't use individual {} for pool building: {}", cid, msg);
}

type ClassAssertionObject = Vec<Vec<u8>>;

/// Tries to find either a class_assertion or a negative_class_assertion in an Individual.
fn extract_class_assertion_object(
    individual: &ontology::Individual,
) -> Option<ClassAssertionObject> {
    let mut class_assertion_object: Option<ClassAssertionObject> = None;

    if individual.class_assertions.len() > 0 {
        if individual.class_assertions.len() > 1 {
            debug_unsupported_individual(
                individual,
                "multiple class_assertions are currently not supported",
            );
            return None;
        }
        class_assertion_object = Some(vec![individual.class_assertions.get(0).unwrap().to_owned()]);
    }
    if individual.negative_class_assertions.len() > 0 {
        if individual.negative_class_assertions.len() > 1 {
            debug_unsupported_individual(
                individual,
                "multiple negative_class_assertions are currently not supported",
            );
            return None;
        }
        if class_assertion_object.is_some() {
            debug_unsupported_individual(
                        individual,
                        "individuals with both class_assertions and negative_class_assertions are are currently not supported",
                    );
            return None;
        }
        class_assertion_object = Some(vec![
            individual
                .negative_class_assertions
                .get(0)
                .unwrap()
                .to_owned(),
        ]);
    }

    class_assertion_object
}

/// Constructs all the existent pools that arise from all used propositions.
///
/// Goes through all the individuals used in propositions and finds individuals that
/// assert or negatively assert class memberships about the same subject.
pub fn detect_pools(
    ontology_individuals: &[&ontology::Individual],
    propositions: &[&Proposition],
) -> Vec<PropositionPool> {
    let used_cids: HashSet<Vec<u8>> = propositions
        .iter()
        .map(|n| n.proposition_cid.clone())
        .collect();
    let used_individuals: Vec<_> = ontology_individuals
        .iter()
        .filter(|n| {
            let cid = n.to_cid().unwrap().to_bytes();
            used_cids.contains(&cid)
        })
        .collect();

    let mut individuals_by_subject: HashMap<PropositionSubject, Vec<&ontology::Individual>> =
        HashMap::new();
    for individual in used_individuals {
        let mut entry = individuals_by_subject
            .entry(&individual.annotations)
            .or_insert(Vec::new());
        entry.push(individual);
    }

    let mut pools = Vec::new();
    for (_, individuals) in individuals_by_subject {
        let mut individuals_by_class_assertion_object: HashMap<
            ClassAssertionObject,
            Vec<&ontology::Individual>,
        > = HashMap::new();

        for individual in individuals {
            let mut class_assertion_object = extract_class_assertion_object(individual);
            if class_assertion_object.is_none() {
                debug_unsupported_individual(
                        individual,
                        "individuals without class_assertions and negative_class_assertions are are currently not supported",
                    );
                continue;
            }

            let class_assertion_obj = class_assertion_object.unwrap();
            let entry = individuals_by_class_assertion_object
                .entry(class_assertion_obj)
                .or_insert(Vec::new());
            entry.push(individual);
        }

        for (_, values) in individuals_by_class_assertion_object {
            let pool = PropositionPool::from_values(values.iter().map(|n| (*n).clone()).collect());
            if !pool.is_complete() {
                debug!("Pool of values {:?} is incomplete", pool.fmt_values());
                continue;
            }
            pools.push(pool);
        }
    }

    pools = pools
        .into_iter()
        .map(|mut pool| {
            for proposition in propositions {
                if pool.contains_value_cid(proposition.proposition_cid.clone()) {
                    pool.propositions.push((*proposition).clone());
                }
            }
            pool
        })
        .collect();

    pools
}

/// Calculate the payouts for the supplied propositions.
///
/// Returns the payouts for each individual proposition,
/// which means that there might be two payouts for the same address.
fn calculate_payouts(pools: &[PropositionPool]) -> Vec<Payout> {
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
            payout.amount = payout.amount + (reward as u64).into();
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
fn build_pool_rank_map(pools: &[PropositionPool]) -> HashMap<Vec<u8>, u64> {
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
fn calculate_proposition_in_pool_factors(pool: &PropositionPool) -> Vec<(&Proposition, f64)> {
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
fn build_rewarded_propositions(pool: &PropositionPool) -> Vec<&Proposition> {
    pool.propositions
        .iter()
        .filter(|n| pool.is_aggregated_value(n))
        .collect::<Vec<_>>()
}

/// Build a ranking of propositions based on their age (= block of inclusion).
///
/// This is the `Chronology` factor for the payment function.
fn build_propositions_rank_chronology_map(
    propositions: Vec<&Proposition>,
) -> HashMap<&Proposition, usize> {
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
    propositions: Vec<&Proposition>,
) -> HashMap<&Proposition, f64> {
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
