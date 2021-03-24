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
    // TODO: check if calloc/realloc are implemented through malloc/free
    fn malloc_counter(&mut self, ptr: i32, size: i32) -> i32 {
        self.malloc_counter += 1;
        debug!("{} malloc({}) -> {}", self.malloc_counter, size, ptr);
        ptr
    }

    fn free_counter(&mut self, ptr: i32) {
        self.free_counter += 1;
        debug!("{}. free({})", self.free_counter, ptr);
    }
}
