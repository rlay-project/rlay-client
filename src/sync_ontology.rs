use cid::ToCid;
use rustc_hex::ToHex;
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
use web3_helpers::{raw_query, FromABIV2Response};

pub type EntityMap = BTreeMap<Vec<u8>, EntityKind>;
pub type CidEntityKindMap = BTreeMap<Vec<u8>, String>;

pub fn entity_map_individuals(entity_map: &EntityMap) -> Vec<&ontology::Individual> {
    entity_map
        .values()
        .filter_map(|entity| match entity {
            EntityKind::Individual(val) => Some(val),
            _ => None,
        })
        .collect()
}

pub fn entity_map_class_assertions(entity_map: &EntityMap) -> Vec<&ontology::ClassAssertion> {
    entity_map
        .values()
        .filter_map(|entity| match entity {
            EntityKind::ClassAssertion(val) => Some(val),
            _ => None,
        })
        .collect()
}

pub fn entity_map_negative_class_assertions(
    entity_map: &EntityMap,
) -> Vec<&ontology::NegativeClassAssertion> {
    entity_map
        .values()
        .filter_map(|entity| match entity {
            EntityKind::NegativeClassAssertion(val) => Some(val),
            _ => None,
        })
        .collect()
}

/// Returns a Future that when polled will sync all entities from the blockchain into the provided
/// map.
#[cfg_attr(feature = "cargo-clippy", allow(needless_pass_by_value))]
pub fn sync_ontology(
    eloop_handle: tokio_core::reactor::Handle,
    config: Config,
    entity_map_mutex: Arc<Mutex<EntityMap>>,
    cid_entity_kind_map_mutex: Arc<Mutex<CidEntityKindMap>>,
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
        .and_then(move |(cid, event_name, entity): (Vec<u8>, String, _)| {
            let mut cid_entity_kind_map_lock = cid_entity_kind_map_mutex.lock().unwrap();
            let kind_name = str::replace(&event_name, "Stored", "").to_owned();
            cid_entity_kind_map_lock.insert(cid, kind_name);

            Ok(entity)
        })
        .filter(|res| res.is_some())
        .map(|res| res.unwrap())
        .for_each(move |entity: EntityKind| {
            let mut entity_map_lock = entity_map_mutex.lock().unwrap();
            entity_map_lock.insert(entity.to_bytes(), entity);

            Ok(())
        })
        .map_err(|_| ())
}

fn is_stored_event(event_type: &str) -> bool {
    event_type.ends_with("Stored")
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
) -> impl Future<Item = Option<(Vec<u8>, String, Option<EntityKind>)>, Error = ()> {
    debug!(
        "got OntologyStorage log: {:?} - {:?}",
        log.transaction_hash, log.log_index
    );
    let event = &signature_map[&log.topics[0]];
    let event_name = event.name.to_owned();
    debug!("EVENT TYPE: {:?}", event.name);

    if !is_stored_event(&event.name) {
        return Either::B(Ok(None).into_future());
    }

    let cid = cid_from_log(log, event);
    let cid_base58 = base_encode(Base::Base58btc, &cid);
    debug!("CID {:?}", cid_base58);
    debug!("CID(hex) 0x{}", cid.to_hex());
    let ontology_contract_abi = include_str!("../data/OntologyStorage.abi");
    let abi = ethabi::Contract::load(ontology_contract_abi.as_bytes()).unwrap();
    let contract = web3::contract::Contract::from_json(
        web3.eth(),
        config.contract_address("OntologyStorage"),
        ontology_contract_abi.as_bytes(),
    ).unwrap();

    Either::A(
        get_entity_for_log(web3, &abi, &contract, event.name.as_ref(), &cid)
            .and_then(|entity| Ok(Some((cid, event_name, entity)))),
    )
}

fn get_entity_kind_for_log<K>(
    web3: &web3::Web3<impl Transport>,
    abi: &ethabi::Contract,
    contract: &web3::contract::Contract<impl Transport>,
    cid: Vec<u8>,
    event_name: &str,
    kind_event_name: &str,
    kind_retrieve_name: &str,
) -> impl Future<Item = Option<K>, Error = ()>
where
    K: FromABIV2Response + ToCid + Into<EntityKind>,
{
    if event_name == kind_event_name {
        Either::A(
            raw_query(
                web3.eth(),
                abi,
                contract.address(),
                kind_retrieve_name,
                (cid.to_owned(),),
                None,
                web3::contract::Options::default(),
                None,
            ).and_then(move |res| {
                let ent = K::from_abiv2(&res.0);
                let retrieved_cid = ent.to_cid().unwrap().to_bytes();
                debug!("CID(retrieved) 0x{}", retrieved_cid.to_hex());
                debug_assert!(
                    retrieved_cid == cid,
                    "CID of retrieved Entity was not correct"
                );
                Ok(ent)
            })
                .and_then(|res| Ok(Some(res)))
                .map_err(|_| ()),
        )
    } else {
        Either::B(Ok(None).into_future())
    }
}

fn get_entity_for_log(
    web3: &web3::Web3<impl Transport>,
    abi: &ethabi::Contract,
    contract: &web3::contract::Contract<impl Transport>,
    event_name: &str,
    cid: &[u8],
) -> impl Future<Item = Option<EntityKind>, Error = ()> {
    let annotation_fut = get_entity_kind_for_log(
        web3,
        abi,
        contract,
        cid.to_owned(),
        event_name,
        "AnnotationStored",
        "retrieveAnnotation",
    ).and_then(|n| Ok(n.map(<Annotation as Into<EntityKind>>::into)));
    let class_fut = get_entity_kind_for_log(
        web3,
        abi,
        contract,
        cid.to_owned(),
        event_name,
        "ClassStored",
        "retrieveClass",
    ).and_then(|n| Ok(n.map(<Class as Into<EntityKind>>::into)));
    let individual_fut = get_entity_kind_for_log(
        web3,
        abi,
        contract,
        cid.to_owned(),
        event_name,
        "IndividualStored",
        "retrieveIndividual",
    ).and_then(|n| Ok(n.map(<Individual as Into<EntityKind>>::into)));
    let classassertion_fut = get_entity_kind_for_log(
        web3,
        abi,
        contract,
        cid.to_owned(),
        event_name,
        "ClassAssertionStored",
        "retrieveClassAssertion",
    ).and_then(|n| Ok(n.map(<ClassAssertion as Into<EntityKind>>::into)));
    let negativeclassassertion_fut =
        get_entity_kind_for_log(
            web3,
            abi,
            contract,
            cid.to_owned(),
            event_name,
            "NegativeClassAssertionStored",
            "retrieveNegativeClassAssertion",
        ).and_then(|n| Ok(n.map(<NegativeClassAssertion as Into<EntityKind>>::into)));

    Future::join5(
        annotation_fut,
        class_fut,
        individual_fut,
        classassertion_fut,
        negativeclassassertion_fut,
    ).and_then(|results| {
        Ok(match results {
            (Some(entity), _, _, _, _) => Some(entity),
            (_, Some(entity), _, _, _) => Some(entity),
            (_, _, Some(entity), _, _) => Some(entity),
            (_, _, _, Some(entity), _) => Some(entity),
            (_, _, _, _, Some(entity)) => Some(entity),
            (None, None, None, None, None) => None,
        })
    })
}
