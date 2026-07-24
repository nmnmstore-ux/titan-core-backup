#![allow(dead_code)]
#[cfg(feature = "wasm")]
use wasmtime::*;

pub struct MatchDecision {
    pub allow_match: bool,
    pub price_override: Option<f64>,
    pub quantity_override: Option<f64>,
    pub log: String,
}

pub trait WasmMatchHook: Send + Sync {
    fn on_match(&self, taker: &crate::types::Order, maker: &crate::types::Order) -> MatchDecision;
    fn on_place(&self, order: &crate::types::Order) -> Result<(), String>;
}

pub struct NoopHook;
impl WasmMatchHook for NoopHook {
    fn on_match(&self, _taker: &crate::types::Order, _maker: &crate::types::Order) -> MatchDecision {
        MatchDecision { allow_match: true, price_override: None, quantity_override: None, log: String::new() }
    }
    fn on_place(&self, _order: &crate::types::Order) -> Result<(), String> { Ok(()) }
}

#[cfg(feature = "wasm")]
pub struct WasmHook {
    engine: Engine,
    modules: DashMap<String, (Module, Store<()>, Instance)>,
    hooks_dir: String,
}

#[cfg(feature = "wasm")]
impl WasmHook {
    pub fn new(hooks_dir: &str) -> Result<Self, String> {
        let mut config = Config::new();
        config.debug_info(false);
        config.consume_fuel(true);
        let engine = Engine::new(&config)
            .map_err(|e| format!("wasm engine: {}", e))?;
        Ok(Self {
            engine,
            modules: DashMap::new(),
            hooks_dir: hooks_dir.to_string(),
        })
    }

    pub fn load_hook(&self, name: &str, wasm_bytes: &[u8]) -> Result<(), String> {
        let module = Module::new(&self.engine, wasm_bytes)
            .map_err(|e| format!("wasm module: {}", e))?;
        let mut store = Store::new(&self.engine, ());
        store.set_fuel(1_000_000).map_err(|e| format!("fuel: {}", e))?;
        let instance = Instance::new(&mut store, &module, &[])
            .map_err(|e| format!("wasm instantiate: {}", e))?;
        self.modules.insert(name.to_string(), (module, store, instance));
        Ok(())
    }

    pub fn load_from_file(&self, path: &str) -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|e| format!("read wasm: {}", e))?;
        let name = Path::new(path).file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("hook");
        self.load_hook(name, &bytes)
    }
}

#[cfg(feature = "wasm")]
impl WasmMatchHook for WasmHook {
    fn on_match(&self, taker: &crate::types::Order, maker: &crate::types::Order) -> MatchDecision {
        let name = "default";
        if let Some(entry) = self.modules.get(name) {
            let (_, _, instance) = &*entry;
            if let Ok(func) = instance.get_typed_func::<(f64, f64, f64, f64, i32), i32>(
                &mut entry.value().1, "on_match"
            ) {
                let result = func.call(
                    &mut entry.value().1,
                    (taker.price, taker.quantity, maker.price, maker.quantity, 0)
                );
                match result {
                    Ok(code) => MatchDecision {
                        allow_match: code != 0,
                        price_override: None,
                        quantity_override: None,
                        log: format!("wasm({}) = {}", name, code),
                    },
                    Err(e) => MatchDecision {
                        allow_match: true,
                        price_override: None,
                        quantity_override: None,
                        log: format!("wasm error: {}", e),
                    },
                }
            } else {
                MatchDecision { allow_match: true, price_override: None, quantity_override: None, log: String::new() }
            }
        } else {
            MatchDecision { allow_match: true, price_override: None, quantity_override: None, log: String::new() }
        }
    }

    fn on_place(&self, _order: &crate::types::Order) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(not(feature = "wasm"))]
pub struct WasmHook;
#[cfg(not(feature = "wasm"))]
impl WasmHook {
    pub fn new(_hooks_dir: &str) -> Result<Self, String> {
        Err("WASM runtime not enabled (compile with --features wasm)".to_string())
    }
    pub fn load_hook(&self, _name: &str, _bytes: &[u8]) -> Result<(), String> {
        Err("WASM not enabled".to_string())
    }
    pub fn load_from_file(&self, _path: &str) -> Result<(), String> {
        Err("WASM not enabled".to_string())
    }
}

#[cfg(not(feature = "wasm"))]
impl WasmMatchHook for WasmHook {
    fn on_match(&self, _taker: &crate::types::Order, _maker: &crate::types::Order) -> MatchDecision {
        MatchDecision { allow_match: true, price_override: None, quantity_override: None, log: "wasm disabled".into() }
    }
    fn on_place(&self, _order: &crate::types::Order) -> Result<(), String> {
        Ok(())
    }
}
