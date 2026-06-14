# Verified Examples

Every program on this page is compiled and executed by
`scripts/verify-docs.sh`. The examples share
[`policy.toml`](examples/policy.toml).

## Hello

```nacre
{{#include ../../examples/hello.ncr}}
```

[Open source](examples/hello.ncr)

## Environment and Arithmetic

```nacre
{{#include ../../examples/env-and-math.ncr}}
```

[Open source](examples/env-and-math.ncr)

## Control Flow

The source is formatted with four spaces for each nested block.

```nacre
{{#include ../../examples/control-flow.ncr}}
```

[Open source](examples/control-flow.ncr)

## Functions

```nacre
{{#include ../../examples/functions.ncr}}
```

[Open source](examples/functions.ncr)

## Japanese Text

Nacre source and generated scripts preserve UTF-8 strings.

```nacre
{{#include ../../examples/japanese.ncr}}
```

[Open source](examples/japanese.ncr)

## Approved Commands

```nacre
{{#include ../../examples/commands.ncr}}
```

[Open source](examples/commands.ncr)

## Guarded Filesystem Access

```nacre
{{#include ../../examples/filesystem.ncr}}
```

[Open source](examples/filesystem.ncr)

## Shared Policy

```toml
{{#include ../../examples/policy.toml}}
```

[Open policy](examples/policy.toml)
