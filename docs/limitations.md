# Current Limitations

- Approved commands return captured standard output as `String`; structured
  exit status and standard-error values are not yet exposed.
- Command pipelines, redirects, background execution, and arbitrary shell
  fragments are intentionally unavailable.
- Filesystem roots must exist when the policy is loaded. A write target may be
  new, but its parent directory must already exist.
- Runtime path validation rejects a final symlink and resolves parent
  directories physically, but Bash cannot eliminate all concurrent
  time-of-check/time-of-use races.
- Some language operations use fixed compiler runtime helpers in generated
  Bash. These helpers are part of the compiler runtime, not source-selectable
  commands.
- Imported modules are checked as a whole, so a module containing capability
  calls requires those capabilities even if a particular function is unused.
- Nacre currently targets Bash and assumes Bash 4 or newer.

All code in this repository was developed by a Coding Agent.
