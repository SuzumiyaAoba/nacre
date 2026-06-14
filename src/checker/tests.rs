use super::*;
use crate::parse;

#[test]
fn type_checks_bindings_and_operator_operands() {
    let valid = parse(
        r#"
const answer = 42
const hex: Int = 0xFF
const pi: Float = 3.14
const yes = true
const text = "hello"
const home = env.HOME ?? "/tmp"
const bin: Path = "/usr/bin"
const okCode: ExitCode = 0
const unit: Unit = ()
const copied = answer
const greeting = "Hello, ${text}"
const rawGreeting = r"Hello, ${missing}"
const hasGit = hasCommand("git")
const names: [String] = ["alice", "bob"]
const [firstUser, ...remainingUsers] = names
const nums = [1, 2, 3]
const emptyNames: [String] = []
fn greet(name: String, prefix: String = "Hello"): String {
return "${prefix}, ${name}"
}
const message = greet("Nacre")
const custom = greet("Nacre", "Hi")
const label = if answer > 0 { "positive" } else { "zero" }
const matched = match label { "positive" => "yes", _ => "no" }
const envs: Map[String, String] = { "PORT": "8080", "HOST": "localhost" }
const emptyEnv: Map[String, String] = {}
const port = envs["PORT"]
const firstName = names[0]
const namesLen = names.len()
const pair: (String, Int) = ("localhost", 8080)
const hostName = pair._1
const portNumber = pair._2
const (destructuredHost, destructuredPort) = pair
const user: { name: String, age: Int } = { name: "Ada", age: 36 }
const userName = user.name
const userAge = user.age
let { age } = user
type Account = { id: Int, name: String }
const account: Account = { id: 1, name: "core" }
const accountName = account.name
type Unary = String => String
fn exclaim(value: String): String {
return "${value}!"
}
fn applyString(f: Unary, value: String): String {
return f(value)
}
const applied = applyString(exclaim, "Hi")
fn identity[T](value: T): T {
return value
}
const genericText = identity("generic")
const genericInt = identity(7)
trait Show[T] {
}
impl Show[Int] {
}
fn boundIdentity[T: Show](value: T): T {
return value
}
const boundedInt = boundIdentity(7)
type Box[T] = { item: T }
const boxed: Box[Int] = { item: 7 }
const boxedValue = boxed.item
newtype UserId = Int
newtype GroupId = Int
const uid: UserId = UserId(42)
const gid: GroupId = GroupId(42)
const rawUid: Int = uid.value
if answer > 0 {
const inside = "ok"
} else {
const inside = "zero"
}
while answer > 0 {
break
}
for item in names {
const echoed = item
}
let count = answer + 1
count = count / 2
let ratio: Float = count
const sameInt = count == 21
const sameFloat = pi > 3
const sameBool = yes != false
const sameString = text == home
const ordered = count >= 0
const _ = missing_ok
"#,
    )
    .unwrap();

    assert!(type_check(&valid).is_err());

    let valid = parse(
        r#"
const answer = 42
const yes = true
const text = "hello"
const home = env.HOME ?? "/tmp"
const path: Path = "/tmp"
const copied = answer
let count = answer + 1
count = count / 2
const sameInt = count == 21
const sameBool = yes != false
const sameString = text == home
const ordered = count >= 0
const both = sameBool && ordered
const either = both || false
const inverted = !either
const joined = text ++ home
const pathJoined = path ++ "/file"
const bitMask = 6 & 3
const bitAny = bitMask | 8
const bitFlip = bitAny ^ 1
const shifted = bitFlip << 1
const unshifted = shifted >> 1
const invertedBits = ~unshifted
const bitCheck = bitMask == 2
newtype CastId = Int
const castRaw = 7
const castId: CastId = castRaw as CastId
const castBack: Int = castId as Int
const pathText: String = path as String
const _ = "discarded"
"#,
    )
    .unwrap();
    type_check(&valid).unwrap();

    let discarded_assignment = parse("_ = 1").unwrap();
    type_check(&discarded_assignment).unwrap();
    assert_eq!(Type::Unit.name(), "Unit");

    let cases = [
            ("const x = missing", 1, "undefined variable"),
            ("const x = 1\nconst x = 2", 2, "already defined"),
            ("const x = 1\nx = 2", 2, "cannot assign to const"),
            ("x = 1", 1, "cannot assign to undefined variable"),
            ("let x = 1\nx = true", 2, "type mismatch"),
            ("const x: Bool = 1", 1, "type annotation mismatch"),
            ("const x: ExitCode = 256", 1, "type annotation mismatch"),
            ("const x = true && 1", 1, "requires Bool operands"),
            ("const x = !1", 1, "requires Bool operand"),
            ("const x = \"a\" ++ 1", 1, "requires String or Path operands"),
            ("const x = 1 & true", 1, "requires Int operands"),
            ("const x = ~true", 1, "requires Int operands"),
            ("const x = \"1\" as Int", 1, "cannot cast String to Int"),
            (
                "const x = \"hello ${missing}\"",
                1,
                "undefined variable `missing` in string interpolation",
            ),
            (
                "const x = \"hello ${bad-name}\"",
                1,
                "invalid interpolation name",
            ),
            (
                "const x = \"hello ${missing\"",
                1,
                "unterminated string interpolation",
            ),
            ("const x = [1, true]", 1, "array elements"),
            (
                "const x: [Bool] = [true, 1]",
                1,
                "array elements",
            ),
            (
                "const x: [String] = [1]",
                1,
                "type annotation mismatch",
            ),
            ("fn greet(name: String): String {\nreturn name\n}\nconst x = greet(1)", 4, "argument `name`"),
            (
                "fn exclaim(value: String): String {\nreturn value\n}\nconst x: Int => String = exclaim",
                4,
                "type annotation mismatch",
            ),
            (
                "fn apply(f: String => String): String {\nreturn f(1)\n}",
                1,
                "argument 1",
            ),
            (
                "fn first[T](a: T, b: T): T {\nreturn a\n}\nconst x = first(1, true)",
                4,
                "generic type `T`",
            ),
            (
                "fn first[T: Show](value: T): T {\nreturn value\n}",
                1,
                "unknown trait `Show` in generic bound",
            ),
            (
                "trait Show[T] {\n}\nfn first[T: Show](value: T): T {\nreturn value\n}\nconst x = first(1)",
                6,
                "does not implement trait `Show`",
            ),
            (
                "trait Show[T] {\n}\ntrait Show[T] {\n}",
                3,
                "already defined",
            ),
            (
                "impl Show[Int] {\n}",
                1,
                "unknown trait `Show`",
            ),
            (
                "trait Show[T] {\n}\nimpl Show[Int] {\n}\nimpl Show[Int] {\n}",
                5,
                "already implemented",
            ),
            (
                "type Box[T] = { item: T }\nconst x: Box = { value: 1 }",
                2,
                "unknown type",
            ),
            (
                "type Box[T] = { item: T }\nconst x: Box[Int, String] = { value: 1 }",
                2,
                "expects 1 type arguments",
            ),
            (
                "const [a] = 1",
                1,
                "array destructuring requires array value",
            ),
            (
                "const (a, b) = 1",
                1,
                "tuple destructuring requires tuple value",
            ),
            (
                "const (a, b) = (1, 2, 3)",
                1,
                "tuple destructuring expected 2 values",
            ),
            (
                "const { missing } = { name: \"Ada\" }",
                1,
                "record destructuring field `missing` is missing",
            ),
            ("const x = await missing", 1, "undefined future"),
            ("const x = 1\nconst y = await x", 2, "await expects Future"),
            ("const x = 1\nconst y = x()", 2, "not callable"),
            ("const x = missingFn()", 1, "undefined function"),
            ("fn greet(name: String): String {\nconst value = name\n}", 1, "must return String"),
            ("return 1", 1, "return is only valid inside a function"),
            ("fn greet(prefix: String = \"Hello\", name: String): String {\nreturn name\n}", 1, "required function parameters"),
            ("fn greet(name: String = 1): String {\nreturn name\n}", 1, "default for parameter"),
            ("fn greet(name: String): Int {\nreturn name\n}", 1, "return type mismatch"),
            ("fn greet(): String {\nreturn \"a\"\n}\nfn greet(): String {\nreturn \"b\"\n}", 4, "already defined"),
            ("fn greet(): String {\nreturn \"a\"\n}\nconst x = greet(1)", 4, "expects 0..0 arguments"),
            ("const x = if 1 { \"a\" } else { \"b\" }", 1, "condition must be Bool"),
            ("const x = if true { 1 } else { \"b\" }", 1, "if expression branches"),
            ("const x = match 1 { 1 => \"one\" }", 1, "wildcard `_` arm"),
            ("const x = match 1 { \"one\" => 1, _ => 0 }", 1, "match pattern type mismatch"),
            ("const x = match 1 { 1 => 1, _ => \"zero\" }", 1, "match arms"),
            ("const x = missing[0]", 1, "undefined variable"),
            ("const xs = [1]\nconst x = xs[true]", 2, "array index must be Int"),
            ("const x = 1\nconst y = x[0]", 2, "cannot index"),
            ("const m = { \"a\": 1 }\nconst x = m[1]", 2, "map key must be String"),
            ("const x = { \"a\": 1, 2: 2 }", 1, "map keys"),
            ("const x = { \"a\": 1, \"b\": true }", 1, "map values"),
            (
                "const x: Map[String, String] = { \"a\": 1 }",
                1,
                "type annotation mismatch",
            ),
            (
                "const x = { name: \"Ada\", name: \"Grace\" }",
                1,
                "record field `name`",
            ),
            (
                "const x: { name: String, age: Int } = { name: \"Ada\" }",
                1,
                "type annotation mismatch",
            ),
            (
                "const x = { name: \"Ada\" }\nconst y = x.age",
                2,
                "has no field `age`",
            ),
            (
                "const x = 1\nconst y = x.name",
                2,
                "cannot access field `name`",
            ),
            ("type User = { name: String }\ntype User = { name: String }", 2, "already defined"),
            ("type User = Missing", 1, "unknown type"),
            ("const x = 1\nconst y = x.len()", 2, "has no len method"),
            (
                "const x: (String, Int) = (1, 2)",
                1,
                "type annotation mismatch",
            ),
            ("const x = (1, true)\nconst y = x._3", 2, "has no field _3"),
            (
                "const x = 1\nconst y = x._1",
                2,
                "cannot access tuple field",
            ),
            ("const x: Missing = 1", 1, "unknown type"),
            ("newtype UserId = Int\nnewtype UserId = Int", 2, "already defined"),
            (
                "newtype UserId = Int\nconst x: UserId = 1",
                2,
                "type annotation mismatch",
            ),
            (
                "newtype UserId = Int\nconst x = UserId(true)",
                2,
                "newtype constructor",
            ),
            ("const x = Missing(1)", 1, "unknown type"),
            ("const x = 1\nconst y = x.value", 2, "cannot access `.value`"),
            ("if 1 {\nconst value = 1\n}", 1, "condition must be Bool"),
            ("while 1 {\nbreak\n}", 1, "condition must be Bool"),
            (
                "const x = 1\nfor item in x {\nconst value = item\n}",
                2,
                "for loop iterable must be Array",
            ),
            ("const x = \"1\" + 2", 1, "requires numeric operands"),
            ("const x = 1 == true", 1, "matching operand types"),
            ("const x = true % 2", 1, "requires Int operands"),
            ("const x = \"a\" < \"b\"", 1, "requires numeric operands"),
        ];

    for (source, line, message) in cases {
        let program = parse(source).unwrap();
        let error = match type_check(&program) {
            Err(error) => error,
            Ok(()) => panic!("expected type error for `{source}`"),
        };
        assert_eq!(error.line(), line);
        assert!(error.message().contains(message), "{error}");
    }
}

#[test]
fn check_program_covers_direct_statement_paths() {
    let program = parse(
        r#"
use std.fs
trait Show[T] {
fn show(value: T): String
}
impl Show[Int] {
fn show(value: Int): String {
return "int"
}
}
type User = { name: String }
newtype UserId = Int
fn greet(name: String): String {
return name
}
const names = ["Ada"]
const user: User = { name: "Ada" }
const id = UserId(1)
let count = 1
count = 2
greet("Ada")
if true {
const inside = count
} else {
const inside = 0
}
while true {
break
}
for name in names {
const copy = name
continue
}
"#,
    )
    .unwrap();

    TypeChecker::default().check_program(&program).unwrap();
}

#[test]
fn checker_helpers_cover_generic_substitution_shapes() {
    let mut inferred = HashMap::new();
    inferred.insert("T".into(), Type::String);

    assert_eq!(
        impl_method_name(
            "pkg.Show",
            &Type::Applied("Box".into(), vec![Type::Int]),
            "show-value"
        ),
        "__nacre_trait_pkg_Show_Box_Int__show_value"
    );
    assert_eq!(
        substitute_generics(&Type::Generic("T".into()), &inferred),
        Type::String
    );
    assert_eq!(
        substitute_generics(
            &Type::Future(Box::new(Type::Generic("T".into()))),
            &inferred
        ),
        Type::Future(Box::new(Type::String))
    );
    assert_eq!(
        substitute_generics(
            &Type::Map(
                Box::new(Type::Generic("T".into())),
                Box::new(Type::Array(Box::new(Type::Generic("T".into())))),
            ),
            &inferred,
        ),
        Type::Map(
            Box::new(Type::String),
            Box::new(Type::Array(Box::new(Type::String))),
        )
    );
    assert_eq!(
        substitute_generics(
            &Type::Record(vec![("item".into(), Type::Generic("T".into()))]),
            &inferred,
        ),
        Type::Record(vec![("item".into(), Type::String)])
    );
    assert_eq!(
        substitute_generics(
            &Type::Tuple(vec![Type::Generic("T".into()), Type::Int]),
            &inferred,
        ),
        Type::Tuple(vec![Type::String, Type::Int])
    );
    assert_eq!(
        substitute_generics(
            &Type::Function(
                vec![Type::Generic("T".into())],
                Box::new(Type::Applied("Box".into(), vec![Type::Generic("T".into())])),
            ),
            &inferred,
        ),
        Type::Function(
            vec![Type::String],
            Box::new(Type::Applied("Box".into(), vec![Type::String])),
        )
    );

    let program = Program::new(
        vec![Statement::Trait {
            name: "Show".into(),
            type_param: "T".into(),
            methods: vec![TraitMethod {
                name: "show".into(),
                params: vec![Param {
                    name: "value".into(),
                    ty: Type::Generic("T".into()),
                    default: Some(Expr::String("x".into())),
                    variadic: false,
                    capture_name: None,
                }],
                return_type: Type::String,
            }],
        }],
        vec![1],
    );
    let error = TypeChecker::default().check_program(&program).unwrap_err();
    assert!(error.message().contains("cannot use default parameters"));

    let for_error = parse("const x = 1\nfor item in x {\nconst copy = item\n}").unwrap();
    let error = TypeChecker::default()
        .check_program(&for_error)
        .unwrap_err();
    assert!(error.message().contains("for loop iterable must be Array"));

    let mut checker = TypeChecker::default();
    checker.define("value", Type::Int, false, 1).unwrap();
    checker.method_impls.insert(
        ("show".into(), "Int".into()),
        vec![
            ("Display".into(), "display_show".into()),
            ("Debug".into(), "debug_show".into()),
        ],
    );
    let error = checker.resolve_method_name("value", "show", 1).unwrap_err();
    assert!(error.message().contains("ambiguous method"));
    assert_eq!(
        checker
            .resolve_scoped_method_name("Display", "show", &[Expr::Ident("value".into())], 1)
            .unwrap(),
        "display_show"
    );
    let error = checker
        .resolve_scoped_method_name("Clone", "show", &[Expr::Ident("value".into())], 1)
        .unwrap_err();
    assert!(error.message().contains("does not implement trait `Clone`"));
    let error = checker
        .resolve_scoped_method_name("Display", "missing", &[Expr::Ident("value".into())], 1)
        .unwrap_err();
    assert!(error
        .message()
        .contains("does not implement trait `Display`"));
    let error = checker
        .resolve_scoped_method_name("Display", "show", &[], 1)
        .unwrap_err();
    assert!(error.message().contains("requires a receiver argument"));
    checker.functions.insert(
        "fallback".into(),
        FunctionSig {
            type_params: Vec::new(),
            params: Vec::new(),
            return_type: Type::Unit,
        },
    );
    assert_eq!(
        checker.resolve_method_name("value", "fallback", 1).unwrap(),
        "fallback"
    );
    assert_eq!(
        checker
            .lower_expr(
                &Expr::Field {
                    name: "missing".into(),
                    field: "field".into(),
                },
                1
            )
            .unwrap(),
        Expr::Field {
            name: "missing".into(),
            field: "field".into(),
        }
    );
    assert!(!is_valid_name(""));
    assert_eq!(method_call_parts("value."), None);

    assert!(checker
        .check_tuple(&[Expr::Int(1)], 1)
        .unwrap_err()
        .message()
        .contains("tuple literal"));
    assert!(checker
        .check_len("missing", 1)
        .unwrap_err()
        .message()
        .contains("undefined variable"));
    assert!(checker
        .check_tuple_field("missing", 1, 1)
        .unwrap_err()
        .message()
        .contains("undefined variable"));
    assert!(checker
        .check_field("missing", "field", 1)
        .unwrap_err()
        .message()
        .contains("undefined variable"));
    checker.types.insert("Alias".into(), Type::Int);
    assert!(checker
        .check_newtype_ctor("Alias", &Expr::Int(1), 1)
        .unwrap_err()
        .message()
        .contains("not a newtype"));

    let generic_sig = FunctionSig {
        type_params: vec![TypeParam {
            name: "T".into(),
            bounds: Vec::new(),
        }],
        params: Vec::new(),
        return_type: Type::Generic("T".into()),
    };
    let error = checker
        .check_generic_call("identity", &generic_sig, &[], 1)
        .unwrap_err();
    assert!(error.message().contains("could not infer generic type"));
    let error = checker
        .check_generic_call("identity", &generic_sig, &[Expr::Int(1)], 1)
        .unwrap_err();
    assert!(error.message().contains("expects 0 arguments"));

    let mut inferred = HashMap::new();
    assert!(checker
        .infer_generic_type(
            &Type::Future(Box::new(Type::Generic("T".into()))),
            &Type::Future(Box::new(Type::String)),
            &Expr::Await("job".into()),
            &mut HashMap::new(),
        )
        .is_ok());
    assert!(checker
        .infer_generic_type(
            &Type::Future(Box::new(Type::Generic("T".into()))),
            &Type::Int,
            &Expr::Int(1),
            &mut HashMap::new(),
        )
        .unwrap_err()
        .contains("expected T"));
    assert!(checker
        .infer_generic_type(
            &Type::Map(Box::new(Type::String), Box::new(Type::Generic("T".into()))),
            &Type::Int,
            &Expr::Int(1),
            &mut inferred,
        )
        .unwrap_err()
        .contains("expected Map"));
    assert!(checker
        .infer_generic_type(
            &Type::Tuple(vec![Type::String, Type::Generic("T".into())]),
            &Type::Int,
            &Expr::Int(1),
            &mut HashMap::new(),
        )
        .unwrap_err()
        .contains("expected (String, T)"));
    assert!(checker
        .infer_generic_type(
            &Type::Tuple(vec![Type::String, Type::Generic("T".into())]),
            &Type::Tuple(vec![Type::String]),
            &Expr::Tuple(vec![Expr::String("x".into()), Expr::Int(1)]),
            &mut inferred,
        )
        .unwrap_err()
        .contains("expected (String, T)"));
    assert!(checker
        .infer_generic_type(
            &Type::Record(vec![("item".into(), Type::Generic("T".into()))]),
            &Type::Int,
            &Expr::Int(1),
            &mut HashMap::new(),
        )
        .unwrap_err()
        .contains("expected { item: T }"));
    assert!(checker
        .infer_generic_type(
            &Type::Record(vec![("item".into(), Type::Generic("T".into()))]),
            &Type::Record(Vec::new()),
            &Expr::Record(Vec::new()),
            &mut inferred,
        )
        .unwrap_err()
        .contains("record field `item` is missing"));
    assert!(checker
        .infer_generic_type(
            &Type::Function(
                vec![Type::Generic("T".into())],
                Box::new(Type::Generic("T".into()))
            ),
            &Type::Function(Vec::new(), Box::new(Type::String)),
            &Expr::Ident("f".into()),
            &mut HashMap::new(),
        )
        .unwrap_err()
        .contains("expected T => T"));
    assert!(checker
        .infer_generic_type(
            &Type::Function(
                vec![Type::Generic("T".into())],
                Box::new(Type::Generic("T".into()))
            ),
            &Type::Int,
            &Expr::Int(1),
            &mut inferred,
        )
        .unwrap_err()
        .contains("expected T => T"));
    checker
        .define(
            "callable",
            Type::Function(vec![Type::String], Box::new(Type::Bool)),
            false,
            1,
        )
        .unwrap();
    let error = checker
        .check_function_value_call("callable", &[], 1)
        .unwrap_err();
    assert!(error
        .message()
        .contains("function value `callable` expects 1 arguments"));
    let error = checker
        .resolve_type_with_generics(
            &Type::Applied("Missing".into(), Vec::new()),
            &HashSet::new(),
            1,
        )
        .unwrap_err();
    assert!(error.message().contains("unknown type `Missing`"));
    assert_eq!(
        checker
            .resolve_type_with_generics(&Type::Future(Box::new(Type::String)), &HashSet::new(), 1)
            .unwrap(),
        Type::Future(Box::new(Type::String))
    );
    assert!(checker
        .check_match(&Expr::Int(1), &[], 1)
        .unwrap_err()
        .message()
        .contains("requires at least one arm"));
    assert!(checker
        .check_value_access("missing", 1)
        .unwrap_err()
        .message()
        .contains("undefined variable"));
    assert!(checker.is_assignable(&Type::Path, &Type::String, &Expr::String("/tmp".into())));
    assert!(checker.is_assignable(
        &Type::Future(Box::new(Type::Path)),
        &Type::Future(Box::new(Type::String)),
        &Expr::Await("job".into())
    ));
    assert!(checker.is_assignable(
        &Type::Function(vec![Type::Path], Box::new(Type::Path)),
        &Type::Function(vec![Type::String], Box::new(Type::String)),
        &Expr::Ident("f".into())
    ));
    assert!(checker.is_assignable(
        &Type::Record(vec![("name".into(), Type::String)]),
        &Type::Record(vec![("name".into(), Type::Path)]),
        &Expr::Record(Vec::new())
    ));
    assert!(checker.is_assignable(
        &Type::Brand {
            name: "UserId".into(),
            base: Box::new(Type::Int),
        },
        &Type::Brand {
            name: "UserId".into(),
            base: Box::new(Type::String),
        },
        &Expr::Int(1)
    ));
}
