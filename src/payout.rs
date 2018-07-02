use tokio_core;
use web3;
use std::sync::{Arc, Mutex};
use std::fmt::Write;
use std::collections::HashMap;
use merkle_light::hash::Hashable;
use merkle_light::merkle2::MerkleTree;
use web3::futures::{self, prelude::*};
use web3::types::{Address, H256, U256};
use std::hash::Hasher;
use rustc_hex::ToHex;

use config::Config;
use payout_calculation::payouts_for_epoch;
use sync_proposition_ledger::PropositionLedger;
use merkle::Keccak256Algorithm;

/// Number of host blockchain blocks that make up a epoch
// TODO: should be taken from smart contract
pub const EPOCH_LENGTH: u64 = 20;
/// This block is the start of the first epoch
// TODO: should be taken from smart contract
pub const EPOCH_START_BLOCK: u64 = 0;

pub type PayoutEpochs = HashMap<u64, Vec<Payout>>;

#[derive(Debug, Clone)]
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
        // TODO: if only a single or no payout, pad with payout to zero address
        MerkleTree::from_data(payouts)
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

        let payouts = payouts_for_epoch(epoch, ledger_mtx);
        debug!("Calculated payouts for epoch {}: {:?}", epoch, payouts);
        payout_epochs.insert(epoch, payouts);
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

/// Check if the payout merkle roots for the latest epochs has been submitted to the token contract, and submit them if neccessary.
pub fn submit_epoch_payouts(
    eloop_handle: &tokio_core::reactor::Handle,
    config: Config,
    payout_epochs_mtx: Arc<Mutex<PayoutEpochs>>,
) -> impl Future<Error = ()> {
    let web3 = web3::Web3::new(
        web3::transports::WebSocket::with_event_loop(
            config.network_address.as_ref().unwrap(),
            &eloop_handle,
        ).unwrap(),
    );

    let payout_epochs = payout_epochs_mtx
        .lock()
        .expect("Couldn't aquire lock for payout epochs");

    // Check only the latest epochs so we don't spam the RPC to much
    let mut newest_epochs: Vec<_> = payout_epochs.iter().collect();
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
                    if existing_payout_root != H256::zero() || payouts.len() <= 1 {
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
    write!(&mut proof_str, "{}", payout.amount).unwrap();
    write!(&mut proof_str, ")").unwrap();

    return proof_str;
}
