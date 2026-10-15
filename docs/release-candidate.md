# Review the integrated v4 candidate

The candidate branch is `release/mainnet-rc-20261013`. It combines the upgradeable trial, throughput work, hardening, revised economics, and Hawkeye slot cache. A cache stores an observation for later reuse. It starts from `origin/feat/upgradeable-until-locked` and merges `origin/fix/staker-share-cap` before replacing that fixed cap. Formal-verification work in the original checkout remains separate.

The staker share starts at 8%, rises linearly through $100k-$1M of distributable equity, and remains 99% above $1M. Global counters for winning exposure and a funded profit budget protect the allocation source. Winning exposure covers possible payouts to open winners. The first committed Hawkeye observation fixes each market's snapshot for an execution slot. Failed transaction attempts remain outside the committed-read cap.

V4 requires fresh program state. An existing v3 root cannot upgrade in place. Before uploading code, the deployment script makes sure that the layout is compatible. This candidate has no public devnet or mainnet deployment.

## Assess evidence and remaining work

Evidence resides in `artifacts/release-candidate-evidence/` and the corresponding localnet reports. The native suite includes 672 paired-position scenarios and 2,000 split-settlement cases. Compiled tests exercise token custody, rewards, recovery, sealing, market authorization, and the oracle cache. The final manifest records hashes for source, binaries, dependencies, and evidence. Historical throughput, oracle samples, and formal reports do not establish this candidate's behavior.

Use these references for further tests:

- [Operational acceptance criteria](mainnet-acceptance.md) for capacity, stability, fault recovery, and locking.
- [Economic patch scope](economic-patch-scope.md) for the focused strategy review.
- [Cache rules](oracle-slot-cache.md) for snapshot behavior and read counts.

Sealing still requires genuine Hawkeye batches, v1 execution on a current runtime, sustained production configuration, and elapsed-time recovery drills. The [mainnet preparation review](../mainnet-prep.md) lists additional patches, final tests, trial limits, and remaining sealing gates. The [release manifest](../artifacts/release-candidate.json) binds current source, SBF binaries, and evidence hashes. SBF is Solana's compiled program format.
