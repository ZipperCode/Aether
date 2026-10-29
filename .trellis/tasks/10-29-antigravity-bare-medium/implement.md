# Implementation

1. Change the existing native HTTP regression to expect medium on first/warm no-effort requests and no-effort SSE. Run it once before production changes and retain the wrong-high failure.
2. Transport owner: extend the request reader to accept requested_model, centralize suffix/body/absent-setting precedence and bare Gemini medium default, update all parser callers/tests, and cover private-envelope normalization for base-only Gemini 3 models. Preserve explicit malformed settings and Gemini 2.5 numeric budget wire behavior.
3. Gateway owner: replace the local decoder wrapper with the transport reader at routed/static context attachment; add real ordinary/Pool regressions for bare mapping, per-account missing/disabled/exhausted medium, eligible sibling selection and all-ineligible zero upstream sends. Preserve allowlists and explicit overrides.
   - Review follow-through: preserve nullable OpenAI no-effort defaults in transport and real HTTP scenarios; at the existing Antigravity envelope boundary reuse parsed source selection when conversion omitted thinking, preserving `extra-low`/suffix overrides and rejecting unresolved explicit effort before defaulting.
   - Native parity: when an explicit suffixed client alias resolves to a discovered bare Gemini 3 identity, project the suffix before endpoint rules without conversion; preserve single rule execution and verify native sync/SSE with conflicting body effort.
4. After all edits, run focused transport/pool tests and Gateway dynamic-effort regressions with the existing 16 MiB stack, then targeted Gateway CI/Clippy and formatting. Fix failures without altering unrelated behavior tests.
5. Exercise independent localhost HTTP requests against the real gateway/loopback upstream and inspect exact final model/level plus client completion, including native SSE and explicit overrides.
6. Update the account-model/quota contract to replace obsolete no-effort defaults, capture verified boundaries, remove temporary smoke scaffolding, and isolate this task plus prior budget compatibility changes for the authorized repository delivery flow. No production deployment.
7. Authorized completion: seed only the omitted Codex account discovery fixture, retain original OAuth/candidate-order assertions, run `python3 tools/ci.py preflight`, then create scoped work commits, archive this completed task, record the session and push `master` to `origin`. No production deployment.

## Review gates

- Default is selected before account discovery/permission/quota checks, never merely an envelope-only rewrite.
- No synthesized model catalog, account-crossing resolution or high/low substitution when medium is unavailable.
- Medium uses Gemini thinkingLevel; never emit thinkingBudget: "medium".
- Explicit suffix/level/budget precedence and malformed-setting behavior stay intact.

## Observed verification

- Before production edits, `gateway_antigravity_dynamic_effort_native_alias_variants` failed at `ordinary/step1-no-effort-default`: actual completion model `gemini-3.8-flash-high`, required `gemini-3.8-flash-medium` (retained session tool output, `bg_7` / artifact 104). The behavioral regression remains in the permanent suite.
- Transport Antigravity tests: 82 passed; account-scoped thinking resolver tests: 5 passed.
- Independent localhost curl smoke: 16 requests passed across ordinary and Pool modes. Covered bare default, explicit low, numeric budget high, warm default, native SSE STOP, Chat null effort, Responses null reasoning and null effort. Inspected exact upstream model/thinkingConfig and client completion text/model. Temporary launcher and include were removed after proof.
- Full Gateway run after converted suffix fixes: 5,765 executed, 5,764 passed, 1 failed, 4 skipped. All Antigravity cases passed. The sole failure is `scheduler::candidate::tests::selection::keeps_refresh_failed_oauth_candidate_selectable_before_local_auth_resolution` at `selection.rs:2151` (`len` 1 versus 2); investigate causality without rerunning this observed failure. Gateway Clippy (`--lib --bins --examples -D warnings`) and workspace format check passed at this revision. Native base-only suffix parity is checked separately after its final edit.
- The original full-suite failure was independent of the Antigravity change: the Codex OAuth fixture had no discovery evidence. On the user's explicit repair request, seed its existing `key.upstream_metadata.codex_model_discovery.model_ids` with the exact selected `gpt-4.1-responses` identity, matching neighboring fixtures. Production gates and all original assertions remain unchanged.
- After native base-only suffix parity integration, all 99 Gateway Antigravity tests passed, including ordinary/Pool native sync/SSE default medium and explicit low over body high, converted null/unknown/compatibility efforts, discovered-model admission and scoped quota recovery. Read-only review reported no remaining findings.
- Final native-integrated Gateway Clippy (`--lib --bins --examples -- -D warnings`) and `cargo fmt --all --check` passed. That iteration stopped without commit/archive while the Codex fixture failure awaited authorization; the user has now authorized its repair and commit/push.
- Authorized completion verification: `python3 tools/ci.py preflight` passed workspace formatting, Gateway Clippy (`--lib --bins --examples -D warnings`) and all 5,766 executed Gateway tests (3 skipped). The previously failing OAuth selection test passed with both original candidate-order assertions and no skipped candidates. No runtime policy change was required.
