#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

mod ast;
mod checker;
mod emitter;
mod error;
mod lowering;
mod module_loader;
mod parser;
mod parser_peg;
mod policy;

use std::path::Path;

pub use ast::{
    BinaryOp, BindingPattern, ClosureCapture, DoStep, Expr, ImplMethod, MatchArm, Param, Program,
    Statement, TraitMethod, Type, TypeParam, VariantDecl,
};
pub use checker::{type_check, type_check_with_policy};
pub use error::CompileError;
pub use parser::parse;
pub use policy::ExecutionPolicy;

pub fn compile_source(source: &str) -> Result<String, CompileError> {
    compile_source_with_policy(source, &ExecutionPolicy::deny_all())
}

pub fn compile_source_with_policy(
    source: &str,
    policy: &ExecutionPolicy,
) -> Result<String, CompileError> {
    let program = parse(source)?;
    compile_program(&program, policy)
}

pub fn compile_file(path: &Path) -> Result<String, CompileError> {
    compile_file_with_policy(path, &ExecutionPolicy::deny_all())
}

pub fn compile_file_with_policy(
    path: &Path,
    policy: &ExecutionPolicy,
) -> Result<String, CompileError> {
    let program = module_loader::load_program(path)?;
    compile_program(&program, policy)
}

fn compile_program(program: &Program, policy: &ExecutionPolicy) -> Result<String, CompileError> {
    let program = checker::type_check_and_lower_with_policy(program, policy)?;
    Ok(emitter::transpile_with_policy(&program, policy))
}
