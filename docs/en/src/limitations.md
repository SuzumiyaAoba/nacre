# Current Limitations

These constraints describe the current implementation rather than planned
syntax.

## Execution

- Nacre targets Bash 4 or newer.
- Approved commands return captured standard output as `String`.
- Ordinary approved command calls do not return structured exit status and
  standard-error values. Use `run.result.<group>.<command>` when those values
  are needed.
- Arbitrary shell, pipelines, redirects, background execution, and dynamic
  executable selection are intentionally unavailable.

## Filesystem Safety

- Filesystem roots must exist when the policy is loaded.
- A new write target is permitted only when its parent directory already
  exists beneath an allowed root.
- Existing symlink components in requested paths are rejected and parent
  directories are physically resolved, but shell-based checks cannot eliminate all concurrent
  time-of-check/time-of-use races.

## Compilation

- Imported modules are type-checked as a whole, but capability calls inside
  unreferenced imported function bodies do not require policy capabilities.
  Function-value references count as reachable. Top-level initializers and
  trait implementations are checked conservatively.
- Some structured values use compiler-provided Bash runtime helpers.
- Diagnostics include line, column, source-line, and caret output, but there is
  not yet a dedicated LSP server or CLI JSON output.
- Dependency resolution supports only local path dependencies from
  `nacre.toml`. There is not yet a stable public registry or lockfile.

## Documentation

The verified examples cover representative behavior, not every combination of
language features. The test suite is the authoritative executable record for
edge cases.
