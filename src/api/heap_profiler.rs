use std::collections::HashMap;
use std::time::SystemTime;
use uptown_funk::{host_functions, StateMarker};

use log::debug;

type Ptr = u32;
type Size = u32;

// TODO: currently profiler is implemented on the host
// and there is associated state with it. Alternatively
// profiler state can be implemented in wasm if overhead
// of invoking host functions will be too big.
pub struct HeapProfilerState {
    malloc_counter: u32,
    calloc_counter: u32,
    realloc_counter: u32,
    free_counter: u32,
    memory: HashMap<Ptr, Size>,
    live_heap_size: u64,
    total_allocated: u64,
    heap_history: Vec<(u64, SystemTime)>,
}

impl StateMarker for HeapProfilerState {}

impl HeapProfilerState {
    pub fn new() -> Self {
        Self {
            malloc_counter: 0,
            calloc_counter: 0,
            realloc_counter: 0,
            free_counter: 0,
            memory: HashMap::new(),
            live_heap_size: 0,
            total_allocated: 0,
            heap_history: vec![],
        }
    }
}

#[host_functions(namespace = "heap_profiler")]
impl HeapProfilerState {
    // TODO: check if calloc/realloc are implemented through malloc/free
    fn malloc_profiler(&mut self, size: Size, ptr: Ptr) {
        debug!("{} malloc({}) -> {}", self.malloc_counter, size, ptr);
        self.malloc_counter += 1;
        self.memory.insert(ptr, size);
        self.total_allocated += size as u64;
        self.live_heap_size += size as u64;
        self.heap_history
            .push((self.live_heap_size, SystemTime::now()));
        debug!(
            "live_heap={} allocated={}",
            self.live_heap_size, self.total_allocated
        );
    }

    fn calloc_profiler(&mut self, len: Size, elem_size: Size, ptr: Ptr) {
        debug!(
            "{} calloc({},{}) -> {}",
            self.calloc_counter, len, elem_size, ptr
        );
        let size = len * elem_size;
        self.calloc_counter += 1;
        self.memory.insert(ptr, size);
        self.total_allocated += size as u64;
        self.live_heap_size += size as u64;
        self.heap_history
            .push((self.live_heap_size, SystemTime::now()));
        debug!(
            "live_heap={} allocated={}",
            self.live_heap_size, self.total_allocated
        );
    }

    fn realloc_profiler(&mut self, old_ptr: Ptr, size: Size, new_ptr: Ptr) {
        debug!(
            "{} realloc({},{}) -> {}",
            self.realloc_counter, old_ptr, size, new_ptr
        );
        self.realloc_counter += 1;
        // TODO: log error/trap if unwrap fails
        let removed_size = self.memory.remove(&old_ptr).unwrap();
        self.memory.insert(new_ptr, size);
        let size_delta = size - removed_size;
        self.total_allocated += size_delta as u64;
        self.live_heap_size += size_delta as u64;
        self.heap_history
            .push((self.live_heap_size, SystemTime::now()));
        debug!(
            "live_heap={} allocated={}",
            self.live_heap_size, self.total_allocated
        );
    }

    fn free_profiler(&mut self, ptr: Ptr) {
        debug!("{}. free({})", self.free_counter, ptr);
        self.free_counter += 1;
        if ptr != 0 {
            // TODO: log error/trap if unwrap fails
            let size = self.memory.remove(&ptr).unwrap();
            self.live_heap_size -= size as u64;
            self.heap_history
                .push((self.live_heap_size, SystemTime::now()));
        }
        debug!(
            "live_heap={} allocated={}",
            self.live_heap_size, self.total_allocated
        );
    }
}
