# cmmr agent notes

Work in a lane (`~/agents/scratch/lane-new <lane>`); never edit `~/agents/scratch/ws`.
Run from the lane root (`<lane>/ws`); `cbuild <lane> ...` builds there.

Build and test (quote the verdict line, never infer from a pipe):
- `cbuild <lane> fmt -p cmmr -- --check` — must be clean
- `cbuild <lane> test -p cmmr --all-features` — lib `1`, `contract` `4`,
  `validation` `5`, doc `0` passed, exit 0; long, background it
- `cbuild <lane> clippy -p cmmr --locked --all-targets --all-features -- -D warnings` — exit 0
- `cbuild <lane> check -p cmmr --locked --target wasm32-unknown-unknown` — exit 0
- Full gate (first-party refresh, wasm conformance under a real runner,
  coverage) runs in CI; see `.ci/verify.sh`, `docs/COVERAGE.md`.

Land: `cargo fmt` in touched repos, rebase on main (never force-push),
`cbuild <lane> xtask land --dry`, then `xtask land`.

Invariants (rule -> enforcing test):
- CAS compares all expected revisions before publishing anything;
  expired records are absent, so a stale heartbeat conflicts
  -> `tests/it/contract.rs::revision_race_has_one_winner`, `::storage_contract`
- Full-capacity replacements stay atomic; expiry reclaims capacity;
  generation fencing holds
  -> `::full_capacity_replacements_stay_atomic_then_succeed`,
  `::expiry_reclaims_capacity_and_generation_fencing_holds`
- Invalid batches never change live records or indexes
  -> `tests/it/validation.rs::invalid_batches_never_change_live_records_or_indexes`
- `check_current` atomically checks exact revisions and exclusive deadlines
  and never renews anything; foreign, mutated or expired entries refuse
  -> `src/conformance.rs::run` via `tests/it/contract.rs::storage_contract`
- Bounds, serialization, cursors and deadlines are checked before paging
  -> `::bounds_and_serialization_refuse_invalid_storage_inputs`,
  `::cursor_encoding_and_deadlines_are_checked_before_paging`
- Invalid or unavailable clocks never change records; poisoned mutexes
  fence reads and close clears everything
  -> `::invalid_or_unavailable_clock_never_changes_records`,
  `tests/internal/poison.rs::real_mutex_poisoning_fences_reads_and_close`

Pointers: `src/memory.rs`, `src/conformance.rs`, `docs/CONTRACT.md`, `docs/COVERAGE.md`.
Commits: explicit paths, imperative, author per `~/agents/AGENT.md`; no AI trailers.
`publish=false`; keep the LGPL license text; no GPL-only or AGPL-only dependencies.
