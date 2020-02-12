use cid::ToCid;
use rlay_plugin_interface::prelude::*;
use rustc_hex::ToHex;

#[no_mangle]
extern "C" fn init_filter_plugin() -> Box<dyn RlayFilter + Send + Sync> {
    Box::new(WhitelistFilter)
}

pub struct WhitelistFilter;

const WHITELIST: &[&'static str] = &[
    "0x019480031b2098e8057cbc8f27a31e2767c53bfa92539556c5e5bc42dd8125d9ad5e36a1c10e",
    "0x019580031b20428df13c43218f450d449c62438a7a491ab1c0eb87ab16391ebe31ec8592e03d",
];

impl RlayFilter for WhitelistFilter {
    fn filter_name(&self) -> &'static str {
        "whitelist"
    }

    fn filter_entity(&self, _ctx: &FilterContext, entity: &Entity) -> bool {
        let raw_cid = entity.to_cid().unwrap();
        let cid: String = format!("0x{}", raw_cid.to_bytes().to_hex());

        WHITELIST.contains(&&*cid)
    }
}
