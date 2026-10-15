# Penciltrade onchain programs

This repository contains the native Solana trading program, its test oracle, and the dependencies needed to build and test them.
It contains no hosted frontend, SDK, relayer, or keeper.
The production library is `penciltrade`. The test library is `penciltrade_mock_phoenix`.

The program settles synthetic exposure against shared house cash. It does not hedge positions or place Phoenix orders.
The house starts with zero cash. Unfunded profits become unsecured FIFO claims.
V4 requires a fresh program ID and fresh state. Earlier funded layouts have no migration path.

The 10M notional cap and revised PENCIL curves are deployed on mainnet.
`deployment.json` records the verified native source, upgrade slot, executable hashes, and transaction.
The loader retains upgrade authority. The program is not sealed.

## Build dependencies

The Rust workspace resides in `program/`. Its `Cargo.lock` fixes the complete crate dependency graph.
`solana-program` is pinned to 2.1.21. The SPL Token and Borsh versions also reside in the locked graph.
Native tests use Rust 1.84.1. The SBF build uses Agave 2.1.21 and platform-tools v1.43.
`build-config.json` pins verifier 0.5.2 and the official build image by digest.
The build script also requires Docker and Python 3.

Install Rust with rustup. Then run the native tests from the repository root:

```bash
cargo test --manifest-path program/Cargo.toml --workspace --lib --locked
```

The [program guide](program/README.md) maps source files to their responsibilities.
The fixture under `tests/fixtures/` is required by the native serialization test.

## Deterministic deployment build

On Ubuntu 24.04, install Docker and the checksum-pinned verifier before the production build:

```bash
mkdir -p .tools
curl --fail --location --retry 3 https://github.com/solana-foundation/solana-verifiable-build/releases/download/v0.5.2/solana-verify-0.5.2-linux -o .tools/solana-verify
printf '%s  %s\n' b9d5ccce9634dc14269cab3e7a58649bc141b3306e7735a94478c936341f211f .tools/solana-verify | sha256sum --check
chmod 755 .tools/solana-verify
PATH="$PWD/.tools:$PATH" bash scripts/build-verifiable.sh
```

For another host, install the same verifier version with a compatible compiler outside the pinned native test toolchain.
The release workflow uses Ubuntu 24.04 for the prebuilt verifier's glibc requirement.

The production artifact is `program/target/deploy/penciltrade.so`.
Deploy this deterministic artifact. A separate native SBF build can produce a different executable hash.
The test oracle must remain a local or devnet dependency.

The build workflow runs native tests and repeats the container build before comparing complete executable hashes.
It retains the production executable and its SHA-256 as build artifacts.

## Explorer verification

Publish this repository and retain the exact release commit before deployment.
Use the same image, tool version, and build arguments for repository verification.
The following command requires the deployment's loader authority to upload verification metadata:

```bash
solana-verify verify-from-repo \
  --url <mainnet-rpc-url> \
  --program-id <program-id> \
  --commit-hash <public-release-commit> \
  --mount-path . --workspace-path program \
  --library-name penciltrade \
  --base-image solanafoundation/solana-verifiable-build@sha256:c78fd4ff5399ee5ea3e154881e8991bd8064899ca162a59cafc682da5a5bd516 \
  --cargo-build-sbf-args='--tools-version v1.43' \
  --keypair <loader-authority-keypair-path> \
  https://github.com/penciltrade/onchain-programs
```

After the local hash comparison passes, upload the verification metadata when prompted.
Then submit the remote verification job:

```bash
solana-verify remote submit-job --program-id <program-id> --uploader <metadata-uploader-public-key>
```

Complete explorer verification before Seal removes loader authority.
Record the public commit, complete executable SHA-256, verification hash, program ID, and remote job result for the release.
The [Solana verification guide](https://solana.com/docs/programs/verified-builds) explains the metadata and explorer workflow.
The [verifier source](https://github.com/solana-foundation/solana-verifiable-build/tree/v0.5.2) defines the pinned CLI and image map.
Verification establishes executable/source correspondence. It does not establish an independent security audit.

## Permanent authority lifecycle

Initialization starts an upgradeable trial. The loader authority can change code and configure markets before Seal.
Seal permanently removes loader authority and freezes configuration. It refuses a paused or halted trial.
Seal retains the initial sequencer and guardian. Normal admission still requires the sequencer's signature.

After 86,400 seconds without included sequenced admission or heartbeat, any signer can explicitly call `RemoveRelayer`, instruction tag 31.
That call permanently enables public owner/delegate admission and retires guardian halt power outside local test mode.
The guardian has no early-removal bypass. Time alone does not change authority.
The FIFO queue retains its original arrival order. Order size never changes priority.

Anyone can publish verified checkpoints, drain FIFO, liquidate, harvest, and execute eligible recovery.
Funded custody withdrawals remain owner-controlled and require no relayer or oracle.
An immutable deployment has no governance, key rotation, or configuration repair path.
External oracle programs retain their own authority policies.

## Fixed limits and economics

| Rule | Value |
| --- | --- |
| Maximum leverage | 200x, subject to initialized limits |
| Minimum margin | 1 USDC |
| Maximum position notional | 250,000 USDC |
| Intent lifetime | 30 seconds |
| Owner fallback exit delay | 15 seconds |
| Admission checkpoint age | 120 seconds |
| Accepted mark movement | At most 30% from the preceding accepted mark |
| Oracle wind-down | Seven days and 756,000 slots without an advancing checkpoint |
| Mainnet stake warmup | One hour |
| Eligible trading fee | 2%, divided equally between the pinned team recipient and stakers |
| Profit buffer | 5,000 USDC of distributable equity |
| Profit allocation curve | 8% starter share, linear rise from 100,000 to 1,000,000 USDC, then 99% allocation and 1% retention |
| Oracle cache | At most one committed read per market per execution slot |
| Bounds | 8 markets, 32 oracle accounts per market, 16 actions per batch, 32 recent outcomes |
| Root ceiling | 65,536 bytes |

Global winning exposure reduces distributable equity. Only new cash-funded profit enters the distribution budget.
Paired losing-leg-first settlement cannot distribute cash ahead of reserved winning exposure.
Existing capital and debt losses do not become distributable profit.
Failed transactions roll back cache writes. Attempted calls and simulations remain outside the committed-read cap.
Stopped positions settle at the last verified frozen mark without an oracle CPI.
Unfunded claims retain unsecured FIFO repayment. Frozen settlement does not create house cash.

Mainnet mode pins canonical Circle USDC, Hawkeye, and Phoenix program identities in `program/src/lib.rs`.
The exact configuration schema, instruction ABI, arithmetic, and fixed constants reside in the Rust source.

## License

Project code and documentation use the GNU Affero General Public License, version 3 only (`AGPL-3.0-only`).
See [LICENSE](LICENSE) for the complete terms. Third-party dependencies retain their respective licenses.

## Frontend documentation

The [documentation index](docs/README.md) contains the frontend reference pages and their illustration.
The snapshot distinguishes the current program from policies queued for a later upgrade.
Offchain examples refer to tools and evidence maintained separately.
