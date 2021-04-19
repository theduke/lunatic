use log::debug;
use std::collections::HashMap;
use walrus::*;

/// FIXME: add docs
/// FIXME: enable this patch only with runtime flag
pub fn patch(module: &mut Module) -> Result<()> {
    add_profiler_to(module, "malloc")?;
    add_profiler_to(module, "calloc")?;
    add_profiler_to(module, "realloc")?;
    add_profiler_to(module, "free")?;
    Ok(())
}

fn add_profiler_to(module: &mut Module, name: &str) -> Result<()> {
    // find local function in module
    let fn_id = module
        .funcs
        .by_name(name)
        .ok_or(anyhow::Error::msg(format!(
            "heap_profiler: '{}' was not found in wasm",
            name
        )))?;
    let types = module.types.params_results(module.funcs.get(fn_id).ty());
    let (params, results) = (types.0.to_vec(), types.1.to_vec());

    // Import profiler. Profilers don't return anything. Profilers last argument
    // is result from the original function. For example, local function "malloc(i32) -> u32"
    // will import profiler of type "malloc(i32, u32)".
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

    // create a clone of a function
    let fn_copy_id = clone_function(module, fn_id, Some(format!("{}_wrap", name)));

    let locals = &mut module.locals;
    // create new local params for wrapper function, old params are copied (see clone above) to new
    // function
    let local_vars: Vec<LocalId> = params.iter().map(|t| locals.add(*t)).collect();
    let rets: Vec<LocalId> = results.iter().map(|t| locals.add(*t)).collect();
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

    // save returned values from the above function
    rets.iter().for_each(|r| {
        instr_seq.local_set(*r);
    });

    // prepare args to call profiler function
    local_vars.iter().for_each(|l| {
        instr_seq.local_get(*l);
    });
    rets.iter().for_each(|r| {
        instr_seq.local_get(*r);
    });

    // call profiler function
    instr_seq.call(profiler_id);

    // return saved values from original function
    rets.iter().for_each(|r| {
        instr_seq.local_get(*r);
    });

    // modify wrapper function args
    module.funcs.get_mut(fn_id).kind.unwrap_local_mut().args = local_vars;
    Ok(())
}

fn clone_function(module: &mut Module, fn_id: FunctionId, name: Option<String>) -> FunctionId {
    let types = module.types.params_results(module.funcs.get(fn_id).ty());
    let (params, results) = (types.0.to_vec(), types.1.to_vec());

    let mut fn_builder = FunctionBuilder::new(&mut module.types, &params, &results);
    let fn_local_function = module.funcs.get(fn_id).kind.unwrap_local();
    if let Some(name) = name {
        fn_builder.name(name);
    }
    let mut fn_instr_seq = fn_builder.func_body();

    // copy instructions from fn_id to new function
    clone_rec(
        fn_local_function,
        fn_local_function.block(fn_local_function.entry_block()),
        &mut fn_instr_seq,
        &mut HashMap::new(),
    );
    let fn_copy_id = fn_builder.finish(fn_local_function.args.clone(), &mut module.funcs);

    // number of instructions in original/wrapper and copied function should be the same
    assert_eq!(
        module.funcs.get(fn_id).kind.unwrap_local().size(),
        module.funcs.get(fn_copy_id).kind.unwrap_local().size()
    );
    fn_copy_id
}

fn clone_rec(
    fn_loc: &LocalFunction,
    instrs: &ir::InstrSeq,
    instrs_clone: &mut InstrSeqBuilder,
    jmp_ids: &mut HashMap<ir::InstrSeqId, ir::InstrSeqId>,
) {
    jmp_ids.insert(instrs.id(), instrs_clone.id());
    instrs.instrs.iter().for_each(|(i, _)| match i {
        ir::Instr::Block(block) => {
            let block_instrs = fn_loc.block(block.seq);
            instrs_clone.block(block_instrs.ty, |block_clone| {
                clone_rec(fn_loc, block_instrs, block_clone, jmp_ids);
            });
        }
        ir::Instr::IfElse(if_else) => {
            let consequent_instrs = fn_loc.block(if_else.consequent);
            let jmp_ids_clone = &mut jmp_ids.clone();
            instrs_clone.if_else(
                consequent_instrs.ty,
                |consequent_clone| {
                    clone_rec(fn_loc, consequent_instrs, consequent_clone, jmp_ids);
                },
                |alternative_clone| {
                    clone_rec(
                        fn_loc,
                        fn_loc.block(if_else.alternative),
                        alternative_clone,
                        jmp_ids_clone,
                    );
                },
            );
        }
        ir::Instr::Loop(loop_) => {
            let loop_instrs = fn_loc.block(loop_.seq);
            instrs_clone.block(loop_instrs.ty, |loop_clone| {
                clone_rec(fn_loc, loop_instrs, loop_clone, jmp_ids);
            });
        }
        ir::Instr::Br(br) => {
            instrs_clone.br(jmp_ids[&br.block]);
        }
        ir::Instr::BrIf(br_if) => {
            instrs_clone.br_if(jmp_ids[&br_if.block]);
        }
        _ => {
            instrs_clone.instr(i.clone());
        }
    });
}
