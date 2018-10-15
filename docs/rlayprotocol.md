---
id: rlayprotocol
title: Rlay Protocol Getting Started
sidebar_label: Getting Started
---

[Rlay Protocol](https://github.com/rlay-project/rlay-protocl) provides the Solidity smart contracts required for the Rlay protocol to be executed on an EVM smart contract blockchain such as Ethereum.

## Install

### Requirements

Before continuing with the installation, make sure you have at following libraries and their required versions installed correctly.

- **Node.js with NPM**
  - Install via one of:
    - [NVM](https://github.com/creationix/nvm) (Recommended)
    - [Official Node.js installation](https://nodejs.org/en/download/)

### Setup

```bash
git clone git@github.com:rlay-project/rlay-protocol.git && cd rlay-protocol
```

#### Install dependencies

```bash
npm install
```

### Run testnet

To spawn a development blockchain run:

```bash
npm run testnet
```

### Deploy Smart Contracts

To build and deploy the Rlay smart contracts run the following in a seperate window to the running testnet:

```bash
NODE_OPTIONS="--max-old-space-size=120000" npm run deploy
```

> The `NODE_OPTIONS="--max-old-space-size=120000"` environment is currently needed, since part of the Rlay contracts are automatically generated (= a lot of code) and Truffle exceeds the default Node.js memory limits while compiling it.
