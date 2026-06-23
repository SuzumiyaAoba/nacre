#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    Use {
        path: Vec<String>,
        alias: Option<String>,
        items: Vec<UseItem>,
        re_export: bool,
    },
    Export(Box<Statement>),
    Trait {
        name: String,
        type_param: String,
        methods: Vec<TraitMethod>,
    },
    Impl {
        trait_name: String,
        for_type: Type,
        methods: Vec<ImplMethod>,
    },
    InherentImpl {
        for_type: Type,
        consts: Vec<ImplConst>,
        methods: Vec<ImplMethod>,
    },
    TypeAlias {
        name: String,
        type_params: Vec<String>,
        ty: Type,
    },
    SumType {
        name: String,
        type_params: Vec<String>,
        variants: Vec<VariantDecl>,
    },
    Newtype {
        name: String,
        type_params: Vec<String>,
        base: Type,
    },
    Function {
        name: String,
        override_constructor: bool,
        type_params: Vec<TypeParam>,
        params: Vec<Param>,
        return_type: Type,
        body: Program,
    },
    ExternalFunction {
        name: String,
        type_params: Vec<TypeParam>,
        params: Vec<Param>,
        return_type: Type,
    },
    Const {
        name: String,
        annotation: Option<Type>,
        expr: Expr,
    },
    Let {
        name: String,
        annotation: Option<Type>,
        expr: Expr,
    },
    Destructure {
        mutable: bool,
        pattern: BindingPattern,
        expr: Expr,
    },
    Assign {
        target: AssignTarget,
        expr: Expr,
    },
    Expr(Expr),
    TryCommand(String),
    TryCommandResult(String),
    TryResult(Expr),
    TryPipeline {
        input: Option<Box<Expr>>,
        commands: Vec<String>,
    },
    TryPipelineResult {
        input: Option<Box<Expr>>,
        commands: Vec<String>,
    },
    Command(String),
    Redirect {
        command: String,
        target: String,
        stderr: Option<String>,
        append: bool,
    },
    Require {
        command: String,
        version: Option<String>,
    },
    RequireOneOf {
        commands: Vec<String>,
    },
    Block {
        body: Program,
    },
    Defer(Box<Statement>),
    If {
        condition: Expr,
        then_branch: Program,
        else_branch: Option<Program>,
    },
    While {
        condition: Expr,
        body: Program,
    },
    For {
        binding: ForBinding,
        iterable: Expr,
        body: Program,
    },
    Break,
    Continue,
    Return(Expr),
    Raw(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseItem {
    pub name: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssignTarget {
    Name(String),
    Index {
        name: String,
        index: Expr,
    },
    FieldIndex {
        name: String,
        field: String,
        index: Expr,
    },
    Field {
        name: String,
        field: String,
    },
    TupleField {
        name: String,
        field: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub default: Option<Expr>,
    pub variadic: bool,
    pub capture_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingPattern {
    Name(String),
    Tuple(Vec<BindingPattern>),
    Record(Vec<(String, BindingPattern)>),
    Array {
        patterns: Vec<BindingPattern>,
        rest: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForBinding {
    Name(String),
    Pattern(BindingPattern),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeParam {
    pub name: String,
    pub bounds: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitMethod {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Type,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplMethod {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Type,
    pub body: Program,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplConst {
    pub name: String,
    pub annotation: Option<Type>,
    pub expr: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
    pub pattern: Option<Expr>,
    pub guard: Option<Expr>,
    pub expr: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosureCapture {
    pub source: String,
    pub target: String,
    pub suffixes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantDecl {
    pub name: String,
    pub fields: Vec<Type>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoStep {
    Bind {
        name: String,
        expr: Expr,
    },
    Let {
        name: String,
        annotation: Option<Type>,
        expr: Expr,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Int(i64),
    Float(String),
    Bool(bool),
    String(String),
    RawString(String),
    Unit,
    Some(Box<Expr>),
    None,
    Ok(Box<Expr>),
    Err(Box<Expr>),
    ResultOption(Box<Expr>),
    TryResult(Box<Expr>),
    Default {
        value: Box<Expr>,
        fallback: Box<Expr>,
    },
    DefaultTry {
        value: Box<Expr>,
        fallback: Box<Expr>,
    },
    Command {
        command: String,
        checked: bool,
    },
    CommandResult {
        command: String,
    },
    AllowedCommand {
        group: String,
        command: String,
        args: Vec<Expr>,
        result: bool,
        program: Option<String>,
        read_args: Vec<usize>,
        write_args: Vec<usize>,
    },
    AsyncCommand(String),
    Async(Box<Expr>),
    Await(String),
    Pipeline {
        input: Option<Box<Expr>>,
        commands: Vec<String>,
    },
    TryPipeline {
        input: Option<Box<Expr>>,
        commands: Vec<String>,
    },
    PipelineResult {
        input: Option<Box<Expr>>,
        commands: Vec<String>,
    },
    HasCommand(String),
    PathExists(Box<Expr>),
    Array(Vec<Expr>),
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
        inclusive: bool,
    },
    Map(Vec<(Expr, Expr)>),
    Record(Vec<(String, Expr)>),
    RecordPattern(Vec<(String, Option<Expr>)>),
    ArrayPattern {
        patterns: Vec<Expr>,
        rest: Option<String>,
    },
    AliasPattern {
        pattern: Box<Expr>,
        alias: String,
    },
    Tuple(Vec<Expr>),
    Index {
        name: String,
        index: Box<Expr>,
    },
    IndexValue {
        value: Box<Expr>,
        index: Box<Expr>,
    },
    Slice {
        name: String,
        start: Box<Expr>,
        end: Box<Expr>,
    },
    ArraySliceValue {
        value: Box<Expr>,
        start: Box<Expr>,
        end: Box<Expr>,
    },
    TupleField {
        name: String,
        field: usize,
    },
    TupleFieldValue {
        value: Box<Expr>,
        field: usize,
    },
    Field {
        name: String,
        field: String,
    },
    FieldValue {
        value: Box<Expr>,
        field: String,
    },
    NewtypeCtor {
        name: String,
        value: Box<Expr>,
    },
    Variant {
        name: String,
        args: Vec<Expr>,
        field_types: Vec<Type>,
    },
    Cast {
        expr: Box<Expr>,
        ty: Type,
    },
    Lambda {
        params: Vec<String>,
        body: Box<Expr>,
    },
    Closure {
        name: String,
        captures: Vec<ClosureCapture>,
    },
    Do {
        steps: Vec<DoStep>,
        result: Box<Expr>,
    },
    LetIn {
        name: String,
        annotation: Option<Type>,
        value: Box<Expr>,
        body: Box<Expr>,
    },
    Call {
        name: String,
        args: Vec<Expr>,
    },
    NamedArg {
        name: String,
        value: Box<Expr>,
    },
    Value(String),
    Len(String),
    ArrayLenValue(Box<Expr>),
    MapLenValue(Box<Expr>),
    IsEmpty(String),
    ArrayIsEmptyValue(Box<Expr>),
    MapIsEmptyValue(Box<Expr>),
    ArrayFirst(String),
    ArrayFirstValue(Box<Expr>),
    ArrayLast(String),
    ArrayLastValue(Box<Expr>),
    ArrayReverse(String),
    ArrayReverseValue(Box<Expr>),
    ArraySort(String),
    ArraySortValue(Box<Expr>),
    ArrayUnique(String),
    ArrayUniqueValue(Box<Expr>),
    ArrayMap {
        name: String,
        mapper: Box<Expr>,
    },
    ArrayMapValue {
        value: Box<Expr>,
        mapper: Box<Expr>,
    },
    ArrayFilter {
        name: String,
        predicate: Box<Expr>,
    },
    ArrayFilterValue {
        value: Box<Expr>,
        predicate: Box<Expr>,
    },
    ArrayFlatMap {
        name: String,
        mapper: Box<Expr>,
    },
    ArrayFlatMapValue {
        value: Box<Expr>,
        mapper: Box<Expr>,
    },
    ArrayFind {
        name: String,
        predicate: Box<Expr>,
    },
    ArrayFindValue {
        value: Box<Expr>,
        predicate: Box<Expr>,
    },
    ArrayAny {
        name: String,
        predicate: Box<Expr>,
    },
    ArrayAnyValue {
        value: Box<Expr>,
        predicate: Box<Expr>,
    },
    ArrayAll {
        name: String,
        predicate: Box<Expr>,
    },
    ArrayAllValue {
        value: Box<Expr>,
        predicate: Box<Expr>,
    },
    ArrayFold {
        name: String,
        initial: Box<Expr>,
        reducer: Box<Expr>,
    },
    ArrayFoldValue {
        value: Box<Expr>,
        initial: Box<Expr>,
        reducer: Box<Expr>,
    },
    OptionMap {
        name: String,
        mapper: Box<Expr>,
    },
    OptionMapValue {
        value: Box<Expr>,
        mapper: Box<Expr>,
    },
    OptionFlatMap {
        name: String,
        mapper: Box<Expr>,
    },
    OptionFlatMapValue {
        value: Box<Expr>,
        mapper: Box<Expr>,
    },
    ResultMap {
        name: String,
        mapper: Box<Expr>,
    },
    ResultMapValue {
        value: Box<Expr>,
        mapper: Box<Expr>,
    },
    ResultFlatMap {
        name: String,
        mapper: Box<Expr>,
    },
    ResultFlatMapValue {
        value: Box<Expr>,
        mapper: Box<Expr>,
    },
    OptionAp {
        name: String,
        value: Box<Expr>,
    },
    OptionApValue {
        function: Box<Expr>,
        value: Box<Expr>,
    },
    ResultAp {
        name: String,
        value: Box<Expr>,
    },
    ResultApValue {
        function: Box<Expr>,
        value: Box<Expr>,
    },
    OptionOrElse {
        name: String,
        fallback: Box<Expr>,
    },
    OptionOrElseValue {
        value: Box<Expr>,
        fallback: Box<Expr>,
    },
    OptionOrElseTry {
        value: Box<Expr>,
        fallback: Box<Expr>,
    },
    ArrayTake {
        name: String,
        count: Box<Expr>,
    },
    ArrayTakeValue {
        value: Box<Expr>,
        count: Box<Expr>,
    },
    ArrayDrop {
        name: String,
        count: Box<Expr>,
    },
    ArrayDropValue {
        value: Box<Expr>,
        count: Box<Expr>,
    },
    Join {
        name: String,
        separator: Box<Expr>,
    },
    JoinValue {
        value: Box<Expr>,
        separator: Box<Expr>,
    },
    ArrayPush {
        name: String,
        value: Box<Expr>,
    },
    ArrayPop {
        name: String,
    },
    ArrayContains {
        name: String,
        value: Box<Expr>,
    },
    ArrayContainsValue {
        value: Box<Expr>,
        item: Box<Expr>,
    },
    ArrayIndexOf {
        name: String,
        value: Box<Expr>,
    },
    ArrayIndexOfValue {
        value: Box<Expr>,
        item: Box<Expr>,
    },
    MapKeys(String),
    MapKeysValue(Box<Expr>),
    MapValues(String),
    MapValuesValue(Box<Expr>),
    MapHas {
        name: String,
        key: Box<Expr>,
    },
    MapHasValue {
        value: Box<Expr>,
        key: Box<Expr>,
    },
    MapSet {
        name: String,
        key: Box<Expr>,
        value: Box<Expr>,
    },
    MapRemove {
        name: String,
        key: Box<Expr>,
    },
    StringContains {
        name: String,
        needle: Box<Expr>,
    },
    StringContainsValue {
        value: Box<Expr>,
        needle: Box<Expr>,
    },
    StringIndexOf {
        name: String,
        needle: Box<Expr>,
    },
    StringIndexOfValue {
        value: Box<Expr>,
        needle: Box<Expr>,
    },
    StringStartsWith {
        name: String,
        prefix: Box<Expr>,
    },
    StringStartsWithValue {
        value: Box<Expr>,
        prefix: Box<Expr>,
    },
    StringEndsWith {
        name: String,
        suffix: Box<Expr>,
    },
    StringEndsWithValue {
        value: Box<Expr>,
        suffix: Box<Expr>,
    },
    StringLen(String),
    StringLenValue(Box<Expr>),
    StringIsEmpty(String),
    StringIsEmptyValue(Box<Expr>),
    StringSlice {
        name: String,
        start: Box<Expr>,
        end: Box<Expr>,
    },
    StringSliceValue {
        value: Box<Expr>,
        start: Box<Expr>,
        end: Box<Expr>,
    },
    StringTrim(String),
    StringTrimValue(Box<Expr>),
    StringTrimStart(String),
    StringTrimStartValue(Box<Expr>),
    StringTrimEnd(String),
    StringTrimEndValue(Box<Expr>),
    StringToUpper(String),
    StringToUpperValue(Box<Expr>),
    StringToLower(String),
    StringToLowerValue(Box<Expr>),
    StringRepeat {
        name: String,
        count: Box<Expr>,
    },
    StringRepeatValue {
        value: Box<Expr>,
        count: Box<Expr>,
    },
    StringSplit {
        name: String,
        separator: Box<Expr>,
    },
    StringSplitValue {
        value: Box<Expr>,
        separator: Box<Expr>,
    },
    StringReplace {
        name: String,
        from: Box<Expr>,
        to: Box<Expr>,
    },
    StringReplaceValue {
        value: Box<Expr>,
        from: Box<Expr>,
        to: Box<Expr>,
    },
    PathBasename(String),
    PathBasenameValue(Box<Expr>),
    PathDirname(String),
    PathDirnameValue(Box<Expr>),
    PathStem(String),
    PathStemValue(Box<Expr>),
    PathExtname(String),
    PathExtnameValue(Box<Expr>),
    PathIsAbsolute(String),
    PathIsAbsoluteValue(Box<Expr>),
    EnvDefault {
        name: String,
        default: String,
    },
    Env(String),
    ProcessArgs,
    ProcessEnv {
        name: Box<Expr>,
    },
    FsIsFile {
        path: Box<Expr>,
    },
    FsIsDir {
        path: Box<Expr>,
    },
    FsSize {
        path: Box<Expr>,
    },
    FsReadLines {
        path: Box<Expr>,
    },
    FsList {
        path: Box<Expr>,
    },
    FsWriteLines {
        path: Box<Expr>,
        lines: Box<Expr>,
    },
    FsAppendLines {
        path: Box<Expr>,
        lines: Box<Expr>,
    },
    CliParse,
    JsonParse {
        value: Box<Expr>,
    },
    JsonStringify {
        name: String,
    },
    JsonStringifyValue {
        value: Box<Expr>,
    },
    IfElse {
        condition: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
    },
    Match {
        value: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    MatchGuardResult(Box<Expr>),
    Not(Box<Expr>),
    BitNot(Box<Expr>),
    Ident(String),
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
    },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Concat,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinaryOp {
    pub(crate) fn is_arithmetic(self) -> bool {
        matches!(
            self,
            Self::Add | Self::Sub | Self::Mul | Self::Div | Self::Mod
        )
    }

    pub(crate) fn is_logical(self) -> bool {
        matches!(self, Self::And | Self::Or)
    }

    pub(crate) fn is_bitwise(self) -> bool {
        matches!(
            self,
            Self::BitAnd | Self::BitOr | Self::BitXor | Self::Shl | Self::Shr
        )
    }

    pub(crate) fn bash(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Sub => "-",
            Self::Mul => "*",
            Self::Div => "/",
            Self::Mod => "%",
            Self::Concat => "++",
            Self::BitAnd => "&",
            Self::BitOr => "|",
            Self::BitXor => "^",
            Self::Shl => "<<",
            Self::Shr => ">>",
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::And => "&&",
            Self::Or => "||",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    statements: Vec<Statement>,
    lines: Vec<usize>,
}

impl Program {
    pub(crate) fn new(statements: Vec<Statement>, lines: Vec<usize>) -> Self {
        Self { statements, lines }
    }

    pub fn statements(&self) -> &[Statement] {
        &self.statements
    }

    pub(crate) fn statement_lines(&self) -> &[usize] {
        &self.lines
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Int,
    Float,
    Bool,
    String,
    Path,
    ExitCode,
    Unit,
    Future(Box<Type>),
    Array(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Record(Vec<(String, Type)>),
    Tuple(Vec<Type>),
    Function(Vec<Type>, Box<Type>),
    Union(Vec<Type>),
    Intersection(Vec<Type>),
    Generic(String),
    Applied(String, Vec<Type>),
    Brand { name: String, base: Box<Type> },
    Named(String),
}

impl Type {
    pub(crate) fn name(&self) -> String {
        match self {
            Self::Int => "Int".to_string(),
            Self::Float => "Float".to_string(),
            Self::Bool => "Bool".to_string(),
            Self::String => "String".to_string(),
            Self::Path => "Path".to_string(),
            Self::ExitCode => "ExitCode".to_string(),
            Self::Unit => "Unit".to_string(),
            Self::Future(value) => format!("Future[{}]", value.name()),
            Self::Array(element) => format!("[{}]", element.name()),
            Self::Map(key, value) => format!("Map[{}, {}]", key.name(), value.name()),
            Self::Record(fields) => {
                let names = fields
                    .iter()
                    .map(|(name, ty)| format!("{name}: {}", ty.name()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{ {names} }}")
            }
            Self::Tuple(elements) => {
                let names = elements
                    .iter()
                    .map(Type::name)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("({names})")
            }
            Self::Function(params, return_type) => {
                let params = if params.len() == 1 {
                    params[0].name()
                } else {
                    format!(
                        "({})",
                        params.iter().map(Type::name).collect::<Vec<_>>().join(", ")
                    )
                };
                format!("{params} => {}", return_type.name())
            }
            Self::Union(types) => types.iter().map(Type::name).collect::<Vec<_>>().join(" | "),
            Self::Intersection(types) => {
                types.iter().map(Type::name).collect::<Vec<_>>().join(" & ")
            }
            Self::Generic(name) => name.clone(),
            Self::Applied(name, args) if name == "Option" && args.len() == 1 => {
                format!("{}?", args[0].name())
            }
            Self::Applied(name, args) if name == "Result" && args.len() == 2 => {
                format!("{} \\/ {}", args[0].name(), args[1].name())
            }
            Self::Applied(name, args) => {
                let args = args.iter().map(Type::name).collect::<Vec<_>>().join(", ");
                format!("{name}[{args}]")
            }
            Self::Brand { name, .. } | Self::Named(name) => name.clone(),
        }
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    #[test]
    fn names_cover_all_composite_type_shapes() {
        let cases = [
            (Type::Float, "Float"),
            (Type::String, "String"),
            (Type::Path, "Path"),
            (Type::Unit, "Unit"),
            (Type::Future(Box::new(Type::Int)), "Future[Int]"),
            (
                Type::Record(vec![
                    ("name".into(), Type::String),
                    ("age".into(), Type::Int),
                ]),
                "{ name: String, age: Int }",
            ),
            (
                Type::Function(vec![Type::String, Type::Int], Box::new(Type::Bool)),
                "(String, Int) => Bool",
            ),
            (
                Type::Union(vec![Type::String, Type::Int, Type::Bool]),
                "String | Int | Bool",
            ),
            (
                Type::Intersection(vec![Type::String, Type::Path]),
                "String & Path",
            ),
            (Type::Generic("T".into()), "T"),
            (
                Type::Applied("Box".into(), vec![Type::String, Type::Int]),
                "Box[String, Int]",
            ),
            (
                Type::Applied("Option".into(), vec![Type::String]),
                "String?",
            ),
            (
                Type::Applied("Result".into(), vec![Type::String, Type::Int]),
                "String \\/ Int",
            ),
        ];

        for (ty, expected) in cases {
            assert_eq!(ty.name(), expected);
        }
    }
}
