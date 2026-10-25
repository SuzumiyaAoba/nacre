use super::{parse_expr, parse_type};
use crate::{parse, Type};

#[test]
fn parses_type_grammar() {
    assert_eq!(parse_type("String", 1).unwrap(), Type::String);
    assert_eq!(
        parse_type("Map[String, [Int?]]", 1).unwrap(),
        Type::Map(
            Box::new(Type::String),
            Box::new(Type::Array(Box::new(Type::Applied(
                "Option".into(),
                vec![Type::Int],
            )))),
        )
    );
    assert_eq!(
        parse_type("(String, Int) => String \\/ CmdError", 1).unwrap(),
        Type::Function(
            vec![Type::String, Type::Int],
            Box::new(Type::Applied(
                "Result".into(),
                vec![Type::String, Type::Named("CmdError".into())],
            )),
        )
    );
}

#[test]
fn expression_grammar_parses_representative_syntax() {
    let cases = [
        "1 + 2 * 3",
        "1 - 2",
        "1 / 2",
        "1 % 2",
        "1 != 2",
        "1 <= 2",
        "1 > 2",
        "1 >= 2",
        "1 & 3",
        "1 | 2",
        "1 ^ 3",
        "1 << 2",
        "4 >> 1",
        "(1 + 2) * 3",
        "true || false && true",
        r#""a" ++ "b""#,
        "value as UserId",
        "Some(1)",
        r#"env.HOME ?? "/tmp""#,
        "Ok(1)?",
        "source()!",
        "[1, 2, 3]",
        r#"{ name: "Ada", age: 36 }"#,
        r#"{ "PORT": "8080" }"#,
        r#"("host", 8080)"#,
        "names[0]",
        "pair._2",
        "user.name",
        "names.map(x => x + 1)",
        r#"("nacre").slice(1, 4)"#,
        r#"run.inspect.status("--short")"#,
        "if true { 1 } else { 2 }",
        r#"match code { 0 => "ok", _ => "error" }"#,
        "do {\nvalue <- Some(1)\npure(value)\n}",
        "x => x + 1",
    ];
    for source in cases {
        parse_expr(source, 1).unwrap_or_else(|error| panic!("failed to parse `{source}`: {error}"));
    }
}

#[test]
fn program_grammar_accepts_indented_blocks() {
    parse(
        r#"
fn classify(value: Int): String {
if value > 0 {
    return "positive"
} else {
    return "zero"
}
}

trait Show[T] {
fn show(value: T): String
}

impl Show[Int] {
fn show(value: Int): String {
    return classify(value)
}
}

for value in [1, 0] {
const label = classify(value)
}
"#,
    )
    .unwrap();
}

#[test]
fn program_grammar_parses_repository_sources() {
    let roots = ["docs/examples", "std"];
    for root in roots {
        for entry in std::fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|value| value.to_str()) != Some("ncr") {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            parse(&source)
                .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()));
        }
    }
}

#[test]
fn documentation_nacre_blocks_parse() {
    for root in ["docs/en/src", "docs/ja/src"] {
        for entry in std::fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|value| value.to_str()) != Some("md") {
                continue;
            }
            let markdown = std::fs::read_to_string(&path).unwrap();
            let mut block = None::<String>;
            let mut block_line = 0;

            for (index, line) in markdown.lines().enumerate() {
                if line == "```nacre" {
                    assert!(block.is_none(), "nested Nacre fence in {}", path.display());
                    block = Some(String::new());
                    block_line = index + 2;
                } else if line == "```" && block.is_some() {
                    let source = block.take().unwrap();
                    if !source.contains("{{#include") {
                        parse(&source).unwrap_or_else(|error| {
                            panic!(
                                "failed to parse Nacre block at {}:{block_line}: {error}",
                                path.display()
                            )
                        });
                    }
                } else if let Some(source) = &mut block {
                    source.push_str(line);
                    source.push('\n');
                }
            }

            assert!(
                block.is_none(),
                "unterminated Nacre fence in {}",
                path.display()
            );
        }
    }
}
