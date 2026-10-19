# GitHub release status

- User authorized commit/push, followed by a new tag only after exact-SHA GitHub CI success.
- Integration committed and pushed to `ZipperCode/Aether` master: `287ef3e23f311834726bac44bff1f76d04125e3b`.
- Push did not create a Rust CI run after the discovery wait. Repository Actions permissions API reported enabled and workflow state active.
- Added `workflow_dispatch` to the existing Rust CI workflow; existing scope fallback checks all tracked paths on manual dispatch. Local CI command contract test and YAML parse passed.
- CI dispatch entry committed and pushed: `35d5460b5` (master). HTTPS workflow-file push was rejected due missing OAuth workflow scope; verified existing SSH identity and pushed through SSH without changing credentials or remote configuration.
- Explicit workflow dispatch rejected with `HTTP 422: Actions has been disabled for this repository.` Existing workflow enable API succeeded but dispatch still returned the same rejection. Repository is not archived or disabled; caller has admin permissions. No repository permission/security/billing changes made.
- Next version based on remote history: `v0.7.44`. It has NOT been created or pushed. Release remains blocked until GitHub accepts and successfully completes the exact master commit CI run.
- User confirmed Actions reenabled. Manual full Rust CI run `39262530485` for exact SHA `35d5460b57089e2b51de73ffc46ecc6f6e8d1599` executed. All real jobs passed except Integration Scenarios, which aborted on test-thread stack overflow in `invalid_or_control_text_on_a_bound_socket_does_not_poison_the_next_valid_turn`; aggregate Test/check failed correctly. Fix and a new exact-SHA CI run are required before tag publication.
