use ethabi::{self, Event};
use multibase::{encode as base_encode, Base};
use std::collections::HashMap;
use web3;
use tokio_core;
use web3::futures::{self, Future, Stream};
use web3::types::{BlockNumber, Filter, FilterBuilder, Log};

use config::Config;

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

fn process_log(log: &web3::types::Log, signature_map: &HashMap<web3::types::H256, Event>) {
    println!("got log: {:?} - {:?}", log.transaction_hash, log.log_index);
    let event = &signature_map[&log.topics[0]];
    println!("EVENT TYPE: {:?}", event.name);

    if !is_stored_event(&event.name) {
        return;
    }

    let cid = cid_from_log(log, event);
    let cid_base58 = base_encode(Base::Base58btc, &cid);
    println!("CID {:?}", cid_base58);
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

pub fn run_sync(config: &Config) {
    let mut eloop = tokio_core::reactor::Core::new().unwrap();
    let web3 = web3::Web3::new(
        web3::transports::WebSocket::with_event_loop(
            config.network_address.as_ref().unwrap(),
            &eloop.handle(),
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
    // Filter for Hello event in our contract
    let filter = FilterBuilder::default()
        .from_block(BlockNumber::Earliest)
        .address(vec![ontology_contract_address_hash])
        .build();

    let combined_stream = subscribe_with_history(&web3, filter);

    let final_future = combined_stream
        .for_each(move |log| {
            process_log(&log, &signature_map);
            Ok(())
        })
        .map_err(|_| ());

    eloop.run(final_future).unwrap();
}
