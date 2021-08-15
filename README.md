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

* [ ] Opensea API
* [ ] Opensea Types (Orders etc.)
* [ ] Opensea Contract clients
    * [ ] Generalize the API to arbitrary Opensea marketplace schemas
* [ ] CLI for operations

## Running ignored tests

1. Create a `hardhat.config.js` file and fork from mainnet at this block:

```
export default {
  networks: {
    hardhat: {
      forking: {
        url: "https://eth-mainnet.alchemyapi.io/v2/6qChHD3n9AMo1hC4luVgYMPSqoOwP3II",
        blockNumber: 13031640,
      },
      hardfork: "london",
    }
  }
}
```

2. `cargo test --ignored`
