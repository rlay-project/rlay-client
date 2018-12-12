#![warn(clippy::perf)]

#[macro_use]
extern crate failure;
#[macro_use]
extern crate failure_derive;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;

pub mod aggregation;
pub mod backend;
pub mod config;
pub mod doctor;
pub mod init;
pub mod merkle;
pub mod ontology_ext;
pub mod payout;
pub mod payout_calculation;
pub mod payout_cli;
pub mod rpc;
pub mod sync;
pub mod sync_ontology;
pub mod sync_proposition_ledger;
pub mod web3_helpers;

use clap::{App, Arg, SubCommand};
use env_logger::Builder;
use log::LevelFilter;
use std::io::Write;

use crate::payout_cli::PayoutParams;

fn main() {
    let mut builder = Builder::from_default_env();

    if std::env::var("RUST_LOG").is_err() {
        builder
            .format(|buf, record| writeln!(buf, "{}", record.args()))
            .filter_level(LevelFilter::Info);
    }
    builder.init();

    let config_path_arg = Arg::with_name("config_path")
        .long("config")
        .value_name("FILE")
        .help("Sets a custom config file")
        .takes_value(true);
    let matches = App::new("rlay-client")
        .about("Client to interact with the Rlay protocol")
        .subcommand(
            SubCommand::with_name("client")
                .about("Run the rlay client")
                .arg(&config_path_arg),
        )
        .subcommand(
            SubCommand::with_name("payout")
                .about("Help redeem a reward payout")
                .arg(&config_path_arg)
                .subcommand(
                    SubCommand::with_name("show")
                        .about("Show available payouts at epoch")
                        .arg(
                            Arg::with_name("address")
                                .required(true)
                                .help("The address to look up the payouts for."),
                        )
                        .arg(
                            Arg::with_name("epoch")
                                .required(false)
                                .default_value("latest")
                                .help("The epoch to look up the payouts for."),
                        ),
                ),
        )
        .subcommand(
            SubCommand::with_name("doctor")
                .about("Diagnose problems by running a series of checks")
                .arg(&config_path_arg),
        )
        .subcommand(
            SubCommand::with_name("init").about("Initialize a directory as a project using Rlay"),
        )
        .get_matches();

    if let Some(matches) = matches.subcommand_matches("client") {
        let config_path = matches.value_of("config_path");
        let config = config::Config::from_path_opt(config_path).expect("Couldn't read config file");
        config.init_data_dir().unwrap();
        sync::run_sync(&config);
    } else if let Some(matches) = matches.subcommand_matches("doctor") {
        let config_path = matches.value_of("config_path");
        let config = config::Config::from_path_opt(config_path).expect("Couldn't read config file");
        doctor::run_checks(&config);
    } else if matches.subcommand_matches("init").is_some() {
        init::init();
    } else if let Some(matches) = matches.subcommand_matches("payout") {
        let config_path = matches.value_of("config_path");
        let config = config::Config::from_path_opt(config_path).expect("Couldn't read config file");

        if let Some(matches) = matches.subcommand_matches("show") {
            let payout_args = PayoutParams::from_matches(matches.clone());
            payout_cli::show_payout(&config, payout_args);
        }
    }
}
