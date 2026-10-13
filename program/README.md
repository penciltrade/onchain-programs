# Onchain programs

`penciltrade` is the production trading program. `penciltrade_mock_phoenix` is a separate authority-controlled test oracle.
Mainnet mode rejects the test oracle. Only the production trading executable belongs in a mainnet deployment.

| Source | Responsibility |
| --- | --- |
| `src/types.rs` | Borsh instructions, root state, records, and fixed limits |
| `src/math.rs` | Integer PnL, winning haircut, bust rule, emissions, and profit curve |
| `src/ledger.rs` | Funded cash, FIFO debt, fees, and staking rewards |
| `src/storage.rs` | Record PDAs, global subset accounting, and rent |
| `src/lib.rs` | Custody, oracle CPI, admission, FIFO execution, authority, and recovery |
| `src/recovery_tests.rs` | Recovery boundaries and native entrypoint fixtures |
| `src/hardening_tests.rs` | Admission, exposure reserves, economics, and oracle cache checks |
| `mock-oracle/src/lib.rs` | Test quote accounts and a Hawkeye-compatible view |

The root PDA uses the seed `state`. Record schemas and instruction tags reside in `src/types.rs`.
USDC and PENCIL use six decimals. Prices use micro-USD.
Fresh v4 state is required. Earlier funded state has no migration path.
The test fixture under `../tests/fixtures/` supports the native state serialization test.

See the [repository guide](../README.md) for builds, fixed limits, and explorer verification.
