#![cfg_attr(feature = "cargo-clippy", allow(let_and_return))]

extern crate cid;
extern crate clap;
extern crate console;
extern crate env_logger;
extern crate ethabi;
extern crate failure;
extern crate futures_timer;
#[macro_use]
extern crate log;
extern crate multibase;
extern crate rlay_ontology;
extern crate rustc_hex;
#[macro_use]
extern crate serde_derive;
extern crate tokio_core;
extern crate toml;
extern crate web3;

mod config;
mod doctor;
mod sync;
mod sync_ontology;

use std::io::Write;
use clap::{App, Arg, SubCommand};
use log::LevelFilter;
use env_logger::Builder;

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
            SubCommand::with_name("doctor")
                .about("Diagnose problems by running a series of checks")
                .arg(&config_path_arg),
        )
        .get_matches();

    if let Some(matches) = matches.subcommand_matches("client") {
        let config_path = matches.value_of("config_path");
        let config = config::Config::from_path_opt(config_path).expect("Couldn't read config file");
        sync::run_sync(&config);
    } else if let Some(matches) = matches.subcommand_matches("doctor") {
        let config_path = matches.value_of("config_path");
        let config = config::Config::from_path_opt(config_path).expect("Couldn't read config file");
        doctor::run_checks(&config);
    }
}
