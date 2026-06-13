# Self-Compilation

Nacre includes a bootstrap source file:

```text
bootstrap/self.ncr
```

The Rust compiler compiles that file into a Bash compiler. The generated Bash
compiler can then compile `bootstrap/self.ncr` again. The proof succeeds when
both generated Bash files are byte-identical.

## Manual Check

```bash
cargo run -- bootstrap/self.ncr /tmp/nacre-self.sh
bash /tmp/nacre-self.sh bootstrap/self.ncr /tmp/nacre-self2.sh
diff -u /tmp/nacre-self.sh /tmp/nacre-self2.sh
```

An empty `diff` means the bootstrap output is stable.

## Test Coverage

The unit test `self_compiles_bootstrap_source` performs the same check. Running
`cargo test` verifies the self-compilation path.

## Why `raw` Is Used

The current language subset implements statement-level conditionals, loops,
functions, and modules, but not shell `case` or the full parsing logic needed for a
practical self-hosted compiler. `bootstrap/self.ncr` therefore uses `raw { ... }`
to contain the Bash implementation of the bootstrap compiler while still being a
valid Nacre input.
