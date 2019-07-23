#![warn(clippy::perf)]
#![recursion_limit = "128"]
#![feature(async_await)]

#[macro_use]
extern crate failure;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;

pub mod backend;
pub mod cli;
pub mod config;
pub mod deploy;
pub mod init;
pub mod modules;
pub mod payout_cli;
pub mod rpc;
pub mod sync;
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
        .subcommand(
            SubCommand::with_name("deploy-contracts")
                .about("Deploy Ethereum contracts")
                .arg(&config_path_arg)
                .arg(
                    Arg::with_name("from_address")
                        .long("from")
                        .value_name("FROM_ADDRESS")
                        .help("Sets a deployment from address")
                        .takes_value(true),
                ),
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
        cli::run_doctor(&config);
    } else if matches.subcommand_matches("init").is_some() {
        init::init();
    } else if let Some(matches) = matches.subcommand_matches("payout") {
        let config_path = matches.value_of("config_path");
        let config = config::Config::from_path_opt(config_path).expect("Couldn't read config file");

        if let Some(matches) = matches.subcommand_matches("show") {
            let payout_args = PayoutParams::from_matches(matches.clone());
            payout_cli::show_payout(&config, payout_args);
        }
    } else if let Some(matches) = matches.subcommand_matches("deploy-contracts") {
        let config_path = matches.value_of("config_path");
        let config = config::Config::from_path_opt(config_path).expect("Couldn't read config file");

        let from = matches
            .value_of("from_address")
            .expect("--from is a required flag");
        deploy::deploy_contracts_with_log(&config, from);
    }
}
