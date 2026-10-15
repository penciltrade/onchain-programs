# Frontend documentation

This snapshot contains all 20 Markdown reference pages bundled with the Penciltrade frontend.
The Rust source defines the deployed program. The reference pages describe its active economics and current release status.
Operational acceptance continues before permanent Seal. The countdown remains separate from the deployed program state.
The documentation also describes offchain tools and release evidence maintained outside this repository.
Portable placeholders replace host-specific executable paths.
The documentation uses the repository's [AGPL-3.0-only license](../LICENSE).

## Reference pages

- [PENCILTRADE](01-overview.md)
- [Follow the trading rules](02-trading.md)
- [Use the queues](03-queues.md)
- [Track USDC](04-accounting.md)
- [Manage PENCIL](05-ownership.md)
- [Price trades with the Phoenix mark](06-oracle.md)
- [Use the programs](07-programs.md)
- [Operate the deployment](08-operations.md)
- [Review Phoenix BBO measurements](09-measured-phoenix.md)
- [Handle congestion](10-congestion.md)
- [Recover an immutable deployment](11-recovery.md)
- [Define the neutral extraction patch](economic-patch-scope.md)
- [Prepare the immutable release](immutability-preparation.md)
- [Release immutable code](immutable-release.md)
- [Mainnet acceptance criteria](mainnet-acceptance.md)
- [Mainnet service monitoring](mainnet-operations.md)
- [Cache Hawkeye observations by slot](oracle-slot-cache.md)
- [Public opening checklist — 15 October 2026](public-launch-2026-10-15.md)
- [Review the deployed v4 release](release-candidate.md)
- [Review devnet throughput tests](throughput-bench.md)

[snapshot.json](snapshot.json) records hashes for the pages and illustration.
It also records the portable changes in this copy.
