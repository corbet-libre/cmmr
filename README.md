# cmmr — Memory

A bounded in-memory store for cfrm's volatile records. `Store`, its types and the
single `conformance` suite live here; cvlk implements the same port and cvtl wires
both. See [the implemented contract](docs/CONTRACT.md).

Use `Memory::new(scope, limits, clock)`, then `get`, `compare_exchange`,
`delete_generation`, `page`, `maintain` and `close`. No default wall clock, files,
background tasks or implicit expiry extension. Caller-issued revisions reconcile
uncertain remote writes. Scopes are trusted service capabilities.

Candidates surveyed: Rust's `BTreeMap`/`BTreeSet`, DashMap and Moka. Standard ordered
collections plus one short mutex preserve atomic multi-record/index transitions;
cache eviction and concurrent independent-map writes would violate this contract.
Existing cthl supplies throttle policy separately; old cfrm SQL discovery storage
cannot meet the volatile contract and is not copied. No crypto is implemented.

CI runs native tests and the same storage vectors in WebAssembly. Enable the
`conformance` feature only for adapter acceptance. No registry publication.

## Scope

### Purpose

cmmr provides the volatile storage contract in memory for tests and small single-process setups.

### Owns

| Item | Detail |
|---|---|
| Storage port | The `Store` interface with its scope, limits, clock, record, revision, deadline, write, cursor, and page types |
| Conformance suite | The single shared suite that every backend implementing the port must pass |
| Memory backend | A real bounded in-memory implementation of the port |

### Never

cmmr is not a permissive mock, does not write files, does not extend deadlines beyond what was requested, and does not represent a shared multi-process store. It runs no background tasks and uses no implicit system clock.

### States

The store is either ready (empty or holding live entries) or closed. A failed store reports unavailable. Each entry is absent, live with a revision and an exclusive deadline, or expired and deleted. Writes that exceed capacity are refused, and closing leaves the store empty.

### Test obligations

The identical conformance vectors pass against the memory and Valkey backends, covering expiry boundaries, isolation, revisions, batch atomicity, bounded indexes, cancellation, capacity, cursor binding, and clock regression. A restart begins empty, and native and WebAssembly runs use identical vectors.

## Continuous verification

Dependency updates follow main and are tested against one CI-resolved lockfile.
Line and branch coverage target 100%; failures remain blocking. See
[the coverage contract](docs/COVERAGE.md) for measurement and exclusions.
