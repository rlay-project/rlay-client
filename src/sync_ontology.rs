use cid::ToCid;
use ethabi::{self, Event};
use multibase::{encode as base_encode, Base};
use rlay_ontology::ontology::{self, *};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use tokio_core;
use web3::futures::{future::Either, prelude::*};
use web3::types::{BlockNumber, FilterBuilder};
use web3::Transport;
use web3;

use config::Config;
use sync::subscribe_with_history;
use web3_helpers::{decode_class_call_output, decode_individual_call_output, raw_query};

pub type EntityMap = BTreeMap<Vec<u8>, Entity>;

pub fn entity_map_individuals(entity_map: &EntityMap) -> Vec<&ontology::Individual> {
    entity_map
        .values()
        .filter_map(|entity| match entity {
            Entity::Annotation(_) => None,
            Entity::Class(_) => None,
            Entity::Individual(val) => Some(val),
        })
        .collect()
}

// TODO: refactor into rlay-ontology
#[derive(Debug, Clone, PartialEq)]
pub enum Entity {
    Annotation(ontology::Annotation),
    Class(ontology::Class),
    Individual(ontology::Individual),
}

impl Entity {
    pub fn to_bytes(&self) -> Vec<u8> {
        match &self {
            Entity::Annotation(ent) => ent.to_cid().unwrap().to_bytes(),
            Entity::Class(ent) => ent.to_cid().unwrap().to_bytes(),
            Entity::Individual(ent) => ent.to_cid().unwrap().to_bytes(),
        }
    }
}

/// Returns a Future that when polled will sync all entities from the blockchain into the provided
/// map.
#[cfg_attr(feature = "cargo-clippy", allow(needless_pass_by_value))]
pub fn sync_ontology(
    eloop_handle: tokio_core::reactor::Handle,
    config: Config,
    entity_map_mutex: Arc<Mutex<EntityMap>>,
) -> impl Future<Item = (), Error = ()> {
    let web3 = config.web3_with_handle(&eloop_handle);

    let ontology_contract_abi = include_str!("../data/OntologyStorage.abi");
    let contract = ethabi::Contract::load(ontology_contract_abi.as_bytes()).unwrap();

    let signature_map: HashMap<web3::types::H256, Event> = contract
        .events
        .values()
        .cloned()
        .map(|event| (event.signature(), event))
        .collect();

    let ontology_contract_address_hash = config.contract_address("OntologyStorage");

    let filter = FilterBuilder::default()
        .from_block(BlockNumber::Earliest)
        .address(vec![ontology_contract_address_hash])
        .build();

    let combined_stream = subscribe_with_history(&web3, filter);

    combined_stream
        .map_err(|_| ())
        .and_then(move |log| {
            process_ontology_storage_log(&log, &signature_map, &config, &web3).into_future()
        })
        .filter(|res| res.is_some())
        .map(|res| res.unwrap())
        .for_each(move |entity: Entity| {
            let mut entity_map_lock = entity_map_mutex.lock().unwrap();
            entity_map_lock.insert(entity.to_bytes(), entity);

            Ok(())
        })
        .map_err(|_| ())
}

fn is_stored_event(event_type: &str) -> bool {
    let stored_event_types = vec!["AnnotationStored", "ClassStored", "IndividualStored"];

    stored_event_types.contains(&event_type)
}

fn cid_from_log(log: &web3::types::Log, event: &Event) -> Vec<u8> {
    let raw_log = ethabi::RawLog {
        topics: log.topics.to_owned(),
        data: log.data.0.to_owned(),
    };
    let parsed_log = event.parse_log(raw_log).unwrap();
    let cid_bytes = parsed_log.params[0].value.clone();
    let cid = cid_bytes.to_bytes().to_owned().unwrap();

    cid
}

fn process_ontology_storage_log(
    log: &web3::types::Log,
    signature_map: &HashMap<web3::types::H256, Event>,
    config: &Config,
    web3: &web3::Web3<impl Transport>,
) -> impl Future<Item = Option<Entity>, Error = ()> {
    debug!(
        "got OntologyStorage log: {:?} - {:?}",
        log.transaction_hash, log.log_index
    );
    let event = &signature_map[&log.topics[0]];
    debug!("EVENT TYPE: {:?}", event.name);

    if !is_stored_event(&event.name) {
        return Either::B(Ok(None).into_future());
    }

    let cid = cid_from_log(log, event);
    let cid_base58 = base_encode(Base::Base58btc, &cid);
    debug!("CID {:?}", cid_base58);
    let ontology_contract_abi = include_str!("../data/OntologyStorage.abi");
    let abi = ethabi::Contract::load(ontology_contract_abi.as_bytes()).unwrap();
    let contract = web3::contract::Contract::from_json(
        web3.eth(),
        config.contract_address("OntologyStorage"),
        ontology_contract_abi.as_bytes(),
    ).unwrap();

    Either::A(get_entity_for_log(
        web3,
        &abi,
        &contract,
        event.name.as_ref(),
        &cid,
    ))
}

fn get_entity_for_log(
    web3: &web3::Web3<impl Transport>,
    abi: &ethabi::Contract,
    contract: &web3::contract::Contract<impl Transport>,
    event_name: &str,
    cid: &[u8],
) -> impl Future<Item = Option<Entity>, Error = ()> {
    let annotation_fut_cid = cid.to_owned();
    let class_fut_cid = cid.to_owned();
    let individual_fut_cid = cid.to_owned();

    let annotation_fut = match event_name {
        "AnnotationStored" => Either::A(
            contract
                .query(
                    "retrieveAnnotation",
                    (cid.to_owned(),),
                    None,
                    web3::contract::Options::default(),
                    None,
                )
                .and_then(move |res| {
                    let (property, value): (Vec<u8>, String) = res;
                    let ent = Annotation::new(&property, value);
                    debug_assert!(
                        ent.to_cid().unwrap().to_bytes() == annotation_fut_cid,
                        "CID of retrieved Entity was not correct"
                    );
                    Ok(Entity::Annotation(ent))
                })
                .and_then(|res| Ok(Some(res)))
                .map_err(|_| ()),
        ),
        _ => Either::B(Ok(None).into_future()),
    };
    let class_fut = match event_name {
        "ClassStored" => Either::A(
            raw_query(
                web3.eth(),
                abi,
                contract.address(),
                "retrieveClass",
                (cid.to_owned(),),
                None,
                web3::contract::Options::default(),
                None,
            ).and_then(move |res| {
                let (annotations, sub_class_of_class) = decode_class_call_output(&res.0);
                let ent = Class {
                    annotations,
                    sub_class_of_class,
                };

                debug_assert_eq!(
                    ent.to_cid().unwrap().to_bytes(),
                    class_fut_cid,
                    "CID of retrieved Entity was not correct"
                );

                Ok(Entity::Class(ent))
            })
                .and_then(|res| Ok(Some(res)))
                .map_err(|_| ()),
        ),
        _ => Either::B(Ok(None).into_future()),
    };
    let individual_fut = match event_name {
        "IndividualStored" => Either::A(
            raw_query(
                web3.eth(),
                abi,
                contract.address(),
                "retrieveIndividual",
                (cid.to_owned(),),
                None,
                web3::contract::Options::default(),
                None,
            ).and_then(move |res| {
                let (annotations, class_assertions, negative_class_assertions) =
                    decode_individual_call_output(&res.0);
                let ent = Individual {
                    annotations,
                    class_assertions,
                    negative_class_assertions,
                };

                debug_assert_eq!(
                    ent.to_cid().unwrap().to_bytes(),
                    individual_fut_cid,
                    "CID of retrieved Entity was not correct"
                );
                Ok(Entity::Individual(ent))
            })
                .and_then(|res| Ok(Some(res)))
                .map_err(|_| ()),
        ),
        _ => Either::B(Ok(None).into_future()),
    };

    annotation_fut
        .join3(class_fut, individual_fut)
        .and_then(|results| {
            Ok(match results {
                (Some(entity), _, _) => Some(entity),
                (_, Some(entity), _) => Some(entity),
                (_, _, Some(entity)) => Some(entity),
                (None, None, None) => None,
            })
        })
}
