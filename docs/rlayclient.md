---
id: rlayclient
title: Rlay Client Getting Started
sidebar_label: Getting Started
---

The [Rlay Client](https://github.com/rlay-project/rlay-client) serves to provide multiple functions:
- A simple to use interface for interacting with Rlay's smart contracts via JSONRPC (as established by Ethereum clients)
- Mirror the state of Rlay's smart contracts for quick data retrieval
- Calculate the payout rewards for propositions in the Rlay network

## Install

### Requirements

Before continuing with the installation, make sure you have at following dependencies and their required versions installed correctly.

- **Rust 1.29.0 or newer** (Install via [rustup](https://rustup.rs/))

### Download

```bash
git clone git@github.com:rlay-project/rlay-client.git && cd rlay-client
```

### Compile

```bash
mkdir rlay_data
mkdir rlay_data/epoch_payouts
cargo build --release
```

### Run

Before running the `Rlay Client` make sure that the client connects to the specified Rlay testnet correctly.
You can run a Rlay testnet locally, by following the instructions at [Rlay Protocol](rlayprotocol.md).
To check if the `Rlay Client` can correctly connect to the testnet run:

```bash
cargo run --release -- doctor
```

You can change addresses and pointers in `rlay.config.toml`. After any changes in the config communicate the changes by running

```bash
sh update_data.sh ~/rlay-protocol/build/contracts/
```

Once the `Rlay Client` can connect properly, you can run it by executing

```bash
cargo run --release -- client
```
