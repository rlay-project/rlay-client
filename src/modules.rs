use hlua::Lua;
use rlay_ontology::prelude::*;
use serde_json::Value as JsonValue;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

#[derive(Clone)]
pub struct LuaEntity(EntityFormatWeb3);

impl<'lua, L> hlua::Push<L> for LuaEntity
where
    L: hlua::AsMutLua<'lua>,
{
    type Err = ();

    fn push_to_lua(self, lua: L) -> Result<hlua::PushGuard<L>, (Self::Err, L)> {
        Ok(hlua::push_userdata(self.clone(), lua, |mut metatable| {
            metatable.set(
                "__index",
                hlua::function2(move |_: hlua::AnyLuaValue, ref key: String| {
                    let serialized: JsonValue = serde_json::to_value(self.0.clone()).unwrap();
                    let serialized = serialized.as_object().unwrap();

                    let value = serialized.get(key).unwrap();
                    match value {
                        JsonValue::String(val) => hlua::AnyLuaValue::LuaString(val.to_string()),
                        JsonValue::Array(arr_val) => hlua::AnyLuaValue::LuaArray(
                            arr_val
                                .into_iter()
                                .enumerate()
                                .map(|(i, val)| {
                                    (
                                        hlua::AnyLuaValue::LuaNumber(i as f64),
                                        hlua::AnyLuaValue::LuaString(val.to_string()),
                                    )
                                })
                                .collect(),
                        ),
                        _ => hlua::AnyLuaValue::LuaNil,
                    }
                }),
            )
        }))
    }
}

pub struct LuaModule<'a> {
    pub lua: Lua<'a>,
}

impl<'a> LuaModule<'a> {
    pub fn from_file(path: &str) -> Self {
        let mut lua = Lua::new();
        lua.execute_from_reader::<(), _>(File::open(&Path::new(path)).unwrap())
            .unwrap();

        Self { lua }
    }

    pub fn from_str(content: &str) -> Self {
        let mut lua = Lua::new();
        lua.execute::<()>(content).unwrap();

        Self { lua }
    }
}

pub struct FilterModule<'a> {
    loaded_path: Option<String>,
    module: LuaModule<'a>,
}

impl<'a> FilterModule<'a> {
    pub fn from_file(path: &str) -> Self {
        let mut module = LuaModule::from_file(path);
        module.lua.openlibs();

        Self {
            loaded_path: Some(path.to_owned()),
            module,
        }
    }

    pub fn from_str(content: &str) -> Self {
        let mut module = LuaModule::from_str(content);
        module.lua.openlibs();

        Self {
            loaded_path: None,
            module,
        }
    }

    pub fn filter(&mut self, entity: Entity) -> bool {
        let entity = LuaEntity(entity.to_web3_format());
        let mut filter_fn = self
            .module
            .lua
            .get::<hlua::LuaFunction<_>, _>("filter")
            .expect(&format!(
                "Module at {:?} is missing function \"filter\"",
                &self.loaded_path
            ));

        filter_fn
            .call_with_args::<bool, LuaEntity, ()>(entity)
            .unwrap()
    }
}

pub struct ModuleRegistry<'a> {
    filters: HashMap<String, RefCell<FilterModule<'a>>>,
}

impl<'a> ModuleRegistry<'a> {
    pub fn with_builtins() -> Self {
        let mut _self = Self {
            filters: HashMap::new(),
        };

        _self.filters.insert(
            "whitelist_filter".to_owned(),
            RefCell::new(crate::modules::FilterModule::from_str(include_str!(
                "modules/whitelist_filter.lua",
            ))),
        );

        _self
    }

    pub fn filter(&self, name: &str) -> Option<&'a RefCell<FilterModule>> {
        self.filters.get(name)
    }
}
