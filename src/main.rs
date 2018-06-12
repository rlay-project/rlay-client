extern crate cid;
extern crate clap;
extern crate console;
extern crate ethabi;
extern crate failure;
extern crate itertools;
extern crate multibase;
extern crate rustc_hex;
#[macro_use]
extern crate serde_derive;
extern crate tokio_core;
extern crate toml;
extern crate web3;

mod config;
mod doctor;
mod sync;

use clap::{App, Arg, SubCommand};

fn main() {
    let matches = App::new("rlay-client")
        .about("Client to interact with the Rlay protocol")
        .subcommand(SubCommand::with_name("client").about("Run the rlay client"))
        .subcommand(
            SubCommand::with_name("doctor")
                .about("Diagnose problems by running a series of checks")
                .arg(
                    Arg::with_name("config_path")
                        .long("config")
                        .value_name("FILE")
                        .help("Sets a custom config file")
                        .takes_value(true),
                ),
        )
        .get_matches();

    if let Some(_matches) = matches.subcommand_matches("client") {
        sync::run_sync();
    } else if let Some(matches) = matches.subcommand_matches("doctor") {
        let config_path = matches.value_of("config_path");
        let config = config::Config::from_path_opt(config_path).expect("Couldn't read config file");
        doctor::run_checks(&config);
    }
}
