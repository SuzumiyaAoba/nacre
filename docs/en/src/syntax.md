# Language Design

Nacre is designed for typed scripts whose authority can be reviewed separately
from their source.

## Design Goals

1. Make common scripting data flow concise and statically checked.
2. Compile to readable, standalone Bash with no compiler needed at runtime.
3. Keep executable and filesystem authority outside source-controlled code.
4. Preserve argument boundaries instead of constructing shell command strings.
5. Reject unsafe syntax explicitly rather than attempting to sanitize it.

## Authority Model

```text
Nacre source
    │ parse and type-check
    ▼
Static capability names ────── External TOML policy
    │                                  │
    └──────── resolve authority ───────┘
                       │
                       ▼
              Standalone Bash
```

Source can request a static capability:

```nacre
const text = run.read.document("input/data.txt")
run.output.print(text)
```

Only the policy can map those names to executable files:

```toml
[filesystem]
read = ["input"]

[command_groups.read.commands.document]
program = "bin/read-document"
args = 1
read_args = [0]

[command_groups.output.commands.print]
program = "bin/print"
args = 1
```

## Safe Profile

The safe profile excludes arbitrary shell strings, raw Bash, dynamic executable
names, pipelines, redirects, and background commands. Complex operations belong
in narrow reviewed programs named by the policy.

The parser still contains internal representation for some historical syntax,
but public compilation rejects it before emission. Documentation describes the
accepted safe profile, not those internal forms.

## Runtime Boundary

Generated Bash includes fixed helpers for structured values, closures, and path
guards when needed. These helpers are compiler-owned runtime code and cannot be
selected or replaced by Nacre source.

The policy, approved executables, compiler, and runtime helpers are trusted.
Source-provided values remain data across the command boundary.

## Implementation Notes

- Parsing uses a declarative `rust-peg` grammar.
- Type checking and lowering occur before Bash emission.
- Module imports are expanded and namespaced before checking.
- The generated script enables `set -euo pipefail`.

The implementation and tests, rather than this design overview, define the
precise current behavior.
