extern crate cid;
extern crate clap;
extern crate console;
extern crate ethabi;
#[macro_use]
extern crate failure;
extern crate itertools;
extern crate multibase;
extern crate rustc_hex;
extern crate tokio_core;
extern crate web3;

mod sync;
mod doctor;

use clap::{App, SubCommand};

fn main() {
    let matches = App::new("rlay-client")
        .about("Client to interact with the Rlay protocol")
        .subcommand(SubCommand::with_name("client").about("Run the rlay client"))
        .subcommand(
            SubCommand::with_name("doctor")
                .about("Diagnose problems by running a series of checks"),
        )
        .get_matches();

    if let Some(_matches) = matches.subcommand_matches("client") {
        sync::run_sync();
    } else if let Some(_matches) = matches.subcommand_matches("doctor") {
        doctor::run_checks();
    }
}
