# Review the deployed v4 release

The mainnet economics upgrade finalized at slot 457299186 on 15 October 2026.
The public native source is `e18534eae0950efeae77310213eb87f62e3a4927`.
The verified executable uses a 10M USDC notional cap and 200× maximum leverage.
The relayer, keeper, collector, and monitor run integrated release `74e4694`.
The program retains upgrade authority during operational acceptance.

PENCIL emissions remain at 100 per eligible USDC through $500k of current tracked LP.
Above $500k, the rate follows `100 × 500,000 / T`. Drawdowns increase the rate along the same curve.
The marginal LP profit share follows `8% + 32% × min(max(T, 0) / $1M, 1)^2`, capped at 40%.
For this share, `T` means LP equity after winning-exposure reserves.
The funded budget, $5k buffer, previous peak, and payout priority govern actual distributions.

The first committed Hawkeye observation fixes each market snapshot for an execution slot.
Failed transaction attempts remain outside that committed-read cap.
The funded upgrade preserves account layouts, earned credits, stake, and existing positions. Earlier v3 layouts require fresh state.

## Assess release evidence

The native suite passes 80 tests. Backend and frontend suites pass 185 and 82 tests.
The exact public executable passes 14 profit, 12 recovery, and nine hardening tests on a local validator.
The funded loader rehearsal preserves records and token accounts byte for byte.
A 1 USDC, 50× mainnet long opens and closes, with the close requested ten seconds after the observed fill.
Both admissions and both settlement transactions finalize.

Two pinned container builds produce identical executables. Remote source verification matches the public build to the onchain program.
The complete executable SHA-256 is `92f4eb5a7e8ce6ce23e9fa8dbc2e7d894b782081269c4dc5b670a99019334e11`.
The verification hash is `e555276fbbf60ca8ccb1a673111fe29df33d26d18a5097d4249dd29a0bc8521d`.
Historical throughput, oracle samples, and formal reports remain evidence for their recorded releases.

The release uses these acceptance references:

- [Mainnet acceptance criteria](mainnet-acceptance.md) define capacity, stability, fault recovery, and sealing requirements.
- [Economic patch scope](economic-patch-scope.md) defines the neutral extraction review.
- [Cache rules](oracle-slot-cache.md) define snapshot behavior and read counts.

Permanent Seal requires complete operational evidence for the final binary and configuration.
The requirements include sustained load, a 24-hour soak, elapsed recovery drills, and seven days of the final configuration.
The upgrade transaction and source verification do not satisfy those operating requirements.
