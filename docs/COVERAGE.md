# Coverage contract

CI targets 100% of reachable production lines and branches. Stable Rust runs
the existing native and wasm checks. Nightly Rust is used only for LLVM branch
instrumentation, which currently requires it. Both jobs use the same resolved
Cargo.lock snapshot; all build and test commands after resolution use --locked.

The coverage job executes real tests with cargo-llvm-cov and retains the raw JSON
even when the gate fails. The checker requires positive execution for every emitted source line and
branch in the same execution's upstream LCOV export. Rounded percentages and an
empty report cannot pass.
The report excludes integration-test harness files under tests/, not production
code. No production source exclusions are currently approved.

A failing gate is missing evidence, not permission to lower the threshold or
change domain behavior. Add meaningful failure and round-trip tests. Document
any genuinely unreachable defensive branch precisely before excluding it. Native
coverage does not establish browser execution; keep the actual wasm vectors.

First-party dependencies follow main. Their resolved full revisions remain in
Cargo.lock, with exactly one source per first-party crate. Dependabot maintains
committed snapshots; CI refreshes once per run and retains the tested snapshot.
Auto-merge requires protected main and successful substantive checks on the
exact current Dependabot head. It never executes PR code with write permissions.

The Memory secondary maps are updated only together with their record under the
same mutex. Deleting a committed record therefore requires its index and
generation rows; missing rows are an internal invariant violation, not an
ignored optional deletion. A poisoned mutex makes the store unavailable. The
public tests cover replacement, shared indexes, generation deletion and expiry;
a native test also poisons the actual mutex and verifies the fence.

Page deadline arithmetic uses the validated bounds: observed time is at most
2^53 - 1 and TTL at most 31,536,000,000 milliseconds, so their sum cannot overflow
u64. Deadline validation still refuses sums beyond the supported time range; a
public boundary test covers that refusal. Neither simplification excludes source
from measurement.

Coverage is per production source location, not per generic instantiation. LLVM
JSON summaries can retain an uncovered copy of a generic function even when
other instantiations execute both outcomes; its LCOV DA/BRDA records aggregate
these source locations. CI keeps the original JSON, annotated source and prior
JSON diagnostic checker. The strict gate checks every emitted DA and BRDA record
without exclusions; it does not use inconsistent summary LF/LH/BRF/BRH totals or
claim every instantiation was covered.

The shared policy checker also requires the same raw JSON file inventory,
complete branch-location inventory and matching upstream summary metadata.
Truncated, duplicate or malformed LCOV records fail before coverage is counted.
