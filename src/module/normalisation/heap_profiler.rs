use walrus::*;

/// FIXME: add docs
pub fn patch(module: &mut Module) {
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

    // FIXME: don't panic if malloc is not found
    // add extra line to guest malloc
    let malloc_id = module.funcs.by_name("malloc").unwrap();
    let malloc_function = module
        .funcs
        .iter_local_mut()
        .filter(|(id, _)| id == &malloc_id)
        .next()
        .unwrap()
        .1;
    let malloc_args = malloc_function.args[0];
    malloc_function
        .builder_mut()
        .func_body()
        .call(local_peak)
        .local_get(malloc_args)
        .call(malloc_profiler);

    // FIXME: don't panic if free is not found
    // add extra line to guest free
    let free_id = module.funcs.by_name("free").unwrap();
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
        .local_get(free_args)
        .call(free_profiler);
}
