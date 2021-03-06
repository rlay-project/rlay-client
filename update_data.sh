#!/usr/bin/env bash
set -euxo pipefail

source_dir=$1

cat $source_dir/OntologyStorage.json | jq .abi > data/OntologyStorage.abi

cat $source_dir/RlayToken.json | jq .abi > data/RlayToken.abi

cat $source_dir/PropositionLedger.json | jq .abi > data/PropositionLedger.abi
