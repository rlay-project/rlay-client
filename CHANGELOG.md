## 0.1.2 - 2018-08-02

### Features

- Add `rlay_getPropositionPools` RPC method for providing basic information about proposition pools
- Add information about used contract addresses to `rlay_version` RPC method

## 0.1.1 - 2018-07-26

### Features

- Read epoch start block from contract (see [#4](https://github.com/rlay-project/rlay-client/issues/4))
- Store calculated payouts on disk
- Add basic RPC capabilities
- Add `rlay_version` RPC method
- Add cargo features for IPC and WS transport of upstream Ethereum RPC (`transport_ws` and `transport_ipc`)

### Refactor

- Store state into combined structs
- Change URLs for git dependencies to be easier to clone

## 0.1.0 - 2018-07-06 - Initial Release 
