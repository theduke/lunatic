use anyhow::Result;
use async_wormhole::{
    stack::{OneMbStack, Stack},
    AsyncWormhole, AsyncYielder,
};
use lazy_static::lazy_static;
use smol::{Executor as TaskExecutor, Task};
use std::rc::Rc;
use uptown_funk::{memory::Memory, Executor, FromWasm, HostFunctions, ToWasm};

use crate::module::LunaticModule;
use crate::{api::channel::ChannelReceiver, linker::LunaticLinker};

use log::info;
use std::mem::ManuallyDrop;
use std::{future::Future, marker::PhantomData};

use crate::api::DefaultApi;

use super::api::ProcessState;

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

/// This structure is captured inside HOST function closures passed to Wasmtime's Linker.
/// It allows us to expose Lunatic runtime functionalities inside host functions, like
/// async yields or Instance memory access.
///
/// ### Safety
///
/// Having a mutable slice of Wasmtime's memory is generally unsafe, but Lunatic always uses
/// static memories and one memory per instance. This makes it somewhat safe?
#[cfg_attr(feature = "vm-wasmer", derive(Clone))]
pub struct ProcessEnvironment<T: Clone> {
    memory: Memory,
    yielder: usize,
    yield_value: PhantomData<T>,
}

impl<T: Sized + Clone> uptown_funk::Executor for ProcessEnvironment<T> {
    type Return = T;

    #[inline(always)]
    fn async_<R, F>(&self, f: F) -> R
    where
        F: Future<Output = R>,
    {
        // The yielder should not be dropped until this process is done running.
        let mut yielder = unsafe {
            std::ptr::read(self.yielder as *const ManuallyDrop<AsyncYielder<Result<T, Error<T>>>>)
        };
        yielder.async_suspend(f)
    }

    fn memory(&self) -> Memory {
        self.memory.clone()
    }
}

// Because of a bug in Wasmtime: https://github.com/bytecodealliance/wasmtime/issues/2583
// we need to duplicate the Memory in the Linker before storing it in ProcessEnvironment,
// to not increase the reference count.
// When we are droping the memory we need to make sure we forget the value to not decrease
// the reference count.
// Safety: The ProcessEnvironment has the same lifetime as Memory, so it should be safe to
// do this.
#[cfg(feature = "vm-wasmtime")]
impl<T: Sized + Clone> Drop for ProcessEnvironment<T> {
    fn drop(&mut self) {
        let memory = std::mem::replace(&mut self.memory, Memory::Empty);
        std::mem::forget(memory)
    }
}

// For the same reason mentioned on the Drop trait we can't increase the reference count
// on the Memory when cloning.
#[cfg(feature = "vm-wasmtime")]
impl<T: Sized + Clone> Clone for ProcessEnvironment<T> {
    fn clone(&self) -> Self {
        Self {
            memory: unsafe { std::ptr::read(&self.memory as *const Memory) },
            yielder: self.yielder,
            yield_value: PhantomData::default(),
        }
    }
}

impl<T: Clone + Sized> ProcessEnvironment<T> {
    pub fn new(memory: Memory, yielder: usize) -> Self {
        Self {
            memory,
            yielder,
            yield_value: PhantomData::default(),
        }
    }
}

pub struct Error<T> {
    pub error: anyhow::Error,
    pub value: Option<T>,
}

impl<T, E: Into<anyhow::Error>> From<E> for Error<T> {
    fn from(error: E) -> Self {
        Self {
            error: error.into(),
            value: None,
        }
    }
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

        let stack = OneMbStack::new()?;
        let mut process = AsyncWormhole::new(stack, move |yielder| {
            let yielder_ptr =
                &yielder as *const AsyncYielder<Result<A::Return, Error<A::Return>>> as usize;

            let mut linker = LunaticLinker::<A>::new(module, yielder_ptr, memory)?;
            let ret = linker.add_api(api);
            let instance = linker.instance()?;

            match function {
                FunctionLookup::Name(name) => {
                    #[cfg(feature = "vm-wasmer")]
                    let func = instance.exports.get_function(name)?;
                    #[cfg(feature = "vm-wasmtime")]
                    let func = instance.get_func(name).ok_or_else(|| {
                        anyhow::Error::msg(format!("No function {} in wasmtime instance", name))
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
                    #[cfg(feature = "vm-wasmer")]
                    let func = instance.exports.get_function("lunatic_spawn_by_index")?;
                    #[cfg(feature = "vm-wasmtime")]
                    let func = instance.get_func("lunatic_spawn_by_index").ok_or_else(|| {
                        anyhow::Error::msg(
                            "No function lunatic_spawn_by_index in wasmtime instance",
                        )
                    })?;

                    func.call(&[(index as i32).into()])?;
                }
            }

            Ok(ret)
        })?;

        let cts_saver = super::tls::CallThreadStateSave::new();
        process.set_pre_post_poll(move || cts_saver.swap());
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
        let ret = Process::create_with_api(module, function, memory, api).await;
        let ret = match ret {
            Ok(r) => r,
            Err(Error { error, value }) => match value {
                Some(r) => r,
                None => {
                    dbg!(&error);
                    return Err(From::from(error));
                }
            },
        };
        let mut profile_out = std::fs::File::create("heap.dat")?;
        let profile = Rc::try_unwrap(ret)
            .map_err(|_| {
                anyhow::Error::msg("api_process_create: Heap profiler referenced multiple times")
            })?
            .into_inner();
        profile.write_dat(&mut profile_out)?;

        Ok(())
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

impl ToWasm for Process {
    type To = u32;
    type State = ProcessState;

    fn to(
        state: &mut Self::State,
        _: &impl Executor,
        process: Self,
    ) -> Result<u32, uptown_funk::Trap> {
        Ok(state.processes.add(process))
    }
}

impl FromWasm for Process {
    type From = u32;
    type State = ProcessState;

    fn from(
        state: &mut Self::State,
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
