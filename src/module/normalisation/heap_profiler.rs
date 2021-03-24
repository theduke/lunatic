use walrus::*;

/// FIXME: add docs
pub fn patch(module: &mut Module) {
    let namespace = "heap_profiler";

    // add malloc import
    let malloc_type = module
        .types
        .add(&[ValType::I32, ValType::I32], &[ValType::I32]);
    let (malloc_counter_id, _) = module.add_import_func(namespace, "malloc_counter", malloc_type);

    // add free import
    let free_type = module.types.add(&[ValType::I32], &[]);
    let (free_counter_id, _) = module.add_import_func(namespace, "free_counter", free_type);

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
        .local_get(malloc_args)
        .call(malloc_counter_id);

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
        .call(free_counter_id);
}
