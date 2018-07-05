use ethabi::{self, Event};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio_core;
use web3::futures::prelude::*;
use web3::types::{Address, BlockNumber, FilterBuilder, Log, U256};
use web3;

use config::Config;
use sync::subscribe_with_history;

// TODO: reevaluate Hash, ParitialEq and Eq derives as there could theoretically be collisions.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Proposition {
    pub proposition_cid: Vec<u8>,
    pub amount: U256,
    pub sender: Address,
    pub block_number: u64,
}

impl Proposition {
    pub fn from_log(log: &Log, event: &Event) -> Self {
        let raw_log = ethabi::RawLog {
            topics: log.topics.to_owned(),
            data: log.data.0.to_owned(),
        };
        let parsed_log = event.parse_log(raw_log).unwrap();

        let proposition_cid_bytes = parsed_log.params[0].value.clone();
        let proposition_cid = proposition_cid_bytes.to_bytes().to_owned().unwrap();

        let amount_raw = parsed_log.params[1].value.clone();
        let amount = amount_raw.to_uint().to_owned().unwrap();

        let sender_raw = parsed_log.params[2].value.clone();
        let sender = sender_raw.to_address().to_owned().unwrap();

        let block_number = log.block_number.unwrap().as_u64();

        Self {
            proposition_cid,
            amount,
            sender,
            block_number,
        }
    }
}

pub type PropositionLedger = Vec<Proposition>;

#[cfg_attr(feature = "cargo-clippy", allow(needless_pass_by_value))]
pub fn sync_ledger(
    eloop_handle: tokio_core::reactor::Handle,
    config: Config,
    proposition_ledger_mutex: Arc<Mutex<PropositionLedger>>,
    ledger_block_highwatermark_mtx: Arc<Mutex<u64>>,
) -> impl Future<Item = (), Error = ()> {
    let web3 = web3::Web3::new(
        web3::transports::WebSocket::with_event_loop(
            config.network_address.as_ref().unwrap(),
            &eloop_handle,
        ).unwrap(),
    );

    let ledger_contract_abi = include_str!("../data/PropositionLedger.abi");
    let contract = ethabi::Contract::load(ledger_contract_abi.as_bytes()).unwrap();

    let signature_map: HashMap<web3::types::H256, Event> = contract
        .events
        .values()
        .cloned()
        .map(|event| (event.signature(), event))
        .collect();

    let ledger_contract_address_hash = config.contract_address("PropositionLedger");

    let filter = FilterBuilder::default()
        .from_block(BlockNumber::Earliest)
        .address(vec![ledger_contract_address_hash])
        .build();

    let combined_stream = subscribe_with_history(&web3, filter);

    combined_stream
        .map_err(|_| ())
        .and_then(move |log| process_ledger_log(&log, &signature_map).into_future())
        .filter(|res| res.is_some())
        .map(|res| res.unwrap())
        .for_each(move |proposition: Proposition| {
            let mut proposition_ledger_lock = proposition_ledger_mutex.lock().unwrap();
            let mut ledger_block_highwatermark = ledger_block_highwatermark_mtx.lock().unwrap();
            debug!("New proposition: {:?}", &proposition);
            *ledger_block_highwatermark = proposition.block_number.clone();
            proposition_ledger_lock.push(proposition);
            Ok(())
        })
        .map_err(|_| ())
}

fn process_ledger_log(
    log: &web3::types::Log,
    signature_map: &HashMap<web3::types::H256, Event>,
) -> impl Future<Item = Option<Proposition>, Error = ()> {
    debug!(
        "got PropositionLedger log: {:?} - {:?}",
        log.transaction_hash, log.log_index
    );
    let event = &signature_map[&log.topics[0]];

    if !is_relevant_event(&event.name) {
        return Ok(None).into_future();
    }

    let proposition = Proposition::from_log(log, &event);
    Ok(Some(proposition)).into_future()
}

fn is_relevant_event(event_type: &str) -> bool {
    let relevant_event_types = vec!["PropositionWeightIncreased"];

    relevant_event_types.contains(&event_type)
}
