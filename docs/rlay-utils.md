---
id: rlay-utils
title: Rlay Utils - Getting Started
sidebar_label: Getting Started
---

`rlay-utils` is a set of CLI tools for interacting with the Rlay network for common tasks

`rlay-utils` comes with the following commands:

  - `rlay-seed`: Helps with seeding entities to the network
  - `rlay-dump`: Dumps the entities in the network to a file usable with `rlay-seed`
  - `rlay-sync-redis-search`: Synchronizes entities from the network into a [RediSearch](https://oss.redislabs.com/redisearch/) instance, to make them available for searching

## Installation

`rlay-utils` is provided via the NPM package `@rlay/utils`.

### Install for node project

```bash
npm install --save-dev @rlay/utils
```

### Install globally

```bash
npm install -g @rlay/utils
```
