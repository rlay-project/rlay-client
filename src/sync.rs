use futures_timer::Interval;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_core;
use web3::futures::{self, prelude::*};
use web3::types::{Filter, Log};
use web3;

use config::Config;
use sync_ontology::{sync_ontology, Entity};
use sync_proposition_ledger::{sync_ledger, PropositionLedger};

// TODO: possibly contribute to rust-web3
/// Subscribe on a filter, but also get all historic logs that fit the filter
pub fn subscribe_with_history(
    web3: &web3::Web3<web3::transports::WebSocket>,
    filter: Filter,
) -> impl Stream<Item = Log> {
    let history_future = web3.eth().logs(filter.clone());
    let subscribe_future = web3.eth_subscribe().subscribe_logs(filter);

    let combined_future = history_future
        .join(subscribe_future)
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

    let entity_map: BTreeMap<Vec<u8>, Entity> = BTreeMap::new();
    let entity_map_mutex = Arc::new(Mutex::new(entity_map));
    let proposition_ledger: PropositionLedger = vec![];
    let proposition_ledger_mutex = Arc::new(Mutex::new(proposition_ledger));

    let sync_ontology_fut = sync_ontology(eloop.handle(), config.clone(), entity_map_mutex.clone());
    let sync_proposition_ledger_fut = sync_ledger(
        eloop.handle(),
        config.clone(),
        proposition_ledger_mutex.clone(),
    );
    let counter_stream = Interval::new(Duration::from_secs(5))
        .for_each(|_| {
            let entity_map_lock = entity_map_mutex.lock().unwrap();
            let ledger_lock = proposition_ledger_mutex.lock().unwrap();
            info!("Num entities: {}", entity_map_lock.len());
            info!("Num propositions: {}", ledger_lock.len());

            Ok(())
        })
        .map_err(|_| ());

    eloop
        .run(sync_ontology_fut.join3(sync_proposition_ledger_fut, counter_stream))
        .unwrap();
}
