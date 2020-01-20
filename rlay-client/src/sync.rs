use tokio_core;

use crate::config::Config;

pub fn run_sync(config: &Config) {
    let mut eloop = tokio_core::reactor::Core::new().unwrap();

    let rpc_config = config.clone();
    ::std::thread::spawn(move || {
        crate::rpc::start_rpc(&rpc_config);
    });

    loop {
        eloop.turn(None);
    }
}
