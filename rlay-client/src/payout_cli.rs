use clap::ArgMatches;
use rustc_hex::{FromHex, ToHex};
use std::collections::HashMap;
use std::num::ParseIntError;
use std::str::FromStr;
use std::sync::Mutex;
use web3::types::{Address, H160};

use crate::config::Config;
use crate::payout::{fill_epoch_payouts_cumulative, load_epoch_payouts, Payout, PayoutEpochs};

pub enum Epoch {
    Number(u64),
    Latest,
}

impl FromStr for Epoch {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "latest" {
            return Ok(Epoch::Latest);
        }

        let num = u64::from_str(s)?;
        Ok(Epoch::Number(num))
    }
}

pub struct PayoutParams {
    pub address: Address,
    pub epoch: Epoch,
}

impl PayoutParams {
    pub fn from_matches(matches: ArgMatches) -> Self {
        let address_bytes = matches
            .value_of("address")
            .expect("Could not find param address")
            .from_hex()
            .expect("address param can not be parsed as address");
        let address = H160::from_slice(&address_bytes);

        let epoch = Epoch::from_str(matches.value_of("epoch").unwrap()).unwrap();

        PayoutParams { address, epoch }
    }
}

pub fn show_payout(config: &Config, payout_params: PayoutParams) {
    let mut payout_epochs: PayoutEpochs = HashMap::new();
    // Load state from storage
    load_epoch_payouts(config.clone(), &mut payout_epochs);

    let payout_epochs_cum: PayoutEpochs = {
        let payout_epochs_mutex = Mutex::new(payout_epochs);
        let payout_epochs_cum: PayoutEpochs = HashMap::new();
        let payout_epochs_cum_mutex = Mutex::new(payout_epochs_cum);
        fill_epoch_payouts_cumulative(&payout_epochs_mutex, &payout_epochs_cum_mutex);

        payout_epochs_cum_mutex.into_inner().unwrap()
    };

    let epoch: u64 = match payout_params.epoch {
        Epoch::Latest => *payout_epochs_cum.keys().max().unwrap(),
        Epoch::Number(num) => num,
    };

    let payouts = payout_epochs_cum.get(&epoch).unwrap();
    let tree = Payout::build_merkle_tree(payouts);

    let payout = payouts
        .iter()
        .find(|n| n.address == payout_params.address)
        .expect("Could not find payout for requested address.");
    let proof_str = crate::payout::format_redeem_payout_call(epoch, &tree, payout);
    println!("Address: 0x{}", payout.address.to_hex());
    println!("Cumulative blance: {}", payout.amount.to_string());
    println!(
        "Payout root for epoch {}: 0x{}",
        epoch,
        tree.root().to_hex()
    );
    println!("");
    println!("web3 call: {}", proof_str);
}
