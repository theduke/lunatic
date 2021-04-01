use walrus::*;

/// FIXME: add docs
/// FIXME: enable this patch only with runtime flag
pub fn patch(module: &mut Module) -> Result<()> {
    add_profiler_to(module, "malloc")?;
    add_profiler_to(module, "calloc")?;
    add_profiler_to(module, "realloc")?;
    add_profiler_to(module, "free")
}

fn add_profiler_to(module: &mut Module, name: &str) -> Result<()> {
    let function_id = module
        .funcs
        .by_name(name)
        .ok_or(anyhow::Error::msg(format!(
            "heap_profiler: '{}' was not found in wasm",
            name
        )))?;
    let function = &module
        .funcs
        .iter_local()
        .filter(|(id, _)| id == &function_id)
        .next()
        .unwrap()
        .1;

    let function_args = function.args.clone();
    let arg_types = module.types.params(function.ty()).to_vec();
    let ret_types = module.types.results(function.ty()).to_vec();

    let profiler_type = module
        .types
        .add(&[arg_types.clone(), ret_types.clone()].concat(), &[]);
    let (profiler, _) = module.add_import_func(
        "heap_profiler",
        &format!("{}_profiler", name),
        profiler_type,
    );

    let function_body = &mut module
        .funcs
        .iter_local_mut()
        .filter(|(id, _)| id == &function_id)
        .next()
        .unwrap()
        .1
        .builder_mut()
        .func_body();

    let locs = &mut module.locals;
    let local_vars: Vec<LocalId> = arg_types.iter().map(|t| locs.add(*t)).collect();

    // save function args to local var
    local_vars
        .iter()
        .zip(function_args)
        .for_each(|(local_var, func_arg)| {
            function_body
                .local_get_at(0, func_arg)
                .local_set_at(1, *local_var);
        });
    let return_val = match ret_types[..] {
        [t] => {
            // we assume there is only one return
            Some(module.locals.add(t))
        }
        _ => None,
    };

    // find return instruction indexes
    let return_indexes: Vec<usize> = function_body
        .instrs()
        .iter()
        .enumerate()
        .filter(|(_, (instr, _))| match instr {
            ir::Instr::Return(_) => true,
            _ => false,
        })
        .map(|(i, _)| i)
        .collect();

    let end_index = function_body.instrs().len();
    // Insert profiler function call at specific position
    let mut insert_profiler_at = |pos: usize| {
        let insert_local_args = |body: &mut InstrSeqBuilder| {
            local_vars.iter().rev().for_each(|var| {
                body.local_get_at(pos, *var);
            })
        };
        match return_val {
            None => {
                function_body.call_at(pos, profiler);
                insert_local_args(function_body);
            }
            Some(ret_val) => {
                function_body.call_at(pos, profiler);
                function_body.local_get_at(pos, ret_val);
                insert_local_args(function_body);
                function_body.local_tee_at(pos, ret_val);
            }
        }
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
