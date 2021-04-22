use crate::api::process::api::ProcessState;
use std::{cell::RefCell, rc::Rc};
use uptown_funk::{Executor, HostFunctions};

use crate::api::channel::ChannelReceiver;
use crate::module::LunaticModule;

use crate::api::{channel, heap_profiler::HeapProfilerState, networking, process, wasi};
pub struct DefaultApi {
    context_receiver: Option<ChannelReceiver>,
    module: LunaticModule,
}

impl DefaultApi {
    pub fn new(context_receiver: Option<ChannelReceiver>, module: LunaticModule) -> Self {
        Self {
            context_receiver,
            module,
        }
    }
}

impl HostFunctions for DefaultApi {
    type Return = (Rc<RefCell<HeapProfilerState>>, Rc<RefCell<ProcessState>>);

    #[cfg(feature = "vm-wasmtime")]
    fn add_to_linker<E>(self, executor: E, linker: &mut wasmtime::Linker) -> Self::Return
    where
        E: Executor + Clone + 'static,
    {
        let channel_state = channel::api::ChannelState::new(self.context_receiver);
        let profiler = HeapProfilerState::new();
        let process_state = process::api::ProcessState::new(self.module, channel_state.clone());
        let networking_state = networking::TcpState::new(channel_state.clone());
        let wasi_state = wasi::api::WasiState::new();

        channel_state.add_to_linker(executor.clone(), linker);
        networking_state.add_to_linker(executor.clone(), linker);
        wasi_state.add_to_linker(executor.clone(), linker);
        profiler_state.add_to_linker(executor.clone(), linker);
        let rc_process_state = process_state.add_to_linker(executor.clone(), linker);
        let rc_profiler = profiler.add_to_linker(executor.clone(), linker);
        (rc_profiler, rc_process_state)
    }

    #[cfg(feature = "vm-wasmer")]
    fn add_to_wasmer_linker<E>(
        self,
        executor: E,
        linker: &mut uptown_funk::wasmer::WasmerLinker,
        store: &wasmer::Store,
    ) -> Self::Return
    where
        E: Executor + Clone + 'static,
    {
        let channel_state = channel::api::ChannelState::new(self.context_receiver);
        let profiler = HeapProfilerState::new();
        let process_state = process::api::ProcessState::new(self.module, channel_state.clone());
        let networking_state = networking::TcpState::new(channel_state.clone());
        let wasi_state = wasi::api::WasiState::new();

        channel_state.add_to_wasmer_linker(executor.clone(), linker, store);
        networking_state.add_to_wasmer_linker(executor.clone(), linker, store);
        wasi_state.add_to_wasmer_linker(executor.clone(), linker, store);
        let rc_process_state = process_state.add_to_wasmer_linker(executor.clone(), linker, store);
        let rc_profiler = profiler.add_to_wasmer_linker(executor.clone(), linker, store);
        (rc_profiler, rc_process_state)
    }
}
