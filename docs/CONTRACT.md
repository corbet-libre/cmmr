# Implemented storage contract

`Memory<C: Clock>` is a real process-local scoped store. `Scope` combines a
service-authorized community with a fresh incarnation; it is not authentication.
`Limits` bounds records, value bytes, writes per batch, indexes, pages and TTL.
Names are 1–128 ASCII alphanumeric, underscore or hyphen bytes. Records contain
opaque bytes; callers must encrypt private content before crossing this boundary.

States: Ready (empty or live) → Closed. Mutex poisoning reports Unavailable.
Entries: Absent → Live(revision, exclusive deadline) → Expired or Deleted.
A fresh instance is empty. Cloned Arc handles share one state. Close clears it.
Clock is injected Unix milliseconds and sampled under the mutex. Regression
refuses access until it catches up; elapsed deadlines are never extended.

Every Write contains a validated Deadline. CAS compares all expected revisions
before publishing any replacement, including indexes. None means create only if
absent; expired records are absent, so a stale heartbeat conflicts. Callers supply
fresh nonzero 128-bit revision tokens for every replacement and creation; do not
reuse tokens. Re-read the current revisions to reconcile an uncertain commit;
absence or a superseding writer does not prove a lost operation never committed.
There is deliberately no unbounded historical operation receipt journal.

Indexes and generation membership are owned by records; they cannot be written
separately. Delete-generation removes only records still bearing that generation.
Expiry is checked by every read; maintenance reclaims expired values and indexes
without evicting valid entries. An ordered expiry queue makes repeated capacity
refusals cheap. Rejected batches preserve all live values and indexes.

Pages are ordered by (deadline, key) with a configured maximum size. A continuation
is bound to its scope, index and exclusive deadline. It is not an authenticated
public API token or a snapshot: concurrent replacements may move in the ordering.
The API owner must bind external cursors to the requester and query. Every returned
record is live at the operation boundary. A full last page can return a cursor
whose next page is empty. Cursor expiry never extends on subsequent pages.

Cancellation before polling performs no work. An in-memory mutation has no await
once it starts; on network stores cancellation after sending may still commit.
Errors and Debug contain no names, record bytes, revisions or dependency messages.
There is no SQL fallback, logging, persistence, request history or background I/O.

The exported conformance suite runs unchanged against Memory and disposable
Valkey. It covers exact expiry, expired renewal, isolation, CAS, atomic rejection,
index replacement/removal, generation fencing, paging, cursor binding/expiry,
capacity, invalid writes, cancellation-before-poll and clock regression. Native
contention tests and identical wasm vectors supplement it.
