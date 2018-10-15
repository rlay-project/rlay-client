---
id: rlayclient
title: Rlay Client
sidebar_label: Overview
---

[Rlay Client](https://github.com/rlay-project/rlay-client) provides an interface for interacting with and receiving the state of Rlay's smart contracts via the Ethereum RPC. It also calculates the received payout rewards.

## Install

### Requirements

Before continuing with the installation, make sure you have at following libraries and their required versions installed correctly.

- Rust 1.29.0
- @TODO

### Download

```
git clone git@github.com:rlay-project/rlay-client.git && cd rlay-client
```

### Compile

```
mkdir rlay_data
mkdir rlay_data/epoch_payouts
cargo build --release -- client
```

### Run

Before running the `Rlay Client` make sure that the client connects to the specified Rlay testnet correctly. You can run a Rlay testnet locally, by following the instructions at [Rlay Protocol](). To check if the `Rlay Client` connects correctly to the Rlay testnet run

```
cargo run --release -- doctor
```

You can change addresses and pointers in `rlay.config.toml`. After any changes in the config communicate the changes by running

```
sh update_data.sh ~/rlay-protocol/build/contracts/
```

Once the `Rlay Client` can connect properly, you can run it by executing

```
cargo run --release -- client
```

## Develop

### API

`Rlay Client` exposes the following RPC endpoints to interact with the Rlay testnet.

#### `getPropositionPools`

Returns all Proposition Pools found

Example Usage:

```
```

#### `experimentalListCids`

Returns all stored CIDs

Example Usage:

```
```

#### `experimentalKindForCid`

? @TODO

Example Usage:

```
```

#### `experimentalListCidsIndex`

? @TODO

Example Usage:

```
```

#### `experimentalGetEntity`

? @TODO

Example Usage:

```
```

#### `experimentalGetEntityCid`

? @TODO

Example Usage:

```
```

#### `version`

? @TODO

Example Usage:

```
```
