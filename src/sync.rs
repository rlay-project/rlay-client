use futures_timer::Interval;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_core;
use web3::futures::{self, prelude::*};
use web3::types::{Filter, Log, U256};
use web3;
use rustc_hex::ToHex;

use config::Config;
use sync_ontology::{sync_ontology, Entity};
use sync_proposition_ledger::{sync_ledger, PropositionLedger};
use payout::{fill_epoch_payouts, fill_epoch_payouts_cumulative, load_epoch_payouts,
             retrieve_epoch_start_block, submit_epoch_payouts, Payout, PayoutEpochs, EPOCH_LENGTH};

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
    let proposition_ledger_block_highwatermark_mutex = Arc::new(Mutex::new(0u64));

    let mut payout_epochs: PayoutEpochs = HashMap::new();
    // Load state from storage
    load_epoch_payouts(config.clone(), &mut payout_epochs);
    let payout_epochs_mutex = Arc::new(Mutex::new(payout_epochs));
    // Cummulative epoch payouts
    let payout_epochs_cum: PayoutEpochs = HashMap::new();
    let payout_epochs_cum_mutex = Arc::new(Mutex::new(payout_epochs_cum));

    // Sync ontology concepts from smart contract to local state
    let sync_ontology_fut = sync_ontology(eloop.handle(), config.clone(), entity_map_mutex.clone());
    // Sync proposition ledger from smart contract to local state
    let sync_proposition_ledger_fut = sync_ledger(
        eloop.handle(),
        config.clone(),
        proposition_ledger_mutex.clone(),
        proposition_ledger_block_highwatermark_mutex.clone(),
    );
    // Calculate the payouts based on proposition ledger
    let epoch_length: U256 = EPOCH_LENGTH.into();
    let calculate_payouts_fut = retrieve_epoch_start_block(&eloop.handle().clone(), config)
        .and_then(|epoch_start_block| {
            Interval::new(Duration::from_secs(15))
                .and_then(move |_| Ok(epoch_start_block))
                .for_each(|epoch_start_block| {
                    fill_epoch_payouts(
                        epoch_start_block,
                        epoch_length,
                        &proposition_ledger_block_highwatermark_mutex.clone(),
                        &proposition_ledger_mutex.clone(),
                        &payout_epochs_mutex.clone(),
                        &entity_map_mutex.clone(),
                    );
                    fill_epoch_payouts_cumulative(
                        &payout_epochs_mutex.clone(),
                        &payout_epochs_cum_mutex.clone(),
                    );
                    Ok(())
                })
                .map_err(|_| ())
        });
    // Print some statistics about the local state
    let counter_stream = Interval::new(Duration::from_secs(5))
        .for_each(|_| {
            let entity_map_lock = entity_map_mutex.lock().unwrap();
            let ledger_lock = proposition_ledger_mutex.lock().unwrap();
            let payout_epochs = payout_epochs_cum_mutex.lock().unwrap();
            debug!("Num entities: {}", entity_map_lock.len());
            let mut annotation_count = 0;
            let mut class_count = 0;
            let mut individual_count = 0;
            for entity in entity_map_lock.values() {
                match entity {
                    Entity::Annotation(_) => annotation_count += 1,
                    Entity::Class(_) => class_count += 1,
                    Entity::Individual(_) => individual_count += 1,
                }
            }
            debug!("--- Num annotation: {}", annotation_count);
            debug!("--- Num class: {}", class_count);
            debug!("--- Num individual: {}", individual_count);
            debug!("Num propositions: {}", ledger_lock.len());

            for (epoch, payouts) in payout_epochs.iter() {
                trace!("Payouts for epoch {}: {:?}", epoch, payouts);
                if payouts.len() <= 0 {
                    trace!("Not enough payouts to build payout tree");
                    continue;
                }
                let tree = Payout::build_merkle_tree(payouts);
                debug!(
                    "submitPayoutRoot({}, \"0x{}\")",
                    epoch,
                    tree.root().to_hex()
                );
                for payout in payouts {
                    let proof_str = ::payout::format_redeem_payout_call(*epoch, &tree, payout);
                    debug!("Payout for 0x{}: {}", payout.address.to_hex(), proof_str);
                }
            }

            Ok(())
        })
        .map_err(|_| ());

    // Submit calculated payout roots to smart contract
    let submit_handle = eloop.handle().clone();
    let submit_payout_epochs_mutex = payout_epochs_mutex.clone();
    let submit_payout_epochs_cum_mutex = payout_epochs_cum_mutex.clone();
    let submit_payouts = Interval::new(Duration::from_secs(5))
        .map_err(|err| {
            error!("{:?}", err);
            ()
        })
        .for_each(move |_| {
            submit_epoch_payouts(
                &submit_handle,
                config.clone(),
                submit_payout_epochs_mutex.clone(),
                submit_payout_epochs_cum_mutex.clone(),
            ).map(|_| ())
                .map_err(|err| {
                    error!("{:?}", err);
                    ()
                })
        })
        .map_err(|err| {
            error!("{:?}", err);
            ()
        });

    eloop
        .run(sync_ontology_fut.join5(
            sync_proposition_ledger_fut,
            calculate_payouts_fut,
            counter_stream,
            submit_payouts,
        ))
        .unwrap();
}
