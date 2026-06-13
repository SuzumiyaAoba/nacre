# Current Limitations

Nacre is currently a compact compiler prototype. These limitations are part of
the documented behavior of this repository state.

## Compact Control Flow

The compiler implements `if` expressions, `match` expressions, statement-level
`if`, `while`, `for`, `break`, and `continue`. Use `raw { ... }` when generated
Bash needs unsupported constructs.

## Compact Static Type Checker

The compiler has a small static checker for primitive values, annotations,
arrays, maps, tuples, newtypes, functions, block scopes, and loop bodies. It
supports UFCS-style method calls, trait method declarations, impl method
definitions, and generic bounds. Trait methods are emitted with type-specific
function names, so the same method name can be implemented for different
receiver types. When multiple traits implement the same method name for the same
receiver type, unqualified `value.method()` calls are rejected and
`TraitName.method(value)` must be used.

## Structured Closure Captures

Lambdas capture primitive and structured values by value, including arrays,
maps, records, tuples, and Option or Result wrappers with structured payloads.
The captured Bash declarations are restored only for the duration of a closure
call.

Intermediate `const` and `let` declarations inside `do` expressions can store
primitive and structured values. `<-` bindings may carry supported Option or
Result payloads through the generated `flatMap` chain.

## Sum Type Payloads

Sum types support nullary variants and variants with positional primitive or
structured fields, including arrays, maps, records, tuples, and Option or
Result wrappers with structured payloads. A bare all-named declaration such as
`type State = Ready | Failed` is interpreted as a sum type; use built-in or
applied type syntax when declaring structural unions.

## Result Propagation Positions

`try value` and postfix `value!` propagate `Err` from the current
Result-returning function or lambda. They can be used as standalone statements,
binding or `return` values, and nested in eagerly evaluated expressions such as
function arguments, constructors, collection elements, and arithmetic. The
compiler also preserves lazy evaluation for propagation in `&&`, `||`, `??`,
`<|>`, `if` or `match` result branches, and `match` guards. Result-returning
lambdas are propagation scopes of their own, including mapper lambdas whose
Result return type is inferred from a body containing `!`.

## Comparison and Numeric Expressions Use `awk`

Comparison operators compile to `true` or `false` values when bound to
variables, and to exit-status checks when used in control-flow conditions.
Numeric arithmetic and comparison are emitted through `awk`, which means the
generated Bash expects `awk` to be available for those expressions.

For maximum portability without `awk`, prefer strings, environment defaults,
commands, and raw blocks.
