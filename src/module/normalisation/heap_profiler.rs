use walrus::*;

/// FIXME: add docs
/// FIXME: enable this patch only with runtime flag
pub fn patch(module: &mut Module) -> Result<()> {
    let namespace = "heap_profiler";

    // add malloc import
    let malloc_profiler_type = module.types.add(&[ValType::I32, ValType::I32], &[]);
    let (malloc_profiler, _) =
        module.add_import_func(namespace, "malloc_profiler", malloc_profiler_type);

    // add free import
    let free_profiler_type = module.types.add(&[ValType::I32], &[]);
    let (free_profiler, _) = module.add_import_func(namespace, "free_profiler", free_profiler_type);

    // add utility function that inserts the same value to the stack
    let mut double_stack = FunctionBuilder::new(
        &mut module.types,
        &[ValType::I32],
        &[ValType::I32, ValType::I32],
    );
    let val = module.locals.add(ValType::I32);
    double_stack.func_body().local_get(val).local_get(val);
    // local_peak iserts the same value that is on top of the stack
    let local_peak = double_stack.finish(vec![val], &mut module.funcs);

    let malloc_id = module.funcs.by_name("dlmalloc").ok_or(anyhow::Error::msg(
        "heap_profiler: 'dlmalloc' was not found in wasm",
    ))?;
    let malloc_function = module
        .funcs
        .iter_local_mut()
        .filter(|(id, _)| id == &malloc_id)
        .next()
        .unwrap()
        .1;
    let malloc_args = malloc_function.args[0];
    let my_malloc_args = module.locals.add(ValType::I32);
    malloc_function
        .builder_mut()
        .func_body()
        .local_get_at(0, malloc_args)
        .local_set_at(1, my_malloc_args)
        .call(local_peak)
        .local_get(my_malloc_args)
        // NOTE: this works only because there is no 'return' in dlmalloc
        // TODO: check for 'return's
        .call(malloc_profiler);

    let free_id = module.funcs.by_name("dlfree").ok_or(anyhow::Error::msg(
        "heap_profiler: 'dlfree' was not found in wasm",
    ))?;

    let free_function = module
        .funcs
        .iter_local_mut()
        .filter(|(id, _)| id == &free_id)
        .next()
        .unwrap()
        .1;
    let free_args = free_function.args[0];
    free_function
        .builder_mut()
        .func_body()
        .local_get_at(0, free_args)
        .call_at(1, free_profiler);
    Ok(())
}
