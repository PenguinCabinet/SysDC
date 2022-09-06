use crate::name::Name;
use crate::error::{ PResult, PErrorKind };
use crate::structure::{ SysDCSystem, SysDCFunction, SysDCSpawn, SysDCSpawnChild };
use super::utils::define::DefinesManager;

pub struct TypeMatchChecker<'a> {
    def_manager: &'a DefinesManager,
    imports: &'a Vec<Name>
}

impl<'a> TypeMatchChecker<'a> {
    pub fn check(system: &SysDCSystem, def_manager: &'a DefinesManager, imports: &'a Vec<Name>) -> PResult<()> {
        let checker = TypeMatchChecker{ def_manager, imports };
        for unit in &system.units {
            for module in &unit.modules {
                for func in &module.functions {
                    checker.check_function(&func)?;
                }
            }
        }
        Ok(())
    }

    fn check_function(&self, func: &SysDCFunction) -> PResult<()> {
        let req_ret_type = &func.returns.as_ref().unwrap().1;
        let act_ret_type = self.def_manager.resolve_from_name(func.returns.clone().unwrap().0, &self.imports)?.1;
        if req_ret_type != &act_ret_type {
            return PErrorKind::TypeUnmatch2(req_ret_type.clone(), act_ret_type).to_err();
        }
        for spawn in &func.spawns {
            self.check_spawn(&spawn)?;
        }
        Ok(())
    }

    fn check_spawn(&self, spawn: &SysDCSpawn) -> PResult<()> {
        for child in &spawn.details {
            self.check_spawn_child(child)?;
            match child {
                SysDCSpawnChild::Return(_, act_ret_type) =>
                    if &spawn.result.1 != act_ret_type {
                        return PErrorKind::TypeUnmatch2(spawn.result.1.clone(), act_ret_type.clone()).to_err();
                    }
                _ => {}
            }
        }
        Ok(())
    }

    fn check_spawn_child(&self, spawn_child: &SysDCSpawnChild) -> PResult<()> {
        match &spawn_child {
            SysDCSpawnChild::LetTo { func: (func, _), args, .. } => {
                for ((_, act_arg_type), req_arg_type) in args.iter().zip(self.def_manager.get_args_type(&func, &self.imports)?.iter()) {
                    if act_arg_type != req_arg_type {
                        return PErrorKind::TypeUnmatch2(req_arg_type.clone(), act_arg_type.clone()).to_err();
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}