use crate::config::Config;

pub fn run_sync(config: &Config) {
    crate::rpc::start_rpc(&config);
}
