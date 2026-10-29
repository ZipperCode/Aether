# Design

## Ownership and data flow
- The pool cursor emits `pool_group_exhausted` for a no-attempt group, keeping the existing `pool_group_exhaustion` counters. Delete the plurality-reason selector; member reasons remain in counters and request diagnostics.
- The gateway request-candidate writer maps the explicitly marked group summary to `key_id: None` using the existing nullable contract. Actual member rows retain their keys. No schema or repository interface change.
- The existing persisted-context capacity classifier derives explicitly marked groups from their nonempty positive integer member-reason counts. It requires every reason in the existing capacity set, keeping diagnostics-disabled image pools at 429 only for pure capacity; mixed, unknown or malformed facts remain 503. Never classify `pool_group_exhausted` itself as capacity.
- The admin timeline recognizes the existing `extra_data.pool_group_exhaustion` marker as group scope, including historical rows. Render one pool-level diagnosis with member-reason counts and suppress representative key/account metadata. Per-key views remain unchanged.
- Keep bounded diagnostics, authentication, scheduler eligibility, request-level error classification and yielded-member suppression unchanged.

## Compatibility
No database migration or new public field. Historical stored rows are not rewritten: UI interprets their already-present group marker correctly. Nullable key identities are already supported by candidate records.

## Verification
Extend an existing real HTTP pool fixture for mixed model incompatibility and exhausted supporting members. Assert exact summary scope/counts, no upstream sends, status, and unaffected real member records. Run the running frontend with a synthetic local trace containing the incident-shaped group payload; inspect desktop/mobile output and ordinary-key control. No real provider requests or production writes.
