use walrus::*;

/// FIXME: add docs
pub fn patch(module: &mut Module) {
    let void_type = module.types.add(&[], &[]);
    // add malloc import
    let (malloc_counter_id, _) = module.add_import_func("env", "malloc_counter", void_type);

    // add free import
    let (free_counter_id, _) = module.add_import_func("env", "free_counter", void_type);

    // FIXME: don't panic if malloc is not found
    // add extra line to guest malloc
    let malloc_id = module.funcs.by_name("malloc").unwrap();
    module
        .funcs
        .iter_local_mut()
        .filter(|(id, _)| id == &malloc_id)
        .next()
        .unwrap()
        .1
        .builder_mut()
        .func_body()
        .call_at(0, malloc_counter_id);

    // FIXME: don't panic if free is not found
    // add extra line to guest free
    let free_id = module.funcs.by_name("free").unwrap();
    module
        .funcs
        .iter_local_mut()
        .filter(|(id, _)| id == &free_id)
        .next()
        .unwrap()
        .1
        .builder_mut()
        .func_body()
        .call_at(0, free_counter_id);
}
