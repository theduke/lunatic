#![feature(available_concurrency)]

use anyhow::Result;
use easy_parallel::Parallel;

use clap::{crate_version, Clap};
use lunatic_runtime::api::process::{FunctionLookup, MemoryChoice, Process, EXECUTOR};
use lunatic_runtime::module;

use std::fs;
use std::thread;

#[cfg(all(feature = "vm-wasmer", target_family = "unix"))]
use wasmer_vm::traphandlers::setup_unix_sigaltstack;
#[cfg(all(feature = "vm-wasmtime", target_family = "unix"))]
use wasmtime_runtime::traphandlers::setup_unix_sigaltstack;

#[derive(Clap)]
#[clap(version = crate_version!())]
struct Opts {
    /// .wasm file
    input: String,
    /// All other arguments are forwarded to the .wasm file
    #[clap(min_values(0))]
    _args: Vec<String>,
    /// Save heap profile to heap.dat
    #[clap(short, long)]
    profile: bool,
    /// Output patched/normalised wasm to normalised.wasm
    #[clap(short, long)]
    normalised_out: bool,
}

pub fn run() -> Result<()> {
    let opts: Opts = Opts::parse();
    let is_profile = opts.profile;
    let is_normalised_out = opts.normalised_out;

    let wasm = fs::read(opts.input).expect("Can't open .wasm file");

    let module = module::LunaticModule::new(&wasm, is_profile, is_normalised_out)?;

    // Set up async runtime
    let cpus = thread::available_concurrency().unwrap();
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    Parallel::new()
        .each(0..cpus.into(), |_| {
            // Extend the signal stack on all execution threads
            #[cfg(target_family = "unix")]
            setup_unix_sigaltstack().unwrap();
            smol::future::block_on(EXECUTOR.run(shutdown.recv()))
        })
        .finish(|| {
            smol::future::block_on(async {
                let result = Process::create(
                    None,
                    module,
                    FunctionLookup::Name("_start"),
                    MemoryChoice::New(None),
                    is_profile,
                )
                .await;
                drop(signal);
                result
            })
        })
        .1
        .map_err(|e| e.error)?;

    Ok(())
}

fn main() -> Result<()> {
    env_logger::init();
    run()
}
