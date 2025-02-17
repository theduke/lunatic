use anyhow::Result;
use async_wormhole::{
    stack::{OneMbStack, Stack},
    AsyncWormhole, AsyncYielder,
};
use lazy_static::lazy_static;
use smol::{Executor as TaskExecutor, Task};
use uptown_funk::{Executor, FromWasm, HostFunctions, ToWasm};

use crate::module::{LunaticModule, Runtime};
use crate::{api::channel::ChannelReceiver, linker::*};

use log::info;
use std::future::Future;

use crate::api::DefaultApi;

use super::api::ProcessState;
use super::err::*;

lazy_static! {
    pub static ref EXECUTOR: TaskExecutor<'static> = TaskExecutor::new();
}

/// Used to look up a function by name or table index inside of an Instance.
pub enum FunctionLookup {
    TableIndex(u32),
    Name(&'static str),
}

/// For now we always create a new memory per instance, but eventually we will want to support
/// sharing memories between instances.
///
/// A new memory can enforce a maximum size in Wasm pages, where 1 Wasm page = 64KiB memory.
#[derive(Clone)]
pub enum MemoryChoice {
    Existing,
    New(Option<u32>),
}

/// A lunatic process represents an actor.
pub struct Process {
    task: Task<Result<(), Error<()>>>,
}

impl Process {
    pub fn task(self) -> Task<Result<(), Error<()>>> {
        self.task
    }

    /// Creates a new process with a custom API.
    pub async fn create_with_api<A>(
        module: LunaticModule,
        function: FunctionLookup,
        memory: MemoryChoice,
        api: A,
    ) -> Result<A::Return, Error<A::Return>>
    where
        A: HostFunctions + 'static + Send,
    {
        // The creation of AsyncWormhole needs to be wrapped in an async function.
        // AsyncWormhole performs linking between the new and old stack, so that tools like backtrace work correctly.
        // This linking is performed when AsyncWormhole is created and we want to postpone the creation until the
        // executor has control.
        // Otherwise it can happen that the process from which we are creating the stack from dies before the new
        // process. In this case we would link the new stack to one that gets freed, and backtrace crashes once
        // it walks into the freed stack.
        // The executor will never live on a "virtual" stack, so it's safe to create AsyncWormhole there.
        let created_at = std::time::Instant::now();
        let runtime = module.runtime();

        let stack = OneMbStack::new()?;
        let mut process = AsyncWormhole::new(stack, move |yielder| {
            let yielder_ptr =
                &yielder as *const AsyncYielder<Result<A::Return, Error<A::Return>>> as usize;

            match module.runtime() {
                #[cfg(feature = "vm-wasmtime")]
                Runtime::Wasmtime => {
                    let mut linker = WasmtimeLunaticLinker::<A>::new(module, yielder_ptr, memory)?;
                    let ret = linker.add_api(api);
                    let instance = linker.instance()?;

                    match function {
                        FunctionLookup::Name(name) => {
                            let func = instance.get_func(name).ok_or_else(|| {
                                anyhow::Error::msg(format!(
                                    "No function {} in wasmtime instance",
                                    name
                                ))
                            })?;

                            // Measure how long the function takes for named functions.
                            let performance_timer = std::time::Instant::now();
                            func.call(&[]).map_err(|error| Error {
                                error: error.into(),
                                value: Some(ret.clone()),
                            })?;
                            info!(target: "performance", "Process {} finished in {:.5} ms.", name, performance_timer.elapsed().as_secs_f64() * 1000.0);
                        }
                        FunctionLookup::TableIndex(index) => {
                            let func =
                                instance.get_func("lunatic_spawn_by_index").ok_or_else(|| {
                                    anyhow::Error::msg(
                                        "No function lunatic_spawn_by_index in wasmtime instance",
                                    )
                                })?;

                            func.call(&[(index as i32).into()])?;
                        }
                    }

                    Ok(ret)
                }
                #[cfg(feature = "vm-wasmer")]
                Runtime::Wasmer => {
                    let mut linker = WasmerLunaticLinker::<A>::new(module, yielder_ptr, memory)?;
                    let ret = linker.add_api(api);
                    let instance = linker.instance()?;

                    match function {
                        FunctionLookup::Name(name) => {
                            let func = instance.exports.get_function(name)?;

                            // Measure how long the function takes for named functions.
                            let performance_timer = std::time::Instant::now();
                            func.call(&[]).map_err(|error| Error {
                                error: error.into(),
                                value: Some(ret.clone()),
                            })?;
                            info!(target: "performance", "Process {} finished in {:.5} ms.", name, performance_timer.elapsed().as_secs_f64() * 1000.0);
                        }
                        FunctionLookup::TableIndex(index) => {
                            let func = instance.exports.get_function("lunatic_spawn_by_index")?;
                            func.call(&[(index as i32).into()])?;
                        }
                    }

                    Ok(ret)
                }
            }
        })?;

        #[cfg(feature = "vm-wasmtime")]
        let mut wasmtime_cts_saver = super::tls::CallThreadStateSaveWasmtime::new();
        #[cfg(feature = "vm-wasmer")]
        let wasmer_cts_saver = super::tls::CallThreadStateSaveWasmer::new();
        process.set_pre_post_poll(move || match runtime {
            #[cfg(feature = "vm-wasmtime")]
            Runtime::Wasmtime => wasmtime_cts_saver.swap(),
            #[cfg(feature = "vm-wasmer")]
            Runtime::Wasmer => wasmer_cts_saver.swap(),
        });

        let ret = process.await;
        info!(target: "performance", "Total time {:.5} ms.", created_at.elapsed().as_secs_f64() * 1000.0);
        ret
    }

    /// Creates a new process using the default api.
    pub async fn create(
        context_receiver: Option<ChannelReceiver>,
        module: LunaticModule,
        function: FunctionLookup,
        memory: MemoryChoice,
    ) -> Result<(), Error<()>> {
        let api = DefaultApi::new(context_receiver, module.clone());
        Process::create_with_api(module, function, memory, api).await
    }

    /// Spawns a new process on the `EXECUTOR`
    pub fn spawn<Fut>(future: Fut) -> Self
    where
        Fut: Future<Output = Result<(), Error<()>>> + Send + 'static,
    {
        let task = EXECUTOR.spawn(future);
        Self { task }
    }
}

impl ToWasm<&mut ProcessState> for Process {
    type To = u32;

    fn to(
        state: &mut ProcessState,
        _: &impl Executor,
        process: Self,
    ) -> Result<u32, uptown_funk::Trap> {
        Ok(state.processes.add(process))
    }
}

impl FromWasm<&mut ProcessState> for Process {
    type From = u32;

    fn from(
        state: &mut ProcessState,
        _: &impl Executor,
        process_id: u32,
    ) -> Result<Self, uptown_funk::Trap>
    where
        Self: Sized,
    {
        match state.processes.remove(process_id) {
            Some(process) => Ok(process),
            None => Err(uptown_funk::Trap::new("Process not found")),
        }
    }
}
