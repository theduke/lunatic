use log::debug;
use walrus::*;

/// FIXME: add docs
/// FIXME: enable this patch only with runtime flag
pub fn patch(module: &mut Module) -> Result<()> {
    add_profiler_to(module, "malloc")?;
    //    add_profiler_to(module, "calloc")?;
    //    add_profiler_to(module, "realloc")?;
    add_profiler_to(module, "free")?;
    Ok(())
}

fn add_profiler_to(module: &mut Module, name: &str) -> Result<()> {
    let fn_id = module
        .funcs
        .by_name(name)
        .ok_or(anyhow::Error::msg(format!(
            "heap_profiler: '{}' was not found in wasm",
            name
        )))?;
    let types = module.types.params_results(module.funcs.get(fn_id).ty());
    let (params, results) = (types.0.to_vec(), types.1.to_vec());

    // profilers don't return anything
    let profiler_type = module
        .types
        .add(&[params.clone(), results.clone()].concat(), &[]);
    let profiler_id = module
        .add_import_func(
            "heap_profiler",
            &format!("{}_profiler", name),
            profiler_type,
        )
        .0;

    let mut fn_builder = FunctionBuilder::new(&mut module.types, &params, &results);
    let fn_local_function = module.funcs.get(fn_id).kind.unwrap_local();
    fn_builder.name(format!("{}_wrap", name));
    let mut fn_instr_seq = fn_builder.func_body();

    // copy instructions from fn_id to new function
    clone_rec(
        fn_local_function,
        fn_local_function.block(fn_local_function.entry_block()),
        &mut fn_instr_seq,
    );
    let fn_copy_id = fn_builder.finish(fn_local_function.args.clone(), &mut module.funcs);

    // number of instructions in original/wrapper and copied function should be the same
    assert_eq!(
        module.funcs.get(fn_id).kind.unwrap_local().size(),
        module.funcs.get(fn_copy_id).kind.unwrap_local().size()
    );

    let locals = &mut module.locals;
    // create new local params for wrapper function, old params are copied (see clone above) to new
    // function
    let local_vars: Vec<LocalId> = params.iter().map(|t| locals.add(*t)).collect();
    let mut instr_seq = module
        .funcs
        .get_mut(fn_id)
        .kind
        .unwrap_local_mut()
        .builder_mut()
        .func_body();

    // remove all instructions from wrapper function (they are copied over to new function)
    *instr_seq.instrs_mut() = vec![];

    // prepare args to call new function
    local_vars.iter().for_each(|l| {
        instr_seq.local_get(*l);
    });

    // call new copied function from wrapper function
    instr_seq.call(fn_copy_id);

    // modify wrapper function args
    module.funcs.get_mut(fn_id).kind.unwrap_local_mut().args = local_vars;
    Ok(())
}

fn clone_rec(fn_loc: &LocalFunction, instrs: &ir::InstrSeq, instrs_clone: &mut InstrSeqBuilder) {
    instrs.instrs.iter().for_each(|(i, _)| match i {
        ir::Instr::Block(block) => {
            let block_instrs = fn_loc.block(block.seq);
            instrs_clone.block(block_instrs.ty, |block_clone| {
                clone_rec(fn_loc, block_instrs, block_clone);
            });
        }
        ir::Instr::IfElse(if_else) => {
            let consequent_instrs = fn_loc.block(if_else.consequent);
            instrs_clone.if_else(
                consequent_instrs.ty,
                |consequent_clone| {
                    clone_rec(fn_loc, consequent_instrs, consequent_clone);
                },
                |alternative_clone| {
                    clone_rec(fn_loc, fn_loc.block(if_else.alternative), alternative_clone);
                },
            );
        }
        ir::Instr::Loop(loop_) => {
            let loop_instrs = fn_loc.block(loop_.seq);
            instrs_clone.block(loop_instrs.ty, |loop_clone| {
                clone_rec(fn_loc, loop_instrs, loop_clone);
            });
        }
        _ => {
            instrs_clone.instr(i.clone());
        }
    });
}
