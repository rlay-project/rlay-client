use merkle_light::hash::Hashable;
use merkle_light::merkle2::MerkleTree;
use rustc_hex::ToHex;
use serde_json;
use std::collections::HashMap;
use std::fmt::Write;
use std::fs::{self, File};
use std::hash::Hasher;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio_core;
use web3::futures::{self, prelude::*};
use web3::types::{Address, H256, U256};
use web3;

use config::Config;
use merkle::Keccak256Algorithm;
use payout_calculation::payouts_for_epoch;
use sync_ontology::EntityMap;
use sync_proposition_ledger::PropositionLedger;

/// Number of host blockchain blocks that make up a epoch
// TODO: should be taken from smart contract
pub const EPOCH_LENGTH: u64 = 20;
/// This block is the start of the first epoch
// TODO: should be taken from smart contract
pub const EPOCH_START_BLOCK: u64 = 0;

pub type PayoutEpochs = HashMap<u64, Vec<Payout>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Payout {
    pub address: Address,
    pub amount: U256,
}

impl Payout {
    pub fn empty_for_address(address: Address) -> Self {
        Self {
            address,
            amount: U256::zero(),
        }
    }

    /// Build a merkle tree for a list of payouts
    pub fn build_merkle_tree(payouts: &[Self]) -> MerkleTree<[u8; 32], Keccak256Algorithm> {
        if payouts.len() < 2 {
            let mut padded_payouts: Vec<_> = payouts.to_owned();
            // TODO: maybe also pad with a second address
            padded_payouts.push(Payout::empty_for_address(Address::zero()));
            return MerkleTree::from_data(padded_payouts);
        }
        MerkleTree::from_data(payouts)
    }

    /// Sums up all the payouts for each address.
    ///
    /// The result of this should be directly usable for `build_merkle_tree`.
    pub fn compact_payouts(payouts: Vec<Self>) -> Vec<Self> {
        let mut payouts_by_address: HashMap<Address, Vec<Self>> = HashMap::new();
        for payout in payouts {
            let mut payout_group = payouts_by_address
                .entry(payout.address)
                .or_insert(Vec::new());
            payout_group.push(payout);
        }
        payouts_by_address
            .into_iter()
            .map(|(address, group)| {
                let total_for_address =
                    group.iter().fold(U256::zero(), |acc, val| acc + val.amount);
                Payout {
                    address,
                    amount: total_for_address,
                }
            })
            .collect()
    }
}

impl<H: Hasher> Hashable<H> for Payout {
    fn hash(&self, state: &mut H) {
        self.address.hash(state);

        let mut amount_bytes = [0u8; 32];
        self.amount.to_big_endian(&mut amount_bytes);
        amount_bytes.hash(state);
    }
}

/// Fill the epoch payouts map with the payouts for all completed epochs.
///
/// See also [`payouts_for_epoch`].
///
/// [`payouts_for_epoch`]: ./fn.payouts_for_epoch.html
pub fn fill_epoch_payouts(
    ledger_block_highwatermark_mtx: &Mutex<u64>,
    ledger_mtx: &Mutex<PropositionLedger>,
    payout_epochs_mtx: &Mutex<PayoutEpochs>,
    entity_map_mtx: &Mutex<EntityMap>,
) {
    let ledger_block_highwatermark = ledger_block_highwatermark_mtx.lock().unwrap();
    let mut payout_epochs = payout_epochs_mtx.lock().unwrap();

    let latest_completed_epoch = (*ledger_block_highwatermark - EPOCH_START_BLOCK) / EPOCH_LENGTH;
    debug!("Ledger sync highwatermark: {}", ledger_block_highwatermark);
    debug!("Latest completed epoch: {}", latest_completed_epoch);
    for epoch in 0..=latest_completed_epoch {
        if payout_epochs.contains_key(&epoch) {
            continue;
        }

        let payouts = payouts_for_epoch(epoch, ledger_mtx, entity_map_mtx);
        debug!("Calculated payouts for epoch {}: {:?}", epoch, payouts);
        payout_epochs.insert(epoch, payouts);
    }
}

/// Fill the cumulative epoch payouts map from the payouts map.
pub fn fill_epoch_payouts_cumulative(
    payout_epochs_mtx: &Mutex<PayoutEpochs>,
    payout_epochs_cum_mtx: &Mutex<PayoutEpochs>,
) {
    let payout_epochs = payout_epochs_mtx.lock().unwrap();
    let mut payout_epochs_cum = payout_epochs_cum_mtx.lock().unwrap();

    let latest_calculated_epoch = *payout_epochs.keys().max().unwrap();
    for epoch in 0..=latest_calculated_epoch {
        if payout_epochs_cum.contains_key(&epoch) {
            continue;
        }

        let mut current_epoch_payouts = payout_epochs.get(&epoch).unwrap().clone();
        if epoch == 0 {
            payout_epochs_cum.insert(epoch, current_epoch_payouts);
            continue;
        }

        let mut prev_epoch_payouts = payout_epochs.get(&(epoch - 1)).unwrap().clone();
        current_epoch_payouts.append(&mut prev_epoch_payouts);
        let cumulative_payouts = Payout::compact_payouts(current_epoch_payouts);

        payout_epochs_cum.insert(epoch, cumulative_payouts);
    }
}

fn rlay_token_contract(
    config: &Config,
    web3: &web3::Web3<web3::transports::WebSocket>,
) -> web3::contract::Contract<web3::transports::WebSocket> {
    let token_contract_abi = include_str!("../data/RlayToken.abi");
    web3::contract::Contract::from_json(
        web3.eth(),
        config.contract_address("RlayToken"),
        token_contract_abi.as_bytes(),
    ).expect("Couldn't load RlayToken contract")
}

/// Load epoch_payouts from files in data directory.
pub fn load_epoch_payouts(config: Config, payout_epochs: &mut PayoutEpochs) {
    let epoch_dir = Path::new(&config.data_path.unwrap()).join(Path::new("./epoch_payouts/"));
    for epoch_file in fs::read_dir(epoch_dir).unwrap() {
        let epoch_file = epoch_file.unwrap();
        trace!("Loading epoch_payouts from file {:?}", epoch_file.path());
        let file = File::open(epoch_file.path()).unwrap();

        let contents: serde_json::Value =
            serde_json::from_reader(file).expect("Could not parse JSON file.");
        let epoch_num = contents.get("epoch").unwrap().as_u64().unwrap();
        let payouts: Vec<Payout> =
            serde_json::from_value(contents.get("payouts").unwrap().clone()).unwrap();

        payout_epochs.insert(epoch_num, payouts);
    }
}

/// Store epoch_payouts to files in data directory.
pub fn store_epoch_payouts(config: Config, payout_epochs_mtx: Arc<Mutex<PayoutEpochs>>) {
    let payout_epochs = payout_epochs_mtx
        .lock()
        .expect("Couldn't aquire lock for payout epochs");

    let epoch_dir = Path::new(&config.data_path.unwrap()).join(Path::new("./epoch_payouts/"));
    ::std::fs::create_dir_all(&epoch_dir).unwrap();

    for (epoch_num, payouts) in payout_epochs.iter() {
        let filename = format!("{:08}.json", epoch_num);
        let file_path = epoch_dir.join(Path::new(&filename));

        if file_path.exists() {
            trace!(
                "File at {:?} already exists. Not storing epoch payouts.",
                file_path
            );
            continue;
        }

        let content = json!{{
            "epoch": epoch_num,
            "payouts": payouts,
        }};
        trace!("Writing payout epochs to {:?}.", file_path);
        let payout_file = ::std::fs::File::create(file_path).expect("Could not create file.");
        ::serde_json::to_writer(payout_file, &content).unwrap();
    }
}

/// Check if the payout merkle roots for the latest epochs has been submitted to the token contract, and submit them if neccessary.
pub fn submit_epoch_payouts(
    eloop_handle: &tokio_core::reactor::Handle,
    config: Config,
    payout_epochs_mtx: Arc<Mutex<PayoutEpochs>>,
    payout_epochs_cum_mtx: Arc<Mutex<PayoutEpochs>>,
) -> impl Future<Error = ()> {
    store_epoch_payouts(config.clone(), payout_epochs_mtx.clone());

    let web3 = web3::Web3::new(
        web3::transports::WebSocket::with_event_loop(
            config.network_address.as_ref().unwrap(),
            &eloop_handle,
        ).unwrap(),
    );

    let payout_epochs_cum = payout_epochs_cum_mtx
        .lock()
        .expect("Couldn't aquire lock for payout epochs");

    // Check only the latest epochs so we don't spam the RPC to much
    let mut newest_epochs: Vec<_> = payout_epochs_cum.iter().collect();
    newest_epochs.sort_by_key(|ref n| n.0);
    newest_epochs.reverse();
    let epochs_to_check: Vec<(u64, Vec<Payout>)> = newest_epochs
        .into_iter()
        .take(10)
        .map(|(n, m)| (*n, m.clone()))
        .collect();

    // Get token issuer from contract (only account that is permissioned to submit payout root)
    let contract = rlay_token_contract(&config, &web3);
    let contract_owner = contract
        .query("owner", (), None, web3::contract::Options::default(), None)
        .map_err(|err| {
            error!("{:?}", err);
            ()
        });

    // For each epoch check if a payment root has already been submitted, and if not do so
    contract_owner.and_then(move |token_issuer_address: Address| {
        let epoch_check_futs: Vec<_> = epochs_to_check
            .into_iter()
            .map(|(epoch, payouts)| {
                let contract = rlay_token_contract(&config, &web3);
                let payout_root = contract
                    .query(
                        "payout_roots",
                        epoch,
                        None,
                        web3::contract::Options::default(),
                        None,
                    )
                    .map_err(|err| {
                        error!("{:?}", err);
                        ()
                    });

                payout_root.and_then(move |existing_payout_root: H256| {
                    if payouts.len() <= 0 {
                        trace!(
                            "Payout root for epoch {} does not have enough payouts to submit to smart contract",
                            epoch
                        );
                        return futures::future::Either::A(futures::future::ok(()));
                    }
                    if existing_payout_root != H256::zero() {
                        trace!(
                            "Payout root for epoch {} already present in smart contract",
                            epoch
                        );
                        return futures::future::Either::A(futures::future::ok(()));
                    }

                    let payout_root = Payout::build_merkle_tree(&payouts).root();
                    futures::future::Either::B(
                        contract
                            .call(
                                "submitPayoutRoot",
                                (epoch, payout_root),
                                token_issuer_address,
                                web3::contract::Options::default(),
                            )
                            .and_then(|submit_tx| {
                                info!("Submitted payout root: {:?} (txhash)", submit_tx);
                                Ok(())
                            })
                            .then(|_| Ok(())),
                    )
                })
            })
            .collect();
        futures::future::join_all(epoch_check_futs)
    })
}

pub fn format_redeem_payout_call(
    epoch: u64,
    tree: &MerkleTree<[u8; 32], Keccak256Algorithm>,
    payout: &Payout,
) -> String {
    let proof = ::merkle::gen_proof_for_data(&tree, payout);
    let lemma = proof.lemma().to_owned();
    let mut proof_str = String::new();

    write!(&mut proof_str, "redeemPayout(").unwrap();
    write!(&mut proof_str, "{}, ", epoch).unwrap();
    write!(&mut proof_str, "[").unwrap();
    for proof_item in lemma.iter().skip(1).take(lemma.len() - 2) {
        write!(&mut proof_str, "'0x{}',", proof_item.to_hex()).unwrap();
    }
    write!(&mut proof_str, "],").unwrap();
    write!(&mut proof_str, "'0x{}'", payout.address.to_hex()).unwrap();
    write!(&mut proof_str, ",").unwrap();
    write!(&mut proof_str, "'{}'", payout.amount).unwrap();
    write!(&mut proof_str, ")").unwrap();

    return proof_str;
}
