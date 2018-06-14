use ethabi::{self, Event};
use multibase::{encode as base_encode, Base};
use std::collections::{BTreeMap, HashMap};
use web3;
use tokio_core;
use web3::futures::{self, Future, IntoFuture, Stream};
use web3::types::{BlockNumber, Filter, FilterBuilder, Log};
use rlay_ontology::ontology::{self, Annotation};
use cid::ToCid;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use futures_timer::Interval;

use config::Config;

type EntityMap = BTreeMap<Vec<u8>, Entity>;

#[derive(Debug)]
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

fn is_stored_event(event_type: &str) -> bool {
    let stored_event_types = vec!["AnnotationStored", "Class", "IndividualStored"];

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

fn process_log(
    log: &web3::types::Log,
    signature_map: &HashMap<web3::types::H256, Event>,
    config: &Config,
    web3: &web3::Web3<web3::transports::WebSocket>,
) -> impl Future<Item = Option<Entity>, Error = ()> {
    println!("got log: {:?} - {:?}", log.transaction_hash, log.log_index);
    let event = &signature_map[&log.topics[0]];
    println!("EVENT TYPE: {:?}", event.name);

    if !is_stored_event(&event.name) {
        return futures::future::Either::B(Ok(None).into_future());
    }

    let cid = cid_from_log(log, event);
    let cid_base58 = base_encode(Base::Base58btc, &cid);
    println!("CID {:?}", cid_base58);
    let ontology_contract_abi = include_str!("../data/OntologyStorage.abi");
    let contract = web3::contract::Contract::from_json(
        web3.eth(),
        config.contract_address("OntologyStorage"),
        ontology_contract_abi.as_bytes(),
    ).unwrap();
    let fut = match event.name.as_ref() {
        "AnnotationStored" => futures::future::Either::A(
            contract
                .query(
                    "retrieveAnnotation",
                    (cid,),
                    None,
                    web3::contract::Options::default(),
                    None,
                )
                .and_then(|res| {
                    let (property, value): (Vec<u8>, String) = res;
                    Ok(Entity::Annotation(Annotation::new(&property, value)))
                })
                .and_then(|res| Ok(Some(res)))
                .map_err(|_| ()),
        ),
        _ => futures::future::Either::B(Ok(None).into_future()),
    };
    fut
}

/// Subscribe on a filter, but also get all historic logs that fit the filter
fn subscribe_with_history(
    web3: &web3::Web3<web3::transports::WebSocket>,
    filter: Filter,
) -> impl Stream<Item = Log> {
    let history_future_1 = web3.eth().logs(filter.clone());
    let subscribe_future_1 = web3.eth_subscribe().subscribe_logs(filter);

    let combined_future = history_future_1
        .join(subscribe_future_1)
        .into_stream()
        .and_then(|(history, subscribe_stream)| {
            let history_stream = futures::stream::iter_ok(history);

            Ok(Stream::chain(history_stream, subscribe_stream))
        })
        .flatten();

    combined_future
}

/// Returns a Future that when polled will sync all entities from the blockchain into the provided
/// map.
#[cfg_attr(feature = "cargo-clippy", allow(needless_pass_by_value))]
pub fn sync_future(
    eloop_handle: tokio_core::reactor::Handle,
    config: Config,
    entity_map_mutex: Arc<Mutex<EntityMap>>,
) -> impl Future<Item = (), Error = ()> {
    let web3 = web3::Web3::new(
        web3::transports::WebSocket::with_event_loop(
            config.network_address.as_ref().unwrap(),
            &eloop_handle,
        ).unwrap(),
    );

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
        .and_then(move |log| process_log(&log, &signature_map, &config, &web3).into_future())
        .filter(|res| res.is_some())
        .map(|res| res.unwrap())
        .for_each(move |entity: Entity| {
            let mut entity_map_lock = entity_map_mutex.lock().unwrap();
            entity_map_lock.insert(entity.to_bytes(), entity);
            Ok(())
        })
        .map_err(|_| ())
}

pub fn run_sync(config: &Config) {
    let mut eloop = tokio_core::reactor::Core::new().unwrap();

    let entity_map: BTreeMap<Vec<u8>, Entity> = BTreeMap::new();
    let entity_map_mutex = Arc::new(Mutex::new(entity_map));

    let final_future = sync_future(eloop.handle(), config.clone(), entity_map_mutex.clone());
    let counter_stream = Interval::new(Duration::from_secs(5))
        .for_each(|_| {
            let entity_map_lock = entity_map_mutex.lock().unwrap();
            println!("Num entities: {}", entity_map_lock.len());

            Ok(())
        })
        .map_err(|_| ());

    eloop.run(final_future.join(counter_stream)).unwrap();
}
