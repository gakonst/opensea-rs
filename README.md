# <h1 align="center"> opensea.rs </h1>

*Rust bindings & CLI to the Opensea API and Contracts*

![Github Actions](https://github.com/gakonst/opensea-rs/workflows/Tests/badge.svg)

## Development

We use the standard Rust toolchain

```
cargo check
cargo test
cargo doc --open
cargo run
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
./target/debug/opensea-cli -h
Usage: ./target/debug/opensea-cli [OPTIONS]

Optional arguments:
  -h, --help
  -u, --url URL          The tracing / archival node's URL (default: http://localhost:8545)
  -p, --private-key PRIVATE-KEY
                         Your private key string
  -a, --address ADDRESS  The NFT address you want to buy
  -i, --id ID            The NFT id you want to buy
```
