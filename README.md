# <h1 align="center"> opensea.rs </h1>

*Rust bindings & CLI to the Opensea API and Contracts*

![Github Actions](https://github.com/gakonst/opensea-rs/workflows/Tests/badge.svg)

## Development

We use the standard Rust toolchain

```
cargo check
cargo test
cargo doc --open
cargo r -- \
    --url http://localhost:8545 \
    --private-key <your private key without 0x> \
    --address <your address> \
    --ids 1 \
    --ids 2 \
    --flashbots
```

## Roadmap

* [x] Opensea API
* [x] Opensea Types (Orders etc.)
* [x] Opensea Contract clients
    * [x] ERC721
    * [x] ERC1155
    * [ ] Generalize the API to arbitrary Opensea marketplace schemas
* [x] CLI for operations

## Running ignored tests

1. Create a `hardhat.config.js` file and fork from mainnet at this block:

```
export default {
  networks: {
    hardhat: {
      forking: {
        url: "https://eth-mainnet.alchemyapi.io/v2/6qChHD3n9AMo1hC4luVgYMPSqoOwP3II",
        blockNumber: 13037331,
      },
      hardfork: "london",
    }
  }
}
```

2. `cargo test --ignored`

## CLI Usage

```
$ ./target/debug/opensea-cli -h
opensea-cli 0.1.0

USAGE:
    opensea-cli [FLAGS] [OPTIONS] --address <address> --private-key <private-key> --url <url>

FLAGS:
    -f, --flashbots
    -h, --help         Prints help information
    -V, --version      Prints version information

OPTIONS:
    -a, --address <address>            The NFT address you want to buy
    -i, --ids <ids>...                 The NFT id you want to buy
    -p, --private-key <private-key>    Your private key string
    -u, --url <url>                    The tracing / archival node's URL
```
