use async_trait::async_trait;
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

#[async_trait]
impl RlayFilter for WhitelistFilter {
    fn filter_name(&self) -> &'static str {
        "whitelist"
    }

    async fn filter_entities(&self, ctx: FilterContext, entities: Vec<Entity>) -> Vec<bool> {
        let raw_cids = entities
            .into_iter()
            .map(|entity| entity.to_cid().unwrap())
            .collect::<Vec<_>>();

        let used_whitelist: Vec<String> = ctx
            .params
            .as_object()
            .and_then(|obj| obj.get("whitelist"))
            .and_then(|whitelist| {
                serde_json::from_value(whitelist.clone()).expect("Unable to parse whitelist")
            })
            .unwrap_or_else(|| WHITELIST.iter().map(|n| n.to_string()).collect());

        let cids: Vec<String> = raw_cids
            .iter()
            .map(|raw_cid| format!("0x{}", raw_cid.to_bytes().to_hex()))
            .collect::<Vec<_>>();

        cids.into_iter()
            .map(|cid| used_whitelist.contains(&cid))
            .collect::<Vec<_>>()
    }
}
