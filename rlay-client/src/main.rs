#![warn(clippy::perf)]
#![recursion_limit = "128"]

#[macro_use]
extern crate failure;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;
extern crate static_assertions as sa;

pub mod backend;
pub mod cli;
pub mod config;
pub mod modules;
pub mod rpc;
pub mod sync;

use clap::{App, Arg, SubCommand};
use env_logger::Builder;
use log::LevelFilter;
use std::io::Write;

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
            SubCommand::with_name("init").about("Initialize a directory as a project using Rlay"),
        )
        .get_matches();

    if let Some(matches) = matches.subcommand_matches("client") {
        let config_path = matches.value_of("config_path");
        let config = config::Config::from_path_opt(config_path).expect("Couldn't read config file");
        config.init_data_dir().unwrap();
        sync::run_sync(&config);
    } else if matches.subcommand_matches("init").is_some() {
        cli::run_init();
    }
}
