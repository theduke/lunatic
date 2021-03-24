use uptown_funk::{host_functions, StateMarker};

use log::debug;

// TODO: currently profiler is implemented on the host
// and there is associated state with it. Alternatively
// profiler state can be implemented in wasm if overhead
// of invoking host functions will be too big.
pub struct HeapProfilerState {
    malloc_counter: u32,
    free_counter: u32,
}

impl StateMarker for HeapProfilerState {}

impl HeapProfilerState {
    pub fn new() -> Self {
        Self {
            malloc_counter: 0,
            free_counter: 0,
        }
    }
}

#[host_functions(namespace = "heap_profiler")]
impl HeapProfilerState {
    fn malloc_counter(&mut self) {
        self.malloc_counter += 1;
        debug!("malloc_counter={}", self.malloc_counter);
    }

    fn free_counter(&mut self) {
        self.free_counter += 1;
        debug!("free_counter={}", self.free_counter);
    }
}
