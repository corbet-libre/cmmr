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

Own the bounded process-local volatile Store, shared record/deadline/CAS/index/generation contract and adapter conformance vectors. No authentication, member policy, persistent storage, request logging or background I/O.

## Continuous verification

Dependency updates follow main and are tested against one CI-resolved lockfile.
Line and branch coverage target 100%; failures remain blocking. See
[the coverage contract](docs/COVERAGE.md) for measurement and exclusions.
