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

    let malloc_id = module.funcs.by_name("malloc").ok_or(anyhow::Error::msg(
        "heap_profiler: 'malloc' was not found in wasm",
    ))?;
    let malloc_function = module
        .funcs
        .iter_local_mut()
        .filter(|(id, _)| id == &malloc_id)
        .next()
        .unwrap()
        .1;
    let malloc_args = malloc_function.args[0];
    let mut malloc_func_body = malloc_function.builder_mut().func_body();
    let my_malloc_args = module.locals.add(ValType::I32);
    let my_malloc_ret = module.locals.add(ValType::I32);
    // save malloc args to local var
    malloc_func_body
        .local_get_at(0, malloc_args)
        .local_set_at(1, my_malloc_args);
    // find return instruction indexes
    let return_indexes: Vec<usize> = malloc_func_body
        .instrs()
        .iter()
        .enumerate()
        .filter(|(_, (instr, _))| match instr {
            ir::Instr::Return(_) => true,
            _ => false,
        })
        .map(|(i, _)| i)
        .collect();
    // insert malloc profiler at the specific position
    let l = malloc_func_body.instrs().len();
    let mut insert_malloc_profiler_at = |i: usize| {
        malloc_func_body
            .call_at(i, malloc_profiler)
            .local_get_at(i, my_malloc_ret)
            .local_get_at(i, my_malloc_args)
            .local_tee_at(i, my_malloc_ret);
    };
    // call malloc profiler at the end of the function
    insert_malloc_profiler_at(l);
    // call malloc profiler before every return instruction
    return_indexes
        .iter()
        .rev()
        .for_each(|i| insert_malloc_profiler_at(*i));

    let free_id = module.funcs.by_name("free").ok_or(anyhow::Error::msg(
        "heap_profiler: 'free' was not found in wasm",
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
