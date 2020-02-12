use ambassador::Delegate;
use libloading as lib;
use libloading::Symbol;
use rlay_ontology::prelude::Entity;
use rlay_plugin_interface::{FilterContext, RlayFilter};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Path;
use std::sync::Arc;

sa::assert_impl_all!(RlayFilterPlugin: Send, Sync);
#[derive(Delegate)]
#[delegate(rlay_plugin_interface::RlayFilter, target = "filter")]
pub struct RlayFilterPlugin {
    filter: Box<dyn RlayFilter + Send + Sync>,
    // Library that the plugin comes from.
    // Has to be the last field so that it's dropped last
    #[allow(unused)]
    library: lib::Library,
}

impl RlayFilterPlugin {
    pub fn load_filter<P: AsRef<OsStr> + std::fmt::Debug>(path: P) -> Self {
        let plugin_lib = lib::Library::new(path).unwrap();
        let filter = unsafe {
            let init_fn: Symbol<extern "C" fn() -> Box<dyn RlayFilter + Send + Sync>> = plugin_lib
                .get(b"init_filter_plugin")
                .expect("Plugin does not expose init_filter_plugin initialization function");
            let filter = init_fn();
            filter
        };

        RlayFilterPlugin {
            library: plugin_lib,
            filter,
        }
    }
}

#[derive(Clone)]
pub struct PluginRegistry {
    filters: HashMap<String, Arc<RlayFilterPlugin>>,
}

impl PluginRegistry {
    pub fn from_dir<P: AsRef<Path>>(dir_path: P) -> Self {
        let filters: Vec<_> = std::fs::read_dir(dir_path)
            .unwrap()
            .map(|dir_entry| dir_entry.unwrap())
            .map(|dir_entry| RlayFilterPlugin::load_filter(dir_entry.path()))
            .collect();

        let mut filter_map = HashMap::new();
        for filter in filters {
            let filter_name = filter.filter_name();
            let existing_filter = filter_map.insert(filter_name.to_owned(), Arc::new(filter));
            if existing_filter.is_some() {
                panic!(
                    "Tried to load two filters with same name \"{name}\".",
                    name = filter_name
                );
            }
        }

        Self {
            filters: filter_map,
        }
    }

    pub fn filter(&self, name: &str) -> Option<Arc<RlayFilterPlugin>> {
        self.filters.get(name).map(|n| n.to_owned())
    }
}
