# Implementation plan

1. Scheduler owner: add fail-before regression for representative with incompatible/empty list, scope mismatch and unchanged non-pool denial; implement fallback only in logical enumeration. Preserve strict per-member resolver. Extend real HTTP pool fixture for incompatible representative plus eligible sibling and all-incompatible pool.
2. Association owner (parent): verify production runtime and normal admin authentication; use existing credentials/session only for user-requested route mutations. Preserve all existing configuration, use explicit Gemini binding and unconditional real-target mapping. No schema change or auth bypass.
3. Verify: focused scheduler suite, real loopback HTTP pool cases including both exact globals to same true upstream, affected pool resolution guards, formatting check. Do not rerun unrelated failures already recorded in previous task.
4. Update association contract, record verification, stage only isolated task changes; commit only with passing relevant verification. No push without explicit user authorization.
