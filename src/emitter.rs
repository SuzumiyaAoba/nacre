mod arithmetic;
mod async_values;
mod bindings;
mod collections;
mod commands;
mod control_flow;
mod expressions;
mod filesystem;
mod json;
mod local_mangling;
mod match_analysis;
mod matching;
mod option_result;
mod quoting;
mod runtime;
mod shell;
mod strings;
mod values;

use self::bindings::{emit_assignment_target, emit_binding, emit_destructure};
use self::collections::{emit_array_pop, emit_array_push, emit_map_remove, emit_map_set};
use self::commands::{
    emit_allowed_command, emit_allowed_command_result_discard, emit_redirect, emit_require,
    emit_require_one_of,
};
use self::control_flow::{
    emit_block, emit_block_with_defers, emit_deferred_statements, emit_for, emit_if_with_defers,
    emit_return, emit_try_command_result, emit_try_result, emit_while,
};
use self::expressions::{emit_call_arg, emit_call_head, emit_expr};
use self::filesystem::{emit_fs_append_lines_statement, emit_fs_write_lines_statement};
use self::quoting::emit_shell_word;
use self::runtime::{emit_policy_runtime, program_needs_runtime, CLOSURE_RUNTIME};
use self::shell::{emit_pipeline, emit_shell_command};
use crate::lowering::lower_method_calls;
use crate::policy::ExecutionPolicy;
use crate::symbols::{is_identifier, shell_function_name};
use crate::{Expr, Param, Program, Statement};

#[derive(Clone, Copy)]
struct AllowedCommandParts<'a> {
    program: Option<&'a str>,
    args: &'a [Expr],
    read_args: &'a [usize],
    write_args: &'a [usize],
}

impl<'a> AllowedCommandParts<'a> {
    fn new(
        program: Option<&'a str>,
        args: &'a [Expr],
        read_args: &'a [usize],
        write_args: &'a [usize],
    ) -> Self {
        Self {
            program,
            args,
            read_args,
            write_args,
        }
    }
}

#[cfg(test)]
pub(crate) fn transpile(program: &Program) -> String {
    transpile_with_policy(program, &ExecutionPolicy::deny_all())
}

pub(crate) fn transpile_with_policy(program: &Program, policy: &ExecutionPolicy) -> String {
    let program = lower_method_calls(program);
    let program = local_mangling::mangle_function_locals(&program);
    let mut out = String::from("#!/usr/bin/env bash\nset -euo pipefail\nargs=(\"$@\")\n");
    emit_policy_runtime(&mut out, policy);
    if program_needs_runtime(&program) {
        out.push_str(CLOSURE_RUNTIME);
    }
    for statement in program.statements() {
        out.push('\n');
        emit_statement(&mut out, statement, EmitScope::TopLevel, &[]);
    }
    out
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum EmitScope {
    TopLevel,
    Function,
    TopLevelLoop,
    FunctionLoop,
}

impl EmitScope {
    fn is_function(self) -> bool {
        matches!(self, Self::Function | Self::FunctionLoop)
    }

    fn is_loop(self) -> bool {
        matches!(self, Self::TopLevelLoop | Self::FunctionLoop)
    }

    fn loop_body(self) -> Self {
        if self.is_function() {
            Self::FunctionLoop
        } else {
            Self::TopLevelLoop
        }
    }
}

fn emit_statement(out: &mut String, statement: &Statement, scope: EmitScope, defers: &[Statement]) {
    match statement {
        Statement::Export(inner) => emit_statement(out, inner, scope, defers),
        Statement::Use { path, .. } => emit_use(out, path),
        Statement::ExternalFunction { .. } => {}
        Statement::Trait { .. } => {}
        Statement::Impl { methods, .. } => {
            for method in methods {
                emit_function(out, &method.name, &method.params, &method.body);
            }
        }
        Statement::InherentImpl {
            consts, methods, ..
        } => {
            for value in consts {
                emit_binding(
                    out,
                    &value.name,
                    &value.expr,
                    !scope.is_loop(),
                    scope.is_function(),
                );
            }
            for method in methods {
                emit_function(out, &method.name, &method.params, &method.body);
            }
        }
        Statement::TypeAlias { .. } => {}
        Statement::SumType { .. } => {}
        Statement::Newtype { .. } => {}
        Statement::Function {
            name, params, body, ..
        } => emit_function(out, name, params, body),
        Statement::Const { name, expr, .. } => {
            emit_binding(out, name, expr, !scope.is_loop(), scope.is_function());
        }
        Statement::Let { name, expr, .. } => {
            emit_binding(out, name, expr, false, scope.is_function());
        }
        Statement::Destructure {
            mutable,
            pattern,
            expr,
        } => emit_destructure(
            out,
            pattern,
            expr,
            !mutable && !scope.is_loop(),
            scope.is_function(),
        ),
        Statement::Assign { target, expr } => {
            emit_assignment_target(out, target, expr);
        }
        Statement::Expr(expr) => emit_expr_statement(out, expr),
        Statement::TryCommand(command) => {
            emit_shell_command(out, command);
            out.push_str(" || exit $?\n");
        }
        Statement::TryCommandResult(command) => emit_try_command_result(out, command, scope),
        Statement::TryResult(expr) => emit_try_result(out, expr, scope),
        Statement::TryPipeline { input, commands } => {
            emit_pipeline(out, input.as_deref(), commands);
            out.push_str(" || exit $?\n");
        }
        Statement::TryPipelineResult { input, commands } => {
            let mut command = String::new();
            emit_pipeline(&mut command, input.as_deref(), commands);
            emit_try_command_result(out, &command, scope);
        }
        Statement::Command(command) => {
            out.push_str(command);
            out.push('\n');
        }
        Statement::Redirect {
            command,
            target,
            stderr,
            append,
        } => emit_redirect(out, command, target, stderr.as_deref(), *append),
        Statement::Require { command, version } => emit_require(out, command, version.as_deref()),
        Statement::RequireOneOf { commands } => emit_require_one_of(out, commands),
        Statement::Block { body } => emit_block_with_defers(out, body, scope, defers),
        Statement::Defer(_) => {}
        Statement::If {
            condition,
            then_branch,
            else_branch,
        } => emit_if_with_defers(
            out,
            condition,
            then_branch,
            else_branch.as_ref(),
            scope,
            defers,
        ),
        Statement::While { condition, body } => emit_while(out, condition, body, scope),
        Statement::For {
            binding,
            iterable,
            body,
        } => emit_for(out, binding, iterable, body, scope),
        Statement::Break => {
            emit_deferred_statements(out, defers, scope);
            out.push_str("break\n");
        }
        Statement::Continue => {
            emit_deferred_statements(out, defers, scope);
            out.push_str("continue\n");
        }
        Statement::Return(expr) => {
            emit_deferred_statements(out, defers, scope);
            emit_return(out, expr);
        }
        Statement::Raw(raw) => out.push_str(raw),
    }
}

fn emit_expr_statement(out: &mut String, expr: &Expr) {
    match expr {
        Expr::ArrayPush { name, value } => emit_array_push(out, name, value),
        Expr::ArrayPop { name } => emit_array_pop(out, name),
        Expr::MapSet { name, key, value } => emit_map_set(out, name, key, value),
        Expr::MapRemove { name, key } => emit_map_remove(out, name, key),
        Expr::FsWriteLines { path, lines } => emit_fs_write_lines_statement(out, path, lines),
        Expr::FsAppendLines { path, lines } => emit_fs_append_lines_statement(out, path, lines),
        Expr::AllowedCommand {
            program,
            args,
            result,
            read_args,
            write_args,
            ..
        } => {
            if *result {
                emit_allowed_command_result_discard(
                    out,
                    AllowedCommandParts::new(program.as_deref(), args, read_args, write_args),
                );
            } else {
                emit_allowed_command(out, program.as_deref(), args, read_args, write_args, false);
            }
            out.push('\n');
        }
        Expr::Call { name, args } => {
            emit_call_head(out, name);
            for arg in args {
                out.push(' ');
                emit_call_arg(out, arg);
            }
            out.push('\n');
        }
        _ => {
            emit_expr(out, expr);
            out.push('\n');
        }
    }
}

fn emit_use(out: &mut String, path: &[String]) {
    out.push_str("source \"$(dirname \"$0\")/");
    out.push_str(&path.join("/"));
    out.push_str(".sh\"\n");
}

fn emit_function(out: &mut String, name: &str, params: &[Param], body: &Program) {
    let shell_name = shell_function_name(name);
    out.push_str(&shell_name);
    out.push_str("() {\n");
    let mut position = 1;
    for param in params {
        if param.capture_name.is_some() {
            continue;
        }
        if param.variadic {
            out.push_str("local -a ");
            out.push_str(&param.name);
            out.push_str("=(\"${@:");
            out.push_str(&position.to_string());
            out.push_str("}\")\n");
        } else if let Some(default) = &param.default {
            out.push_str("if [ \"$#\" -ge ");
            out.push_str(&position.to_string());
            out.push_str(" ]; then\nlocal ");
            out.push_str(&param.name);
            out.push_str("=\"$");
            out.push_str(&position.to_string());
            out.push_str("\"\nelse\nlocal ");
            out.push_str(&param.name);
            out.push('=');
            emit_expr(out, default);
            out.push_str("\nfi\n");
        } else {
            out.push_str("local ");
            out.push_str(&param.name);
            out.push_str("=\"$");
            out.push_str(&position.to_string());
            out.push_str("\"\n");
        }
        position += 1;
    }
    emit_block(out, body, EmitScope::Function);
    out.push_str("}\n");
    if is_identifier(name) {
        out.push_str("readonly ");
        out.push_str(name);
        out.push('=');
        emit_shell_word(out, &shell_name);
        out.push('\n');
    }
}

#[cfg(test)]
mod tests;
