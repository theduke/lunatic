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
    memory: HashMap<Ptr, Size>,
    live_heap_size: u64,
    total_allocated: u64,
    heap_history: Vec<(u64, SystemTime)>,
}

impl StateMarker for HeapProfilerState {}

impl HeapProfilerState {
    pub fn new() -> Self {
        Self {
            memory: HashMap::new(),
            live_heap_size: 0,
            total_allocated: 0,
            heap_history: vec![(0, SystemTime::now())],
        }
    }
}

#[host_functions(namespace = "heap_profiler")]
impl HeapProfilerState {
    fn malloc_profiler(&mut self, size: Size, ptr: Ptr) {
        debug!("heap_profiler: malloc({}) -> {}", size, ptr);
        self.memory.insert(ptr, size);
        self.total_allocated += size as u64;
        self.live_heap_size += size as u64;
        self.heap_history
            .push((self.live_heap_size, SystemTime::now()));
        debug!(
            "heap_profiler: live_heap={} allocated={}",
            self.live_heap_size, self.total_allocated
        );
    }

    fn calloc_profiler(&mut self, len: Size, elem_size: Size, ptr: Ptr) {
        debug!("heap_profiler: calloc({},{}) -> {}", len, elem_size, ptr);
        let size = len * elem_size;
        self.memory.insert(ptr, size);
        self.total_allocated += size as u64;
        self.live_heap_size += size as u64;
        self.heap_history
            .push((self.live_heap_size, SystemTime::now()));
        debug!(
            "heap_profiler: live_heap={} allocated={}",
            self.live_heap_size, self.total_allocated
        );
    }

    fn realloc_profiler(&mut self, old_ptr: Ptr, size: Size, new_ptr: Ptr) {
        debug!(
            "heap_profiler: realloc({},{}) -> {}",
            old_ptr, size, new_ptr
        );
        // TODO: log error/trap if unwrap fails
        let removed_size = self.memory.remove(&old_ptr).unwrap();
        self.memory.insert(new_ptr, size);
        let size_delta = size - removed_size;
        self.total_allocated += size_delta as u64;
        self.live_heap_size += size_delta as u64;
        self.heap_history
            .push((self.live_heap_size, SystemTime::now()));
        debug!(
            "heap_profiler: live_heap={} allocated={}",
            self.live_heap_size, self.total_allocated
        );
    }

    fn free_profiler(&mut self, ptr: Ptr) {
        debug!("heap_profiler: free({})", ptr);
        if ptr != 0 {
            // TODO: log error/trap if unwrap fails
            let size = self.memory.remove(&ptr).unwrap();
            self.live_heap_size -= size as u64;
            self.heap_history
                .push((self.live_heap_size, SystemTime::now()));
        }
        debug!(
            "heap_profiler: live_heap={} allocated={}",
            self.live_heap_size, self.total_allocated
        );
    }
}
