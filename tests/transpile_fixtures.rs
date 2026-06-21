use std::fs;
use std::path::Path;

fn split_transpile_fixture(fixture: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();

    for line in fixture.lines() {
        if line == "-----" {
            parts.push(std::mem::take(&mut current));
        } else {
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
        }
    }
    parts.push(current);

    parts
}

fn normalize_fixture_text(text: &str) -> &str {
    text.trim_end_matches('\n')
}

fn assert_transpile_fixture(path: &Path) {
    let fixture = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let parts = split_transpile_fixture(&fixture);

    assert_eq!(
        parts.len(),
        2,
        "{} must contain exactly one input/output pair separated by -----",
        path.display()
    );

    let source = &parts[0];
    let expected = &parts[1];
    assert!(
        !source.trim().is_empty(),
        "{} has empty input",
        path.display()
    );
    assert!(
        !expected.trim().is_empty(),
        "{} has empty expected output",
        path.display()
    );

    let actual = nacre::compile_source(source).unwrap_or_else(|error| {
        panic!("{} failed to compile:\n{error}", path.display());
    });

    assert_eq!(
        normalize_fixture_text(&actual),
        normalize_fixture_text(expected),
        "{} output mismatch",
        path.display()
    );
}

#[test]
fn transpile_matches_file_fixtures() {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/transpile");
    let mut paths = fs::read_dir(&fixture_dir)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", fixture_dir.display()))
        .map(|entry| {
            entry
                .unwrap_or_else(|error| panic!("failed to read fixture entry: {error}"))
                .path()
        })
        .filter(|path| path.extension().is_some_and(|extension| extension == "txt"))
        .collect::<Vec<_>>();
    paths.sort();

    assert!(
        !paths.is_empty(),
        "no transpile fixture files found in {}",
        fixture_dir.display()
    );

    for path in paths {
        assert_transpile_fixture(&path);
    }
}
