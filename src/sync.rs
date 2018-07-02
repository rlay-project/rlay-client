use futures_timer::Interval;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_core;
use web3::futures::{self, prelude::*};
use web3::types::{Filter, Log};
use web3;
use rustc_hex::ToHex;

use config::Config;
use sync_ontology::{sync_ontology, Entity};
use sync_proposition_ledger::{sync_ledger, PropositionLedger};
use payout::{fill_epoch_payouts, submit_epoch_payouts, Payout, PayoutEpochs};

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
    let payout_epochs: PayoutEpochs = HashMap::new();
    let payout_epochs_mutex = Arc::new(Mutex::new(payout_epochs));

    let sync_ontology_fut = sync_ontology(eloop.handle(), config.clone(), entity_map_mutex.clone());
    let sync_proposition_ledger_fut = sync_ledger(
        eloop.handle(),
        config.clone(),
        proposition_ledger_mutex.clone(),
        proposition_ledger_block_highwatermark_mutex.clone(),
    );
    let calculate_payouts_fut = Interval::new(Duration::from_secs(5))
        .for_each(|_| {
            fill_epoch_payouts(
                &proposition_ledger_block_highwatermark_mutex.clone(),
                &proposition_ledger_mutex.clone(),
                &payout_epochs_mutex.clone(),
            );
            Ok(())
        })
        .map_err(|_| ());
    let counter_stream = Interval::new(Duration::from_secs(5))
        .for_each(|_| {
            let entity_map_lock = entity_map_mutex.lock().unwrap();
            let ledger_lock = proposition_ledger_mutex.lock().unwrap();
            let payout_epochs = payout_epochs_mutex.lock().unwrap();
            info!("Num entities: {}", entity_map_lock.len());
            info!("Num propositions: {}", ledger_lock.len());

            for (epoch, payouts) in payout_epochs.iter() {
                trace!("Payouts for epoch {}: {:?}", epoch, payouts);
                if payouts.len() <= 1 {
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

    let submit_handle = eloop.handle().clone();
    let submit_payout_epochs_mutex = payout_epochs_mutex.clone();
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
