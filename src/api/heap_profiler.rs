use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::Write;
use std::time::{Duration, SystemTime};
use uptown_funk::{host_functions, StateMarker};

use log::debug;
use log::error;

type Ptr = u32;
type Size = u32;
const HISTORY_CAPACITY: usize = 100000;

// TODO: currently profiler is implemented on the host
// and there is associated state with it. Alternatively
// profiler state can be implemented in wasm if overhead
// of invoking host functions will be too big.
pub struct HeapProfilerState {
    memory: HashMap<Ptr, Size>,
    live_heap_size: u64,
    total_allocated: u64,
    started: SystemTime,
    heap_history: VecDeque<(u64, Duration)>,
}

impl StateMarker for HeapProfilerState {}

impl HeapProfilerState {
    pub fn new() -> Self {
        let mut history = VecDeque::with_capacity(HISTORY_CAPACITY);
        history.push_back((0, Duration::new(0, 0)));
        Self {
            memory: HashMap::new(),
            live_heap_size: 0,
            total_allocated: 0,
            started: SystemTime::now(),
            heap_history: history,
        }
    }

    pub fn write_dat(&self, fd: &mut File) -> std::io::Result<()> {
        let mut graph = Vec::new();
        writeln!(&mut graph, "#time/sec heap/byte")?;
        self.heap_history.iter().for_each(|(heap, duration)| {
            writeln!(&mut graph, "{} {}", duration.as_secs_f64(), heap).unwrap();
        });
        fd.write_all(&graph)
    }

    fn history_push(&mut self) {
        // TODO: trap if elapsed failed
        if self.heap_history.len() == HISTORY_CAPACITY {
            // if HISTRY_CAPACITY > 0 this should be safe
            self.heap_history.pop_front().unwrap();
        }
        self.heap_history
            .push_back((self.live_heap_size, self.started.elapsed().unwrap()));
        debug!(
            "heap_profiler: live_heap={} allocated={}",
            self.live_heap_size, self.total_allocated
        );
    }
}

#[host_functions(namespace = "heap_profiler")]
impl HeapProfilerState {
    fn malloc_profiler(&mut self, size: Size, ptr: Ptr) {
        debug!("heap_profiler: malloc({}) -> {}", size, ptr);
        self.memory.insert(ptr, size);
        self.total_allocated += size as u64;
        self.live_heap_size += size as u64;
        self.history_push();
    }

    fn calloc_profiler(&mut self, len: Size, elem_size: Size, ptr: Ptr) {
        debug!("heap_profiler: calloc({},{}) -> {}", len, elem_size, ptr);
        let size = len * elem_size;
        self.memory.insert(ptr, size);
        self.total_allocated += size as u64;
        self.live_heap_size += size as u64;
        self.history_push();
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
        self.history_push();
    }

    fn free_profiler(&mut self, ptr: Ptr) {
        debug!("heap_profiler: free({})", ptr);
        if ptr != 0 {
            match self.memory.remove(&ptr) {
                Some(size) => {
                    self.live_heap_size -= size as u64;
                    self.history_push();
                }
                None => error!("heap_profiler: can't free, pointer {} doesn't exist", ptr),
            };
        }
    }
}
