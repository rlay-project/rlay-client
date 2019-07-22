use ethabi::{self, Event};
use failure::SyncFailure;
use futures01::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio_core;
use web3;
use web3::types::{Address, BlockNumber, FilterBuilder, Log, U256};

use crate::config::EthereumBackendConfig;
use crate::web3_helpers::subscribe_with_history;

// TODO: reevaluate Hash, ParitialEq and Eq derives as there could theoretically be collisions.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct EthProposition {
    pub proposition_cid: Vec<u8>,
    pub amount: U256,
    pub sender: Address,
    pub block_number: u64,
}

impl EthProposition {
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

pub type PropositionLedger = Vec<EthProposition>;

#[derive(Fail, Debug)]
pub enum PropositionLedgerSyncError {
    #[fail(display = "Web3 error: {}", error)]
    Web3 { error: SyncFailure<web3::Error> },
    #[fail(display = "An unknown error has occurred.")]
    UnknownError,
}

pub trait PropositionLedgerSyncer<P: Future<Item = (), Error = PropositionLedgerSyncError>> {
    type Config;

    fn sync_ledger(
        &mut self,
        eloop_handle: &tokio_core::reactor::Handle,
        config: Self::Config,
        proposition_ledger_mutex: Arc<Mutex<PropositionLedger>>,
        ledger_block_highwatermark_mtx: Arc<Mutex<u64>>,
    ) -> P;
}

#[derive(Default)]
pub struct EthPropositionLedgerSyncer;

impl EthPropositionLedgerSyncer {
    fn process_ledger_log(
        log: &web3::types::Log,
        signature_map: &HashMap<web3::types::H256, Event>,
    ) -> impl Future<Item = Option<EthProposition>, Error = ()> {
        debug!(
            "got PropositionLedger log: {:?} - {:?}",
            log.transaction_hash, log.log_index
        );
        let event = &signature_map[&log.topics[0]];

        if !Self::is_relevant_event(&event.name) {
            return Ok(None).into_future();
        }

        let proposition = EthProposition::from_log(log, &event);
        Ok(Some(proposition)).into_future()
    }

    fn is_relevant_event(event_type: &str) -> bool {
        let relevant_event_types = vec!["PropositionWeightIncreased"];

        relevant_event_types.contains(&event_type)
    }
}

impl PropositionLedgerSyncer<Box<dyn Future<Item = (), Error = PropositionLedgerSyncError>>>
    for EthPropositionLedgerSyncer
{
    type Config = EthereumBackendConfig;

    fn sync_ledger(
        &mut self,
        eloop_handle: &tokio_core::reactor::Handle,
        config: Self::Config,
        proposition_ledger_mutex: Arc<Mutex<PropositionLedger>>,
        ledger_block_highwatermark_mtx: Arc<Mutex<u64>>,
    ) -> Box<dyn Future<Item = (), Error = PropositionLedgerSyncError>> {
        let web3 = config.web3_with_handle(&eloop_handle);

        let ledger_contract_abi = include_str!("../data/PropositionLedger.abi");
        let contract = ethabi::Contract::load(ledger_contract_abi.as_bytes())
            .expect("Could not load contract ABI");

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

        Box::new(
            combined_stream
                .map_err(|err| PropositionLedgerSyncError::Web3 {
                    error: SyncFailure::new(err),
                })
                .and_then(move |log| {
                    Self::process_ledger_log(&log, &signature_map)
                        .into_future()
                        .map_err(|_| PropositionLedgerSyncError::UnknownError)
                })
                .filter(|res| res.is_some())
                .map(|res| res.unwrap())
                .for_each(move |proposition: EthProposition| {
                    let mut proposition_ledger_lock = proposition_ledger_mutex
                        .lock()
                        .expect("Unable to get lock for proposition ledger");
                    let mut ledger_block_highwatermark = ledger_block_highwatermark_mtx
                        .lock()
                        .expect("Unable to get lock for proposition ledger highwatermark");
                    debug!("New proposition: {:?}", &proposition);
                    *ledger_block_highwatermark = proposition.block_number.clone();
                    proposition_ledger_lock.push(proposition);
                    Ok(())
                })
                .map_err(|err| err.into()),
        )
    }
}
