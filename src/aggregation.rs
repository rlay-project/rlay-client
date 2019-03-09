use ::web3::types::U256;
use cid::ToCid;
use rlay_ontology::ontology;
use rlay_ontology::prelude::*;
use serde::Serializer;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tiny_keccak::keccak256;

use crate::ontology_ext::*;
use crate::sync_proposition_ledger::Proposition;
use crate::web3_helpers::{base58_encode, HexString};

pub type PropositionSubject<'a> = &'a [u8];

#[derive(Debug, Clone)]
pub struct BooleanPropositionPool {
    pub values: Vec<Assertion>,
}

impl BooleanPropositionPool {
    pub fn new() -> BooleanPropositionPool {
        BooleanPropositionPool { values: Vec::new() }
    }

    pub fn from_value(value: Assertion) -> BooleanPropositionPool {
        BooleanPropositionPool {
            values: vec![value],
        }
    }

    pub fn to_complete_pool(self) -> BooleanPropositionPool {
        let mut pool = self;

        if !pool.has_positive_value() {
            let new_val = pool.canonical_positive_value();
            pool.try_insert(new_val);
        }
        if !pool.has_negative_value() {
            let new_val = pool.canonical_negative_value();
            pool.try_insert(new_val);
        }
        debug_assert!(pool.values.len() >= 2);

        pool
    }

    pub fn try_insert(&mut self, assertion: Assertion) -> bool {
        if self.values.is_empty() {
            self.values.push(assertion);
            return true;
        }
        if assertion.canonical_parts() != self.canonical_parts() {
            return false;
        }
        self.values.push(assertion);

        true
    }

    pub fn subject(&self) -> PropositionSubject {
        self.values.get(0).unwrap().get_subject().unwrap()
    }

    pub fn subject_property(&self) -> Vec<&[u8]> {
        self.values.get(0).unwrap().get_subject_property()
    }

    pub fn target(&self) -> Option<&[u8]> {
        self.values.get(0).unwrap().get_target()
    }

    pub fn canonical_positive_value(&self) -> Assertion {
        let first_val = self.values.get(0).unwrap().clone();

        if first_val.is_positive() {
            first_val.canonical_assertion()
        } else {
            first_val.canonical_opposite_assertion()
        }
    }

    pub fn canonical_negative_value(&self) -> Assertion {
        let first_val = self.values.get(0).unwrap().clone();

        if first_val.is_negative() {
            first_val.canonical_assertion()
        } else {
            first_val.canonical_opposite_assertion()
        }
    }

    pub fn contains_value(&self, entity: &Assertion) -> bool {
        self.values.contains(entity)
    }

    pub fn value_cids(&self) -> Vec<Vec<u8>> {
        self.values
            .iter()
            .map(|n| n.to_cid().unwrap().to_bytes())
            .collect::<Vec<Vec<u8>>>()
    }

    pub fn contains_value_cid(&self, cid: Vec<u8>) -> bool {
        self.value_cids().contains(&cid)
    }

    pub fn positive_values(&self) -> Vec<Assertion> {
        self.values
            .clone()
            .into_iter()
            .filter(IsPositiveAssertion::is_positive)
            .collect()
    }

    pub fn has_positive_value(&self) -> bool {
        !self.positive_values().is_empty()
    }

    pub fn negative_values(&self) -> Vec<Assertion> {
        self.values
            .clone()
            .into_iter()
            .filter(IsNegativeAssertion::is_negative)
            .collect()
    }

    pub fn has_negative_value(&self) -> bool {
        !self.negative_values().is_empty()
    }

    /// Checks if the provided values are equal to all the possible values for this pool.
    pub fn is_complete(&self) -> bool {
        // a boolean pool is complete if it has at least on positive and one negative assertion
        let has_positive = self.has_positive_value();
        let has_negative = self.has_negative_value();

        has_positive && has_negative
    }

    /// Helper for printing the values of a BooleanPropositionPool.
    pub fn fmt_values(&self) -> Vec<String> {
        self.values
            .iter()
            .map(|n| n.to_cid().unwrap().to_string())
            .collect()
    }

    pub fn hash(&self) -> Vec<u8> {
        let hash_data = self
            .canonical_parts()
            .into_iter()
            .fold(Vec::new(), |mut acc, mut val| {
                acc.append(&mut val);
                val
            });
        keccak256(&hash_data).to_vec()
    }

    pub fn serialize_subject<S>(val: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&base58_encode(val))
    }
}

impl ::serde::Serialize for BooleanPropositionPool {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::Serializer,
    {
        #[derive(Serialize)]
        struct BooleanPropositionPoolSerialize {
            values: Vec<FormatWeb3<Entity>>,
            positive_values: Vec<FormatWeb3<Entity>>,
            negative_values: Vec<FormatWeb3<Entity>>,
            canonical_positive_value: FormatWeb3<Entity>,
            canonical_negative_value: FormatWeb3<Entity>,
        }

        let ext = BooleanPropositionPoolSerialize {
            values: self
                .values
                .clone()
                .into_iter()
                .map(|n| FormatWeb3(Into::<Entity>::into(n)))
                .collect(),
            positive_values: self
                .positive_values()
                .clone()
                .into_iter()
                .map(|n| FormatWeb3(Into::<Entity>::into(n)))
                .collect(),
            negative_values: self
                .negative_values()
                .clone()
                .into_iter()
                .map(|n| FormatWeb3(Into::<Entity>::into(n)))
                .collect(),
            canonical_positive_value: FormatWeb3(Into::<Entity>::into(
                self.canonical_positive_value(),
            )),
            canonical_negative_value: FormatWeb3(Into::<Entity>::into(
                self.canonical_negative_value(),
            )),
        };

        Ok(ext.serialize(serializer)?)
    }
}

impl CanonicalParts for BooleanPropositionPool {
    fn canonical_parts(&self) -> Vec<Vec<u8>> {
        self.values[0].canonical_parts()
    }
}

#[derive(Debug, Clone)]
pub struct ValuedBooleanPropositionPool {
    pub pool: BooleanPropositionPool,
    pub propositions: Vec<Proposition>,
}

impl ValuedBooleanPropositionPool {
    pub fn from_pool(pool: BooleanPropositionPool) -> Self {
        Self {
            pool: pool.to_complete_pool(),

            propositions: Vec::new(),
        }
    }

    pub fn is_complete(&self) -> bool {
        self.pool.is_complete()
    }

    pub fn contains_value_cid(&self, cid: Vec<u8>) -> bool {
        self.pool.contains_value_cid(cid)
    }

    pub fn fmt_values(&self) -> Vec<String> {
        self.pool.fmt_values()
    }

    pub fn hash(&self) -> Vec<u8> {
        self.pool.hash()
    }

    fn values(&self) -> &[Assertion] {
        &self.pool.values
    }

    /// Sum of all the weights of the propositions in this pool
    pub fn total_weight(&self) -> U256 {
        self.propositions
            .iter()
            .map(|n| n.amount)
            .fold(U256::zero(), |acc, val| acc + val)
    }

    /// Sum up the weights of all propositions for a single value.
    fn weights_for_value(&self, value: &Assertion) -> U256 {
        let cid = value.to_cid().unwrap().to_bytes();
        self.propositions
            .iter()
            .filter(|n| n.proposition_cid == cid)
            .map(|n| n.amount)
            .fold(U256::zero(), |acc, val| acc + val)
    }

    fn positive_weights(&self) -> U256 {
        self.pool
            .positive_values()
            .iter()
            .map(|n| self.weights_for_value(n))
            .fold(U256::zero(), |acc, val| acc + val)
    }

    fn negative_weights(&self) -> U256 {
        self.pool
            .negative_values()
            .iter()
            .map(|n| self.weights_for_value(n))
            .fold(U256::zero(), |acc, val| acc + val)
    }

    /// Returns the weighted median of the propositions in this pool.
    pub fn aggregated_value(&self) -> Option<bool> {
        let false_weight = self.negative_weights().as_u32();
        let true_weight = self.positive_weights().as_u32();

        if false_weight == true_weight {
            return None;
        }

        if false_weight > true_weight {
            Some(false)
        } else {
            Some(true)
        }
    }

    pub fn is_aggregated_value_entity(&self, val: &Assertion) -> bool {
        let aggregated = match self.aggregated_value() {
            None => return false,
            Some(val) => val,
        };

        if val.is_positive() && aggregated == true {
            return true;
        }
        if val.is_negative() && aggregated == false {
            return true;
        }

        false
    }

    // TODO: potentially broken
    pub fn is_aggregated_value(&self, val: &Proposition) -> bool {
        let aggregated = match self.aggregated_value() {
            None => return false,
            Some(val) => val,
        };
        let false_value_cid = self.values()[0].to_cid().unwrap().to_bytes();
        let true_value_cid = self.values()[1].to_cid().unwrap().to_bytes();

        if val.proposition_cid == false_value_cid && aggregated == false {
            return true;
        }
        if val.proposition_cid == true_value_cid && aggregated == true {
            return true;
        }

        false
    }
}

impl ::serde::Serialize for ValuedBooleanPropositionPool {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::Serializer,
    {
        #[derive(Serialize, Clone)]
        #[allow(non_snake_case)]
        struct AssertionWithWeight {
            #[serde(flatten)]
            pub assertion: FormatWeb3<Entity>,
            pub totalWeight: U256,
            pub isAggregatedValue: bool,
        }

        #[derive(Serialize)]
        #[allow(non_snake_case)]
        struct PropositionPoolSerialize<'a> {
            pub poolType: String,
            pub subject: HexString<'a>,
            pub subjectProperty: Vec<HexString<'a>>,
            pub values: Vec<AssertionWithWeight>,
            pub positiveValues: Vec<AssertionWithWeight>,
            pub negativeValues: Vec<AssertionWithWeight>,
            pub canonicalPositiveValue: AssertionWithWeight,
            pub canonicalNegativeValue: AssertionWithWeight,
            pub aggregatedValue: Option<AssertionWithWeight>,
            pub totalWeight: U256,
            pub totalWeightPositive: U256,
            pub totalWeightNegative: U256,
            pub totalWeightAggregationResult: Option<U256>,
        }

        let pool_type_entity: ontology::Entity = self.values().get(0).unwrap().clone().into();
        let pool_type_entity_kind: &str = pool_type_entity.kind().into();
        let pool_type: String = pool_type_entity_kind.replace("Assertion", "").to_owned();

        let add_weight = |assertion: Assertion| AssertionWithWeight {
            totalWeight: self.weights_for_value(&assertion),
            isAggregatedValue: self.is_aggregated_value_entity(&assertion),
            assertion: FormatWeb3(Into::<Entity>::into(assertion)),
        };

        let add_weights =
            |values: &[Assertion]| values.to_vec().into_iter().map(add_weight).collect();

        let canonical_positive_value = add_weight(self.pool.canonical_positive_value());
        let canonical_negative_value = add_weight(self.pool.canonical_negative_value());

        let mut aggregated_value: Option<AssertionWithWeight> = None;
        if canonical_positive_value.isAggregatedValue {
            aggregated_value = Some(canonical_positive_value.clone())
        } else if canonical_negative_value.isAggregatedValue {
            aggregated_value = Some(canonical_negative_value.clone())
        }

        let mut total_weight_aggregation_result: Option<U256> = None;
        if canonical_positive_value.isAggregatedValue {
            total_weight_aggregation_result = Some(self.positive_weights())
        } else if canonical_negative_value.isAggregatedValue {
            total_weight_aggregation_result = Some(self.negative_weights())
        }

        let subject_property = self
            .pool
            .subject_property()
            .into_iter()
            .map(|n| HexString::wrap(n))
            .collect();

        let ext = PropositionPoolSerialize {
            poolType: pool_type,
            subject: HexString::wrap(self.pool.subject()),
            subjectProperty: subject_property,
            values: add_weights(self.values()),
            positiveValues: add_weights(&self.pool.positive_values()),
            negativeValues: add_weights(&self.pool.negative_values()),
            canonicalPositiveValue: canonical_positive_value,
            canonicalNegativeValue: canonical_negative_value,
            aggregatedValue: aggregated_value,
            totalWeight: self.total_weight(),
            totalWeightPositive: self.positive_weights(),
            totalWeightNegative: self.negative_weights(),
            totalWeightAggregationResult: total_weight_aggregation_result,
        };

        Ok(ext.serialize(serializer)?)
    }
}

pub fn detect_pools(ontology_entities: &[&ontology::Entity]) -> Vec<BooleanPropositionPool> {
    let mut pools: HashMap<Vec<Vec<u8>>, BooleanPropositionPool> = HashMap::new();

    ontology_entities
        .iter()
        .filter_map(|entity| entity.as_assertion())
        .for_each(|assertion| {
            let entry = pools
                .entry(assertion.canonical_parts())
                .or_insert_with(BooleanPropositionPool::new);
            entry.try_insert(assertion);
        });

    pools.into_iter().map(|(_, val)| val).collect()
}

/// Constructs all the existent pools that arise from all used propositions.
///
/// Goes through all the individuals used in propositions and finds individuals that
/// assert or negatively assert class memberships about the same subject.
pub fn detect_valued_pools(
    ontology_entities: &[&ontology::Entity],
    propositions: &[&Proposition],
) -> Vec<ValuedBooleanPropositionPool> {
    let pools = detect_pools(ontology_entities);
    trace!("Built pools");
    let mut valued_pools: Vec<ValuedBooleanPropositionPool> = pools
        .into_iter()
        .map(ValuedBooleanPropositionPool::from_pool)
        .collect();
    trace!("Built valued pools");

    let original_valued_pool_arcs: Vec<_> = valued_pools
        .into_iter()
        .map(|n| Arc::new(Mutex::new(n)))
        .collect();
    let valued_pool_arcs = original_valued_pool_arcs.clone();
    {
        let mut pool_cids_map: HashMap<Vec<u8>, Arc<Mutex<ValuedBooleanPropositionPool>>> =
            HashMap::new();
        for pool_arc in valued_pool_arcs {
            let pool_cids = pool_arc.lock().unwrap().pool.value_cids();
            for pool_cid in pool_cids {
                pool_cids_map.insert(pool_cid, pool_arc.clone());
            }
        }

        for proposition in propositions {
            let mut pool_opt = pool_cids_map.get_mut(&proposition.proposition_cid);
            if let Some(ref mut pool) = pool_opt {
                pool.lock()
                    .unwrap()
                    .propositions
                    .push((*proposition).to_owned());
                continue;
            }
        }
    }
    valued_pools = original_valued_pool_arcs
        .into_iter()
        .map(|n| Arc::try_unwrap(n).unwrap().into_inner().unwrap())
        .collect();

    trace!("Added proposition to pools");
    valued_pools
}
