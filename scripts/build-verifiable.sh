#!/usr/bin/env bash
set -euo pipefail
PENCIL_BUILD_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PENCIL_BUILD_ROOT"
command -v docker >/dev/null || { echo 'Docker is required for the verifiable build.' >&2; exit 1; }
command -v solana-verify >/dev/null || { echo 'Install solana-verify 0.5.2 first.' >&2; exit 1; }
python3 - <<'PYCONFIG'
import json, subprocess
from pathlib import Path
c = json.loads(Path('build-config.json').read_text())
version = subprocess.check_output(['solana-verify', '--version'], text=True).strip()
if version.split()[-1] != c['verifierVersion']:
    raise SystemExit('Expected solana-verify ' + c['verifierVersion'] + ', received ' + version)
root = Path.cwd()
command = ['solana-verify', 'build', str(root), '--workspace-path', str(root / c['workspacePath']),
    '--library-name', c['libraryName'], '--base-image', c['baseImage'],
    '--cargo-build-sbf-args=' + c['cargoBuildSbfArgs']]
if c['cargoArgs']:
    command.extend(['--', *c['cargoArgs']])
subprocess.run(command, check=True)
subprocess.run(['solana-verify', 'get-executable-hash', str(root / c['workspacePath'] / 'target/deploy/penciltrade.so')], check=True)
PYCONFIG
