# rlay-client

Client implementation for the Ɍlay protocol, a Decentralized Information Network.

Currently the main purpose of the client is to calculate the network rewards and submit them to the core protocol ([rlay-protocol][rlay-protocol-github]) via a Merkle Tree.

The [whitepaper][rlay-whitepaper] gives a outline of the theoretical foundations of the protocol.

## Running

Steps to run `rlay-client` from this repository:

  - Clone the repository
  - Make sure you have the prerequsites for [rquantiles][rquantiles-github] installed
  - If you want to run a local network for development:
    - Spawn a local Ethereum RPC and deploy [rlay-protocol][rlay-protocol-github]
  - Make sure that the RPC addresses and the contract addresses are set correctly in the [config file][rlay-config-file]
  - `cargo run -- client`


If you encounter any problems while trying to run rlay-client you can try to use the following command to pinpoint your problems:
```bash
cargo run -- doctor
```

### Via Docker

We provide a Docker image which can be used to run `rlay-client`. I currently has the assumption that it is used with a single Neo4J backend at `127.0.0.1:7474`, which requires it to be run with `--net=host`. You can use it with another configuration by building your own Docker image based on it and overwriting the `/rlay.config.toml` file.

```
docker run --net=host rlayproject/rlay-client
```


## Contributing & Contact

We are very open to contributions! Feel free to open a [Github issue][github-issues], or a Pull Request.

If you want to get in contact you can find us here:

  - [Matrix chat room][matrix-chat] - development focused chat
  - [Telegram channel][telegram-chat] - general Rlay discussion

> Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as below, without any additional terms or conditions.

## License

Licensed under either of

  * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
  * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

[rlay-config-file]: ./rlay.config.toml
[rquantiles-github]: https://github.com/hobofan/rquantiles
[github-issues]: https://github.com/rlay-project/rlay-client/issues
[matrix-chat]: https://matrix.to/#/#rlay:matrix.org
[rlay-protocol-github]: https://github.com/rlay-project/rlay-protocol
[rlay-whitepaper]: https://rlay.com/rlay-whitepaper.pdf
[telegram-chat]: https://t.me/rlay_official
