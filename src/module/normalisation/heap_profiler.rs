use walrus::*;

/// FIXME: add docs
/// FIXME: enable this patch only with runtime flag
pub fn patch(module: &mut Module) -> Result<()> {
    add_profiler_to(module, "malloc")?;
    add_profiler_to(module, "calloc")?;
    add_profiler_to(module, "free")?;

    Ok(())
}

fn add_profiler_to(module: &mut Module, name: &str) -> Result<()> {
    let target_function_id = module
        .funcs
        .by_name(name)
        .ok_or(anyhow::Error::msg(format!(
            "heap_profiler: '{}' was not found in wasm",
            name
        )))?;
    let target_function = module
        .funcs
        .iter_local()
        .filter(|(id, _)| id == &target_function_id)
        .next()
        .unwrap()
        .1;
    let args_len = target_function.args.len();
    let rets_len = module.types.results(target_function.ty()).len();

    // we assume args and returns are I32
    let profiler_type = module
        .types
        .add(&vec![ValType::I32; args_len + rets_len], &[]);
    let (profiler, _) = module.add_import_func(
        "heap_profiler",
        &format!("{}_profiler", name),
        profiler_type,
    );

    let target_function = module
        .funcs
        .iter_local_mut()
        .filter(|(id, _)| id == &target_function_id)
        .next()
        .unwrap()
        .1;

    // we asume args are I32 type
    let local_vars: Vec<LocalId> = std::iter::repeat(module.locals.add(ValType::I32))
        .take(args_len)
        .collect();

    // save function args to local var
    local_vars
        .iter()
        .zip(0..)
        .for_each(|(local_var, func_ind)| {
            let func_arg = target_function.args[func_ind];
            target_function
                .builder_mut()
                .func_body()
                .local_get_at(0, func_arg)
                .local_set_at(1, *local_var);
        });
    let return_val = match rets_len {
        0 => None,
        _ => {
            // we assume return is I32 type
            // we assume there is only one return
            Some(module.locals.add(ValType::I32))
        }
    };

    // find return instruction indexes
    let return_indexes: Vec<usize> = target_function
        .builder_mut()
        .func_body()
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
    let end_index = target_function.builder_mut().func_body().instrs().len();
    // Insert profiler function call at specific position
    let mut insert_profiler_at = |pos: usize| {
        let mut body = target_function.builder_mut().func_body();
        //fn insert_local_args(pos: usize, body: &mut InstrSeqBuilder, local_vars: Vec<LocalId>) {
        //    local_vars.iter().rev().for_each(|var| {
        //        body.local_get_at(pos, *var);
        //    })
        //}
        match return_val {
            None => {
                body.call_at(pos, profiler);
                local_vars.iter().rev().for_each(|var| {
                    body.local_get_at(pos, *var);
                })
            }
            Some(ret_val) => {
                body.call_at(pos, profiler);
                body.local_get_at(pos, ret_val);
                local_vars.iter().rev().for_each(|var| {
                    body.local_get_at(pos, *var);
                });
                body.local_tee_at(pos, ret_val);
            }
        }
        //.call_at(i, profiler);
        //.local_get_at(i, my_malloc_ret)
        //.local_get_at(i, my_malloc_args)
        //.local_tee_at(i, my_malloc_ret);
    };

    // call profiler at the end of the function
    insert_profiler_at(end_index);
    // call profiler before every return instruction
    return_indexes
        .iter()
        .rev()
        .for_each(|i| insert_profiler_at(*i));
    Ok(())
}
