use super::type_utils::{option_element_type, result_types};
use super::{Binding, CapturedBindings, DoFamily, TypeChecker};
use crate::value_layout::value_suffixes;
use crate::{ClosureCapture, CompileError, DoStep, Expr, Param, Program, Statement, Type};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

impl TypeChecker {
    pub(super) fn check_lambda(
        &self,
        params: &[String],
        body: &Expr,
        expected: &Type,
        line: usize,
    ) -> Result<Type, CompileError> {
        let Type::Function(param_types, return_type) = expected else {
            return Err(CompileError::new(
                line,
                format!("lambda requires a function type, found {}", expected.name()),
            ));
        };
        if params.len() != param_types.len() {
            return Err(CompileError::new(
                line,
                format!(
                    "lambda expects {} parameters from its function type, found {}",
                    param_types.len(),
                    params.len()
                ),
            ));
        }
        let (mut checker, _) = self.lambda_checker(params, param_types, line)?;
        checker.expected_return = Some((**return_type).clone());
        let body = Program::new(vec![Statement::Return(body.clone())], vec![line]);
        let body = checker.expand_nested_try_program(&body);
        if let Err(error) = checker.check_program(&body) {
            if let Some(message) = error.message().strip_prefix("return type mismatch: ") {
                return Err(CompileError::new(
                    error.line(),
                    format!("lambda return type mismatch: {message}"),
                ));
            }
            return Err(error);
        }
        Ok(expected.clone())
    }

    pub(super) fn lower_lambda(
        &self,
        params: &[String],
        body: &Expr,
        expected: &Type,
        line: usize,
    ) -> Result<Expr, CompileError> {
        let Type::Function(param_types, return_type) = expected else {
            return Err(CompileError::new(
                line,
                format!("lambda requires a function type, found {}", expected.name()),
            ));
        };
        let (mut checker, captured_bindings) = self.lambda_checker(params, param_types, line)?;
        checker.expected_return = Some((**return_type).clone());
        let body = Program::new(vec![Statement::Return(body.clone())], vec![line]);
        let body = checker.check_and_lower_program(&body)?;
        let mut captures = captured_bindings
            .borrow()
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        captures.sort();
        for capture in &captures {
            self.record_capture(capture);
        }
        let mut index = self.next_lambda.get();
        let name = loop {
            let candidate = format!("__nacre_lambda_{index}");
            index += 1;
            if !self.functions.contains_key(&candidate) {
                break candidate;
            }
        };
        self.next_lambda.set(index);
        let mut capture_params = Vec::with_capacity(captures.len());
        let mut closure_captures = Vec::with_capacity(captures.len());
        for (capture_index, source) in captures.iter().enumerate() {
            let binding = self.bindings.get(source).cloned().or_else(|| {
                (source == "args").then(|| Binding {
                    ty: Type::Array(Box::new(Type::String)),
                    mutable: false,
                })
            });
            let Some(binding) = binding else {
                return Err(CompileError::new(
                    line,
                    format!("undefined captured variable `{source}`"),
                ));
            };
            let target = format!("__nacre_capture_{name}_{capture_index}");
            capture_params.push(Param {
                name: source.clone(),
                ty: binding.ty.clone(),
                default: None,
                variadic: false,
                capture_name: Some(target.clone()),
            });
            closure_captures.push(ClosureCapture {
                source: source.clone(),
                target,
                suffixes: value_suffixes(&binding.ty),
            });
        }
        let params = capture_params
            .into_iter()
            .chain(params.iter().zip(param_types).map(|(name, ty)| Param {
                name: name.clone(),
                ty: ty.clone(),
                default: None,
                variadic: false,
                capture_name: None,
            }))
            .collect::<Vec<_>>();
        self.generated_functions.borrow_mut().push((
            Statement::Function {
                name: name.clone(),
                override_constructor: false,
                type_params: Vec::new(),
                params,
                return_type: (**return_type).clone(),
                body,
            },
            line,
        ));
        if captures.is_empty() {
            Ok(Expr::Ident(name))
        } else {
            Ok(Expr::Closure {
                name,
                captures: closure_captures,
            })
        }
    }

    pub(super) fn lambda_checker(
        &self,
        params: &[String],
        param_types: &[Type],
        line: usize,
    ) -> Result<(Self, CapturedBindings), CompileError> {
        let mut checker = self.clone();
        checker.expected_return = None;
        checker.loop_depth = 0;
        let captured_bindings = Rc::new(RefCell::new(HashSet::new()));
        checker.captured_bindings = Some(captured_bindings.clone());
        checker.capture_params = params.iter().cloned().collect();
        for (name, ty) in params.iter().zip(param_types) {
            checker.bindings.remove(name);
            checker.define(name, ty.clone(), false, line)?;
        }
        Ok((checker, captured_bindings))
    }

    pub(super) fn desugar_do(
        &self,
        steps: &[DoStep],
        result: &Expr,
        line: usize,
    ) -> Result<Expr, CompileError> {
        let family = self.do_family(steps, result, line)?;
        let result = match result {
            Expr::Call { name, args } if name == "pure" => {
                let [value] = args.as_slice() else {
                    return Err(CompileError::new(
                        line,
                        format!("pure expects 1 argument, found {}", args.len()),
                    ));
                };
                match family {
                    DoFamily::Option => Expr::Some(Box::new(value.clone())),
                    DoFamily::Result => Expr::Ok(Box::new(value.clone())),
                }
            }
            result => result.clone(),
        };
        Ok(steps.iter().rev().fold(result, |body, step| match step {
            DoStep::Bind { name, expr } => Expr::OptionFlatMapValue {
                value: Box::new(expr.clone()),
                mapper: Box::new(Expr::Lambda {
                    params: vec![name.clone()],
                    body: Box::new(body),
                }),
            },
            DoStep::Let {
                name,
                annotation,
                expr,
            } => Expr::LetIn {
                name: name.clone(),
                annotation: annotation.clone(),
                value: Box::new(expr.clone()),
                body: Box::new(body),
            },
        }))
    }

    pub(super) fn do_family(
        &self,
        steps: &[DoStep],
        result: &Expr,
        line: usize,
    ) -> Result<DoFamily, CompileError> {
        let mut checker = self.clone();
        for step in steps {
            match step {
                DoStep::Let {
                    name,
                    annotation,
                    expr,
                } => {
                    let value_ty = checker.binding_expr_type(annotation.as_ref(), expr, line)?;
                    let binding_ty =
                        checker.check_annotation(annotation.clone(), value_ty, expr, line)?;
                    checker.define(name, binding_ty, false, line)?;
                }
                DoStep::Bind { expr, .. } => {
                    let ty = checker.check_expr(expr, line)?;
                    if option_element_type(&ty).is_some() {
                        return Ok(DoFamily::Option);
                    }
                    if result_types(&ty).is_some() {
                        return Ok(DoFamily::Result);
                    }
                    return Err(CompileError::new(
                        line,
                        format!("do binding expects Option or Result, found {}", ty.name()),
                    ));
                }
            }
        }
        if matches!(result, Expr::Call { name, .. } if name == "pure") {
            return Err(CompileError::new(
                line,
                "pure in do expression requires an Option or Result binding".to_string(),
            ));
        }
        let ty = checker.check_expr(result, line)?;
        if option_element_type(&ty).is_some() {
            Ok(DoFamily::Option)
        } else if result_types(&ty).is_some() {
            Ok(DoFamily::Result)
        } else {
            Err(CompileError::new(
                line,
                format!(
                    "do expression result must be Option or Result, found {}",
                    ty.name()
                ),
            ))
        }
    }
}
