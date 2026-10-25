use super::builders::{
    applied_type, apply_postfix, binary, binding_statement, collection, compound_assignment,
    decode_escaped, default_expr, do_expression, expression_statement, flatten_intersection,
    flatten_union, impl_method, match_constructor, named_or_value, named_type, pipeline,
    string_expr, trait_method,
};
use super::{
    BindingTarget, CollectionEntry, DoItem, FunctionHead, ImplItem, ParsedStatement, Postfix,
    TypeDeclaration,
};
use crate::{
    AssignTarget, BinaryOp, BindingPattern, Expr, ForBinding, ImplConst, ImplMethod, MatchArm,
    Param, Program, Statement, TraitMethod, Type, TypeParam, UseItem, VariantDecl,
};

peg::parser! {
    pub(super) grammar nacre_grammar() for str {
        rule ws() = quiet!{[' ' | '\t' | '\r' | '\n']*}
        rule comma() = ws() "," ws()

        rule identifier() -> String
            = value:$(['A'..='Z' | 'a'..='z' | '_']
                ['A'..='Z' | 'a'..='z' | '0'..='9' | '_']*)
            { value.to_string() }

        rule language_keyword()
            = ("as" / "async" / "await" / "break" / "const" / "continue" /
               "defer" / "do" / "else" / "export" / "false" / "fn" / "fn!" / "for" /
               "if" / "impl" / "in" / "let" / "match" / "newtype" / "raw" /
               "requireOneOf" / "require" / "return" / "spawn" / "trait" /
               "true" / "try" / "type" / "use" / "while") !identifier_continue()

        rule type_identifier() -> String
            = value:$(['A'..='Z']
                ['A'..='Z' | 'a'..='z' | '0'..='9' | '_']*)
            { value.to_string() }

        rule type_name() -> String
            = head:identifier() tail:(ws() "." ws() part:identifier() { part })+
            {
                let mut parts = Vec::with_capacity(tail.len() + 1);
                parts.push(head);
                parts.extend(tail);
                parts.join(".")
            }
            / name:type_identifier() { name }

        pub rule program_root() -> Vec<ParsedStatement>
            = file_trivia() statements:program_items() file_trivia() ![_] { statements }

        rule program_items() -> Vec<ParsedStatement>
            = statements:(located_statement() ** statement_separator()) { statements }

        rule located_statement() -> ParsedStatement
            = hws() offset:position!() statement:statement()
                { ParsedStatement { offset, statement } }

        rule statement() -> Statement
            = export_statement()
            / external_function_statement()
            / function_statement()
            / trait_statement()
            / impl_statement()
            / raw_statement()
            / if_statement()
            / while_statement()
            / for_statement()
            / defer_statement()
            / block_statement()
            / use_statement()
            / newtype_statement()
            / type_statement()
            / binding_statement_rule()
            / return_statement()
            / break_statement()
            / continue_statement()
            / require_one_of_statement()
            / require_statement()
            / redirect_statement()
            / assignment_statement()
            / expression_statement_rule()

        rule statement_separator()
            = line_tail() newline()+ file_trivia()

        rule file_trivia()
            = ((hws() comment()? newline()+) / shebang_line())*

        rule line_tail() = hws() comment()?
        rule newline() = "\r\n" / "\n"
        rule comment() = "##" (!newline() [_])*
        rule shebang_line() = hws() "#!" (!newline() [_])* newline()+

        rule block_body() -> Program
            = line_tail() newline()+ file_trivia()
              statements:program_items()? file_trivia() hws()
            {
                let statements = statements.unwrap_or_default();
                let lines = vec![1; statements.len()];
                Program::new(
                    statements.into_iter().map(|item| item.statement).collect(),
                    lines,
                )
            }

        rule external_function_statement() -> Statement
            = "export" hws1() "fn" hws1() head:function_signature()
            {
                Statement::ExternalFunction {
                    name: head.name,
                    type_params: head.type_params,
                    params: head.params,
                    return_type: head.return_type,
                }
            }

        rule export_statement() -> Statement
            = "export" hws1() statement:exportable_statement()
            { Statement::Export(Box::new(statement)) }

        rule exportable_statement() -> Statement
            = function_statement()
            / trait_statement()
            / impl_statement()
            / use_statement_inner(true)
            / newtype_statement()
            / type_statement()
            / binding_statement_rule()

        rule function_statement() -> Statement
            = marker:("fn!" { true } / "fn" { false }) hws1()
              head:function_signature_with_marker(marker) hws() "{"
              body:block_body() "}"
            {
                Statement::Function {
                    name: head.name,
                    override_constructor: head.override_constructor,
                    type_params: head.type_params,
                    params: head.params,
                    return_type: head.return_type,
                    body,
                }
            }

        rule function_signature_with_marker(override_constructor: bool) -> FunctionHead
            = name:identifier()
              type_params:type_params()?
              hws() "(" hws() params:(parameter() ** comma()) hws() ")"
              hws() ":" hws() return_type:type_expr()
            {
                FunctionHead {
                    name,
                    override_constructor,
                    type_params: type_params.unwrap_or_default(),
                    params,
                    return_type,
                }
            }

        rule function_signature() -> FunctionHead
            = function_signature_with_marker(false)

        rule type_params() -> Vec<TypeParam>
            = "[" ws() values:(type_param() ++ comma()) ws() "]" { values }

        rule type_param() -> TypeParam
            = name:type_identifier()
              bounds:(hws() ":" hws() values:(type_name() ++ (hws() "+" hws())) { values })?
            { TypeParam { name, bounds: bounds.unwrap_or_default() } }

        rule parameter() -> Param
            = name:identifier() hws() ":" hws()
              variadic:("..." { true })? ty:type_expr()
              default:(hws() "=" hws() value:expression() { value })?
            {
                Param {
                    name,
                    ty: if variadic.unwrap_or(false) {
                        Type::Array(Box::new(ty))
                    } else {
                        ty
                    },
                    default,
                    variadic: variadic.unwrap_or(false),
                    capture_name: None,
                }
            }

        rule trait_statement() -> Statement
            = "trait" hws1() name:type_identifier() hws()
              "[" hws() type_param:type_identifier() hws() "]"
              hws() "{" line_tail() newline()+ file_trivia()
              methods:(trait_method() ** statement_separator())? file_trivia() hws() "}"
            {
                Statement::Trait {
                    name,
                    type_param,
                    methods: methods.unwrap_or_default(),
                }
            }

        rule trait_method() -> TraitMethod
            = hws() "fn" hws1() head:function_signature()
            {? super::trait_method(head) }

        rule impl_statement() -> Statement
            = "impl" hws1() trait_name:type_identifier() hws()
              "[" hws() for_type:type_expr() hws() "]"
              hws() "{" line_tail() newline()+ file_trivia()
              methods:(impl_method() ** statement_separator())? file_trivia() hws() "}"
            {
                Statement::Impl {
                    trait_name,
                    for_type,
                    methods: methods.unwrap_or_default(),
                }
            }
            / "impl" hws1() for_type:type_expr()
              hws() "{" line_tail() newline()+ file_trivia()
              items:(impl_item() ** statement_separator())? file_trivia() hws() "}"
            {
                let mut consts = Vec::new();
                let mut methods = Vec::new();
                for item in items.unwrap_or_default() {
                    match item {
                        ImplItem::Const(value) => consts.push(value),
                        ImplItem::Method(value) => methods.push(value),
                    }
                }
                Statement::InherentImpl {
                    for_type,
                    consts,
                    methods,
                }
            }

        rule impl_item() -> ImplItem
            = value:impl_const() { ImplItem::Const(value) }
            / value:impl_method() { ImplItem::Method(value) }

        rule impl_const() -> ImplConst
            = hws() "const" hws1() name:identifier()
              annotation:(hws() ":" hws() ty:type_expr() { ty })?
              hws() "=" hws() expr:expression()
            { ImplConst { name, annotation, expr } }

        rule impl_method() -> ImplMethod
            = hws() "fn" hws1() head:function_signature() hws() "{"
              body:block_body() "}"
            {? super::impl_method(head, body) }

        rule raw_statement() -> Statement
            = "raw" hws() "{" line_tail() newline()
              value:$(raw_content()*) hws() "}"
            { Statement::Raw(value.to_string()) }

        rule raw_content()
            = raw_nested()
            / !(hws() "}" line_tail() (&newline() / ![_])) raw_line()

        rule raw_nested()
            = hws() "raw" hws() "{" line_tail() newline()
              raw_content()* hws() "}" line_tail() newline()?

        rule raw_line()
            = (!newline() [_])+ newline()?
            / newline()

        rule if_statement() -> Statement
            = "if" hws1() condition:expression() hws() "{"
              then_branch:block_body() "}"
              else_branch:else_statement_clause()?
            { Statement::If { condition, then_branch, else_branch } }

        rule else_statement_clause() -> Program
            = ((line_tail() newline()+ file_trivia() hws()) / hws())
              "else" hws()
              statement:(if_statement() / ("{" body:block_body() "}" { Statement::Block { body } }))
            {
                match statement {
                    Statement::Block { body } => body,
                    statement => Program::new(vec![statement], vec![1]),
                }
            }

        rule while_statement() -> Statement
            = "while" hws1() condition:expression() hws() "{"
              body:block_body() "}"
            { Statement::While { condition, body } }

        rule for_statement() -> Statement
            = "for" hws1() binding:for_binding() hws1() "in" hws1()
              iterable:expression() hws() "{" body:block_body() "}"
            { Statement::For { binding, iterable, body } }

        rule for_binding() -> ForBinding
            = pattern:binding_pattern() { ForBinding::Pattern(pattern) }
            / name:identifier() { ForBinding::Name(name) }

        rule block_statement() -> Statement
            = "{" body:block_body() "}" { Statement::Block { body } }

        rule defer_statement() -> Statement
            = "defer" hws1() statement:(block_statement() / expression_statement_rule())
            { Statement::Defer(Box::new(statement)) }

        rule use_statement() -> Statement
            = use_statement_inner(false)

        rule use_statement_inner(re_export: bool) -> Statement
            = "use" hws1() path:(identifier() ++ (hws() "." hws()))
              items:(hws() "." hws() "{" ws() items:(use_item() ++ comma()) ws() "}" { items })?
              alias:(hws1() "as" hws1() name:identifier() { name })?
            { Statement::Use { path, alias, items: items.unwrap_or_default(), re_export } }

        rule use_item() -> UseItem
            = name:identifier() alias:(hws1() "as" hws1() alias:identifier() { alias })?
            { UseItem { name, alias } }

        rule newtype_statement() -> Statement
            = "newtype" hws1() name:type_identifier()
              type_params:("[" ws() values:(type_identifier() ++ comma()) ws() "]" { values })?
              hws() "=" hws() base:type_expr()
            { Statement::Newtype { name, type_params: type_params.unwrap_or_default(), base } }

        rule type_statement() -> Statement
            = "type" hws1() name:type_identifier()
              type_params:("[" ws() values:(type_identifier() ++ comma()) ws() "]" { values })?
              hws() "=" ws() declaration:type_declaration()
            {
                match declaration {
                    TypeDeclaration::Alias(ty) => Statement::TypeAlias {
                        name,
                        type_params: type_params.unwrap_or_default(),
                        ty,
                    },
                    TypeDeclaration::Sum(variants) => Statement::SumType {
                        name,
                        type_params: type_params.unwrap_or_default(),
                        variants,
                    },
                }
            }

        rule type_declaration() -> TypeDeclaration
            = variants:sum_variants() { TypeDeclaration::Sum(variants) }
            / ty:type_expr() { TypeDeclaration::Alias(ty) }

        rule sum_variants() -> Vec<VariantDecl>
            = "|" ws() first:variant_decl() ws() "|" ws() second:variant_decl()
              rest:(ws() "|" ws() value:variant_decl() { value })*
            {
                let mut variants = Vec::with_capacity(rest.len() + 2);
                variants.push(first);
                variants.push(second);
                variants.extend(rest);
                variants
            }
            / first:variant_decl() ws() "|" ws() second:variant_decl()
              rest:(ws() "|" ws() value:variant_decl() { value })*
            {
                let mut variants = Vec::with_capacity(rest.len() + 2);
                variants.push(first);
                variants.push(second);
                variants.extend(rest);
                variants
            }
            / value:variant_decl_with_fields() { vec![value] }

        rule variant_decl() -> VariantDecl
            = variant_decl_with_fields()
            / name:type_identifier() { VariantDecl { name, fields: Vec::new() } }

        rule variant_decl_with_fields() -> VariantDecl
            = name:type_identifier() hws() "(" ws() fields:(type_expr() ** comma()) ws() ")"
                { VariantDecl { name, fields } }

        rule binding_statement_rule() -> Statement
            = mutable:("const" { false } / "let" { true }) hws1()
              target:binding_target() hws() "=" hws() value:expression()
            { super::binding_statement(mutable, target, value) }

        rule binding_target() -> BindingTarget
            = pattern:binding_pattern() { BindingTarget::Pattern(pattern) }
            / name:identifier()
              annotation:(hws() ":" hws() ty:type_expr() { ty })?
                { BindingTarget::Name(name, annotation) }

        rule binding_pattern() -> BindingPattern
            = binding_pattern_compound()

        rule binding_pattern_value() -> BindingPattern
            = binding_pattern_compound()
            / name:identifier() { BindingPattern::Name(name) }

        rule binding_pattern_compound() -> BindingPattern
            = "(" ws() values:(binding_pattern_value() ++ comma()) ws() ")"
                { BindingPattern::Tuple(values) }
            / "{" ws() fields:(binding_record_field() ++ comma()) ws() "}"
                { BindingPattern::Record(fields) }
            / "[" ws() patterns:(binding_pattern_value() ** comma())
              rest:(comma() "..." name:identifier() { name })? ws() "]"
                { BindingPattern::Array { patterns, rest } }

        rule binding_record_field() -> (String, BindingPattern)
            = field:identifier()
              value:(ws() ":" ws() value:binding_pattern_value() { value })?
                {
                    (field.clone(), value.unwrap_or(BindingPattern::Name(field)))
                }

        rule return_statement() -> Statement
            = "return" hws1() value:expression() { Statement::Return(value) }

        rule break_statement() -> Statement
            = "break" !identifier_continue() { Statement::Break }

        rule continue_statement() -> Statement
            = "continue" !identifier_continue() { Statement::Continue }

        rule require_statement() -> Statement
            = "require" hws() "(" ws() command:string_value()
              version:(comma() "version" hws() "=" hws() value:string_value() { value })?
              ws() ")"
            { Statement::Require { command, version } }

        rule require_one_of_statement() -> Statement
            = "requireOneOf" hws() "(" ws() "[" ws()
              commands:(string_value() ++ comma()) ws() "]" ws() ")"
            { Statement::RequireOneOf { commands } }

        rule redirect_statement() -> Statement
            = first:shell_command()
              rest:(hws() "|>" hws() command:shell_command() { command })*
              hws() ">>" hws()
              append:("write" { false } / "append" { true })
              hws() "(" ws() target:string_value()
              stderr:(comma() "stderr" hws() "=" hws() value:string_value() { value })?
              ws() ")"
            {
                let mut commands = vec![first];
                commands.extend(rest);
                Statement::Redirect {
                    command: commands.join(" | "),
                    target,
                    stderr,
                    append,
                }
            }

        rule assignment_statement() -> Statement
            = !language_keyword() name:identifier() hws() op:compound_assignment_op()
              hws() value:expression()
                { super::compound_assignment(name, op, value) }
            / target:assignment_target() hws() "=" !"=" hws() value:expression()
                { Statement::Assign { target, expr: value } }

        rule assignment_target() -> AssignTarget
            = !language_keyword() name:identifier() ws() "." ws() field:identifier()
              ws() "[" ws() index:expression() ws() "]"
                { AssignTarget::FieldIndex { name, field, index } }
            / !language_keyword() name:identifier() ws() "[" ws() index:expression() ws() "]"
                { AssignTarget::Index { name, index } }
            / !language_keyword() name:identifier() ws() "." ws() "_" field:$(['1'..='9']['0'..='9']*)
                {? field.parse().map(|field| AssignTarget::TupleField { name, field }).or(Err("tuple field")) }
            / !language_keyword() name:identifier() ws() "." ws() field:identifier()
                { AssignTarget::Field { name, field } }
            / !language_keyword() name:identifier()
                { AssignTarget::Name(name) }

        rule compound_assignment_op() -> BinaryOp
            = "++=" { BinaryOp::Concat }
            / "<<=" { BinaryOp::Shl }
            / ">>=" { BinaryOp::Shr }
            / "+=" { BinaryOp::Add }
            / "-=" { BinaryOp::Sub }
            / "*=" { BinaryOp::Mul }
            / "/=" { BinaryOp::Div }
            / "%=" { BinaryOp::Mod }
            / "&=" { BinaryOp::BitAnd }
            / "|=" { BinaryOp::BitOr }
            / "^=" { BinaryOp::BitXor }

        rule expression_statement_rule() -> Statement
            = value:expression() { super::expression_statement(value) }

        pub rule type_root() -> Type
            = ws() value:type_expr() ws() ![_] { value }

        pub rule expression_root() -> Expr
            = ws() value:expression() ws() ![_] { value }

        rule expression() -> Expr
            = lambda()

        rule lambda() -> Expr
            = params:lambda_params() ws() "=>" ws() body:expression()
                { Expr::Lambda { params, body: Box::new(body) } }
            / pipeline()

        rule lambda_params() -> Vec<String>
            = name:identifier() { vec![name] }
            / "(" ws() names:(identifier() ** comma()) ws() ")" { names }

        rule pipeline() -> Expr
            = checked:("try" ws1() { true })? first:operator_expr()
              stages:(ws() "|>" ws() command:shell_command() { command })*
            { super::pipeline(first, stages, checked.unwrap_or(false)) }

        rule operator_expr() -> Expr = precedence! {
            left:(@) ws() "??" ws() right:@ {
                match (left, right) {
                    (Expr::Env(name), Expr::String(default)) =>
                        Expr::EnvDefault { name, default },
                    (left, right) => super::default_expr(left, right),
                }
            }
            --
            left:(@) ws() "<|>" ws() right:@ {
                named_or_value(
                    left,
                    |name| Expr::OptionOrElse {
                        name,
                        fallback: Box::new(right.clone()),
                    },
                    |value| Expr::OptionOrElseValue {
                        value: Box::new(value),
                        fallback: Box::new(right.clone()),
                    },
                )
            }
            --
            left:(@) ws() ">>=" ws() right:@ {
                named_or_value(
                    left,
                    |name| Expr::OptionFlatMap {
                        name,
                        mapper: Box::new(right.clone()),
                    },
                    |value| Expr::OptionFlatMapValue {
                        value: Box::new(value),
                        mapper: Box::new(right.clone()),
                    },
                )
            }
            --
            left:(@) ws() "<*>" ws() right:@ {
                named_or_value(
                    left,
                    |name| Expr::OptionAp {
                        name,
                        value: Box::new(right.clone()),
                    },
                    |function| Expr::OptionApValue {
                        function: Box::new(function),
                        value: Box::new(right.clone()),
                    },
                )
            }
            --
            left:(@) ws() "<$>" ws() right:@ {
                named_or_value(
                    left,
                    |name| Expr::ArrayMap {
                        name,
                        mapper: Box::new(right.clone()),
                    },
                    |value| Expr::ArrayMapValue {
                        value: Box::new(value),
                        mapper: Box::new(right.clone()),
                    },
                )
            }
            --
            left:(@) ws() "..=" ws() right:@ {
                Expr::Range {
                    start: Box::new(left),
                    end: Box::new(right),
                    inclusive: true,
                }
            }
            left:(@) ws() ".." !"." ws() right:@ {
                Expr::Range {
                    start: Box::new(left),
                    end: Box::new(right),
                    inclusive: false,
                }
            }
            --
            left:(@) ws() "||" ws() right:@ { super::binary(left, BinaryOp::Or, right) }
            --
            left:(@) ws() "&&" ws() right:@ { super::binary(left, BinaryOp::And, right) }
            --
            left:(@) ws() "==" ws() right:@ { super::binary(left, BinaryOp::Eq, right) }
            left:(@) ws() "!=" ws() right:@ { super::binary(left, BinaryOp::Ne, right) }
            left:(@) ws() "<=" ws() right:@ { super::binary(left, BinaryOp::Le, right) }
            left:(@) ws() ">=" ws() right:@ { super::binary(left, BinaryOp::Ge, right) }
            left:(@) ws() "<" !("<" / "$" / "|" / "*") ws() right:@
                { super::binary(left, BinaryOp::Lt, right) }
            left:(@) ws() ">" !(">" / "=") ws() right:@
                { super::binary(left, BinaryOp::Gt, right) }
            --
            left:(@) ws() "|" !("|" / ">") ws() right:@
                { super::binary(left, BinaryOp::BitOr, right) }
            --
            left:(@) ws() "^" ws() right:@ { super::binary(left, BinaryOp::BitXor, right) }
            --
            left:(@) ws() "&" !"&" ws() right:@
                { super::binary(left, BinaryOp::BitAnd, right) }
            --
            left:(@) ws() "<<" ws() right:@ { super::binary(left, BinaryOp::Shl, right) }
            left:(@) ws() ">>" !"=" ws() right:@ { super::binary(left, BinaryOp::Shr, right) }
            --
            left:(@) ws() "++" ws() right:@ { super::binary(left, BinaryOp::Concat, right) }
            --
            value:(@) ws1() "as" ws1() ty:type_expr()
                { Expr::Cast { expr: Box::new(value), ty } }
            --
            left:(@) ws() "+" !"+" ws() right:@ { super::binary(left, BinaryOp::Add, right) }
            left:(@) ws() "-" ws() right:@ { super::binary(left, BinaryOp::Sub, right) }
            --
            left:(@) ws() "*" !">" ws() right:@ { super::binary(left, BinaryOp::Mul, right) }
            left:(@) ws() "/" ws() right:@ { super::binary(left, BinaryOp::Div, right) }
            left:(@) ws() "%" ws() right:@ { super::binary(left, BinaryOp::Mod, right) }
            --
            "!" ws() value:@ { Expr::Not(Box::new(value)) }
            "~" ws() value:@ { Expr::BitNot(Box::new(value)) }
            --
            value:postfix() { value }
        }

        rule postfix() -> Expr
            = value:primary() suffixes:postfix_suffix()*
            {? super::apply_postfix(value, suffixes) }

        rule postfix_suffix() -> Postfix
            = ws() "(" ws() args:(call_arg() ** comma()) ws() ")"
                { Postfix::Call(args) }
            / ws() "." ws() method:identifier() ws() "(" ws()
              args:(call_arg() ** comma()) ws() ")"
                { Postfix::Method(method, args) }
            / ws() "[" ws() index:expression() ws() "]" { Postfix::Index(index) }
            / ws() "." ws() "_" field:$(['1'..='9']['0'..='9']*)
                {? field.parse().map(Postfix::TupleField).or(Err("tuple field")) }
            / ws() "." ws() field:identifier() { Postfix::Field(field) }
            / ws() "?" !"?" { Postfix::ResultOption }
            / ws() "!" !"=" { Postfix::TryResult }

        rule primary() -> Expr
            = do_expr()
            / if_expr()
            / match_expr()
            / "async" ws1() value:postfix() { Expr::Async(Box::new(value)) }
            / "async" ws1() command:shell_command() { Expr::AsyncCommand(command) }
            / "spawn" ws1() command:shell_command() { Expr::AsyncCommand(command) }
            / "await" ws1() name:identifier() { Expr::Await(name) }
            / command:shell_command() { Expr::Command { command, checked: false } }
            / triple_string()
            / raw_string()
            / string()
            / float()
            / integer()
            / "true" !identifier_continue() { Expr::Bool(true) }
            / "false" !identifier_continue() { Expr::Bool(false) }
            / "None" !identifier_continue() { Expr::None }
            / "env" "." name:env_identifier() ws() "??" ws() default:string_value()
                { Expr::EnvDefault { name, default } }
            / "env" "." name:env_identifier() { Expr::Env(name) }
            / "env" "." identifier() {? Err("invalid environment name") }
            / "[" ws() values:(expression() ** comma()) ws() "]" { Expr::Array(values) }
            / "{" ws() entries:(collection_entry() ** comma()) ws() "}"
                {? super::collection(entries) }
            / "(" ws() ")" { Expr::Unit }
            / "(" ws() first:expression() ws() "," ws()
              rest:(expression() ** comma()) ws() ")"
            {
                let mut values = Vec::with_capacity(rest.len() + 1);
                values.push(first);
                values.extend(rest);
                Expr::Tuple(values)
            }
            / "(" ws() value:expression() ws() ")" { value }
            / !language_keyword() !("env" ws() ".") name:identifier() { Expr::Ident(name) }

        rule call_arg() -> Expr
            = name:identifier() ws() "=" !"=" ws() value:expression()
                { Expr::NamedArg { name, value: Box::new(value) } }
            / expression()

        rule do_expr() -> Expr
            = "do" ws() "{" ws() items:(do_item() ** do_separator()) ws() "}"
                {? super::do_expression(items) }

        rule do_separator()
            = hws() ";" ws()
            / hws() "\n"+ ws()

        rule do_item() -> DoItem
            = name:identifier() ws() "<-" ws() value:expression()
                { DoItem::Bind(name, value) }
            / ("const" / "let") ws1() name:identifier()
              annotation:(ws() ":" ws() ty:type_expr() { ty })?
              ws() "=" ws() value:expression()
                { DoItem::Let(name, annotation, value) }
            / value:expression() { DoItem::Expr(value) }

        rule if_expr() -> Expr
            = "if" ws1() condition:expression() ws() "{" ws()
              then_expr:expression() ws() "}" ws() "else" ws()
              else_expr:(if_expr() / ("{" ws() value:expression() ws() "}" { value }))
            {
                Expr::IfElse {
                    condition: Box::new(condition),
                    then_expr: Box::new(then_expr),
                    else_expr: Box::new(else_expr),
                }
            }

        rule match_expr() -> Expr
            = "match" ws1() value:expression() ws() "{" ws()
              arms:(match_arm() ** comma()) ws() "}"
            { Expr::Match { value: Box::new(value), arms } }

        rule match_arm() -> MatchArm
            = pattern:match_pattern()
              guard:(ws1() "if" ws1() value:pipeline() { value })?
              ws() "=>" ws() value:expression()
            {
                MatchArm {
                    pattern,
                    guard,
                    expr: value,
                }
            }

        rule match_pattern() -> Option<Expr>
            = "_" !identifier_continue() { None }
            / value:match_pattern_value()
              alias:(ws1() "as" ws1() name:identifier() { name })?
            {
                Some(match alias {
                    Some(alias) => Expr::AliasPattern {
                        pattern: Box::new(value),
                        alias,
                    },
                    None => value,
                })
            }

        rule match_pattern_value() -> Expr
            = "{" ws() fields:(record_pattern_field() ++ comma()) ws() "}"
                { Expr::RecordPattern(fields) }
            / "[" ws() patterns:(match_pattern_value() ** comma())
              rest:(comma() "..." name:identifier() { name })? ws() "]"
                { Expr::ArrayPattern { patterns, rest } }
            / name:identifier() ws() "(" ws()
              args:(match_pattern_value() ** comma()) ws() ")"
                {? super::match_constructor(name, args) }
            / "(" ws() first:match_pattern_value() ws() "," ws()
              rest:(match_pattern_value() ** comma()) ws() ")"
            {
                let mut values = Vec::with_capacity(rest.len() + 1);
                values.push(first);
                values.extend(rest);
                Expr::Tuple(values)
            }
            / "(" ws() value:match_pattern_value() ws() ")" { value }
            / "_" !identifier_continue() { Expr::Ident("_".into()) }
            / triple_string()
            / raw_string()
            / string()
            / float()
            / integer()
            / "true" !identifier_continue() { Expr::Bool(true) }
            / "false" !identifier_continue() { Expr::Bool(false) }
            / "None" !identifier_continue() { Expr::None }
            / name:identifier() { Expr::Ident(name) }

        rule record_pattern_field() -> (String, Option<Expr>)
            = name:identifier()
              value:(ws() ":" ws() value:match_pattern_value() { value })?
                { (name, value) }

        rule collection_entry() -> CollectionEntry
            = name:identifier() ws() ":" ws() value:expression()
                { CollectionEntry::Record(name, value) }
            / key:expression() ws() ":" ws() value:expression()
                { CollectionEntry::Map(key, value) }

        rule integer() -> Expr
            = value:$("-"? "0x" ['0'..='9' | 'a'..='f' | 'A'..='F']+)
            {? i64::from_str_radix(value.trim_start_matches('-').trim_start_matches("0x"), 16)
                .map(|number| Expr::Int(if value.starts_with('-') { -number } else { number }))
                .or(Err("hex integer")) }
            / value:$("-"? "0b" ['0' | '1']+)
            {? i64::from_str_radix(value.trim_start_matches('-').trim_start_matches("0b"), 2)
                .map(|number| Expr::Int(if value.starts_with('-') { -number } else { number }))
                .or(Err("binary integer")) }
            / value:$("-"? ['0'..='9']+) !("." !"." / identifier_continue())
                {? value.parse().map(Expr::Int).or(Err("integer")) }

        rule float() -> Expr
            = value:$("-"? ['0'..='9']+ "." !"." ['0'..='9']+)
              !identifier_continue() { Expr::Float(value.to_string()) }

        rule triple_string() -> Expr
            = "\"\"\"" value:$((!"\"\"\"" [_])*) "\"\"\""
                {? super::string_expr(value.to_string()).or(Err("string")) }

        rule string() -> Expr
            = value:string_value() {? super::string_expr(value).or(Err("string")) }

        rule string_value() -> String
            = "\"" value:$((("\\" [_]) / (!"\"" [_]))*) "\""
                { super::decode_escaped(value) }
            / "'" value:$((("\\" [_]) / (!"'" [_]))*) "'"
                { super::decode_escaped(value) }

        rule raw_string() -> Expr
            = "r\"" value:$((!"\"" [_])*) "\"" { Expr::RawString(value.to_string()) }
            / "r'" value:$((!"'" [_])*) "'" { Expr::RawString(value.to_string()) }

        rule shell_command() -> String
            = "$sh" ws() value:shell_body() { value }

        rule shell_body() -> String
            = "\"" value:$((("\\" [_]) / (!"\"" [_]))*) "\""
                { super::decode_escaped(value) }
            / "'" value:$((("\\" [_]) / (!"'" [_]))*) "'"
                { super::decode_escaped(value) }
            / "{" value:$(shell_piece()*) "}" { value.trim().to_string() }

        rule shell_piece()
            = shell_quoted()
            / "{" shell_piece()* "}"
            / !"}" [_]

        rule shell_quoted()
            = "\"" (("\\" [_]) / (!"\"" [_]))* "\""
            / "'" (("\\" [_]) / (!"'" [_]))* "'"

        rule env_identifier() -> String
            = value:$(['A'..='Z' | '_']['A'..='Z' | '0'..='9' | '_']*)
                { value.to_string() }

        rule identifier_continue()
            = ['A'..='Z' | 'a'..='z' | '0'..='9' | '_']

        rule ws1() = quiet!{[' ' | '\t' | '\r' | '\n']+}
        rule hws() = quiet!{[' ' | '\t' | '\r']*}
        rule hws1() = quiet!{[' ' | '\t' | '\r']+}

        rule type_expr() -> Type
            = params:function_params() ws() "=>" ws() result:type_expr()
                { Type::Function(params, Box::new(result)) }
            / type_result()

        rule function_params() -> Vec<Type>
            = "(" ws() ")" { Vec::new() }
            / "(" ws() values:(type_expr() ++ comma()) ws() ")"
                {? if values.len() >= 2 { Ok(values) } else { Err("at least two tuple parameters") } }
            / value:type_result() { vec![value] }

        rule type_result() -> Type
            = first:type_union() rest:(ws() "\\/" ws() value:type_union() { value })*
            {
                rest.into_iter().fold(first, |left, right| {
                    Type::Applied("Result".to_string(), vec![left, right])
                })
            }

        rule type_union() -> Type
            = first:type_intersection() rest:(ws() "|" ws() value:type_intersection() { value })*
            { rest.into_iter().fold(first, super::flatten_union) }

        rule type_intersection() -> Type
            = first:type_postfix() rest:(ws() "&" ws() value:type_postfix() { value })*
            { rest.into_iter().fold(first, super::flatten_intersection) }

        rule type_postfix() -> Type
            = value:type_atom() suffixes:(ws() "?" { () })*
            {
                suffixes.into_iter().fold(value, |value, ()| {
                    Type::Applied("Option".to_string(), vec![value])
                })
            }

        rule type_atom() -> Type
            = "[" ws() value:type_expr() ws() "]" { Type::Array(Box::new(value)) }
            / "{" ws() fields:(record_type_field() ** comma()) ws() "}"
                { Type::Record(fields) }
            / "(" ws() values:(type_expr() ++ comma()) ws() ")"
                {? if values.len() >= 2 { Ok(Type::Tuple(values)) } else { Err("at least two tuple elements") } }
            / name:type_name() ws() "[" ws() args:(type_expr() ++ comma()) ws() "]"
                { super::applied_type(name, args) }
            / name:type_name() { super::named_type(name) }

        rule record_type_field() -> (String, Type)
            = name:identifier() ws() ":" ws() value:type_expr() { (name, value) }
    }
}
