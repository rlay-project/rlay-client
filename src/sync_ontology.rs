use cid::ToCid;
use rustc_hex::ToHex;
use ethabi::{self, Event};
use multibase::{encode as base_encode, Base};
use rlay_ontology::ontology::{self, *, FromABIV2ResponseHinted};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use tokio_core;
use web3::futures::{future::Either, prelude::*};
use web3::types::{BlockNumber, FilterBuilder};
use web3::Transport;
use web3;

use config::Config;
use sync::subscribe_with_history;
use web3_helpers::raw_query;

pub type EntityMap = BTreeMap<Vec<u8>, Entity>;
pub type CidEntityMap = BTreeMap<Vec<u8>, String>;

pub fn entity_map_individuals(entity_map: &EntityMap) -> Vec<&ontology::Individual> {
    entity_map
        .values()
        .filter_map(|entity| match entity {
            Entity::Individual(val) => Some(val),
            _ => None,
        })
        .collect()
}

pub fn entity_map_class_assertions(entity_map: &EntityMap) -> Vec<&ontology::ClassAssertion> {
    entity_map
        .values()
        .filter_map(|entity| match entity {
            Entity::ClassAssertion(val) => Some(val),
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
            Entity::NegativeClassAssertion(val) => Some(val),
            _ => None,
        })
        .collect()
}

pub trait OntologySyncer<P: Future<Item = (), Error = ()>> {
    type Config;

    /// Returns a Future that when polled will sync all entities from the blockchain into the provided
    /// map.
    #[cfg_attr(feature = "cargo-clippy", allow(needless_pass_by_value))]
    fn sync_ontology(
        &mut self,
        eloop_handle: tokio_core::reactor::Handle,
        config: Self::Config,
        entity_map_mutex: Arc<Mutex<EntityMap>>,
        cid_entity_kind_map_mutex: Arc<Mutex<CidEntityMap>>,
        last_synced_block_mutex: Arc<Mutex<Option<u64>>>,
    ) -> P;
}

pub struct EthOntologySyncer;

impl EthOntologySyncer {
    pub fn new() -> Self {
        Self {}
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
    ) -> impl Future<Item = Option<(Vec<u8>, String, Option<Entity>)>, Error = ()> {
        debug!(
            "got OntologyStorage log: {:?} - {:?}",
            log.transaction_hash, log.log_index
        );
        let event = &signature_map[&log.topics[0]];
        let event_name = event.name.to_owned();
        debug!("EVENT TYPE: {:?}", event.name);

        if !Self::is_stored_event(&event.name) {
            return Either::B(Ok(None).into_future());
        }

        let cid = Self::cid_from_log(log, event);
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
            Self::get_entity_for_log(web3, &abi, &contract, event.name.as_ref(), &cid)
                .and_then(|entity| Ok(Some((cid, event_name, entity)))),
        )
    }

    fn get_entity_kind_for_log(
        web3: &web3::Web3<impl Transport>,
        abi: &ethabi::Contract,
        contract: &web3::contract::Contract<impl Transport>,
        cid: Vec<u8>,
        kind: EntityKind,
    ) -> impl Future<Item = Entity, Error = ()> {
        raw_query(
            web3.eth(),
            abi,
            contract.address(),
            &kind.retrieve_fn_name(),
            (cid.to_owned(),),
            None,
            web3::contract::Options::default(),
            None,
        ).and_then(move |res| {
            let ent: Entity = FromABIV2ResponseHinted::from_abiv2(&res.0, &kind);
            let retrieved_cid = ent.to_cid().unwrap().to_bytes();
            debug!("CID(retrieved) 0x{}", retrieved_cid.to_hex());
            debug_assert!(
                retrieved_cid == cid,
                "CID of retrieved Entity was not correct"
            );
            Ok(ent)
        })
            .map_err(|_| ())
    }

    fn get_entity_for_log(
        web3: &web3::Web3<impl Transport>,
        abi: &ethabi::Contract,
        contract: &web3::contract::Contract<impl Transport>,
        event_name: &str,
        cid: &[u8],
    ) -> impl Future<Item = Option<Entity>, Error = ()> {
        match EntityKind::from_event_name(event_name) {
            Ok(kind) => Either::A(
                Self::get_entity_kind_for_log(web3, abi, contract, cid.to_owned(), kind)
                    .and_then(|entity| Ok(Some(entity))),
            ),
            Err(_) => Either::B(Ok(None).into_future()),
        }
    }
}

impl OntologySyncer<Box<Future<Item = (), Error = ()>>> for EthOntologySyncer {
    type Config = Config;

    fn sync_ontology(
        &mut self,
        eloop_handle: tokio_core::reactor::Handle,
        config: Self::Config,
        entity_map_mutex: Arc<Mutex<EntityMap>>,
        cid_entity_kind_map_mutex: Arc<Mutex<CidEntityMap>>,
        last_synced_block_mutex: Arc<Mutex<Option<u64>>>,
    ) -> Box<Future<Item = (), Error = ()>> {
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

        Box::new(
            combined_stream
                .map_err(|_| ())
                .and_then(move |log| {
                    let mut last_synced_block = last_synced_block_mutex.lock().unwrap();
                    *last_synced_block = log.block_number.map(|n| n.as_u64());
                    trace!("Ontology sync block {:?}", &log.block_number);
                    Self::process_ontology_storage_log(&log, &signature_map, &config, &web3)
                        .into_future()
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
                .for_each(move |entity: Entity| {
                    let mut entity_map_lock = entity_map_mutex.lock().unwrap();
                    entity_map_lock.insert(entity.to_bytes(), entity);

                    Ok(())
                })
                .map_err(|_| ()),
        ) as Box<Future<Item = (), Error = ()>>
    }
}
