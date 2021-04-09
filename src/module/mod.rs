use anyhow::Result;
#[cfg(feature = "vm-wasmer")]
use wasmer::Module;
#[cfg(feature = "vm-wasmtime")]
use wasmtime::Module;

use crate::linker::engine;
use normalisation::patch;

pub mod normalisation;

#[derive(Clone)]
pub struct LunaticModule {
    module: Module,
    min_memory: u32,
    max_memory: Option<u32>,
}

impl LunaticModule {
    pub fn new(wasm: &[u8], is_profile: bool, is_normalisation_out: bool) -> Result<Self> {
        // Transfrom WASM file into a format compatible with Lunatic.
        let ((min_memory, max_memory), wasm) = patch(&wasm, is_profile, is_normalisation_out)?;

        let engine = engine();
        let module = Module::new(&engine, wasm)?;

        Ok(Self {
            module,
            min_memory,
            max_memory,
        })
    }

    pub fn module(&self) -> &Module {
        &self.module
    }

    pub fn min_memory(&self) -> u32 {
        self.min_memory
    }

    pub fn max_memory(&self) -> Option<u32> {
        self.max_memory
    }
}
