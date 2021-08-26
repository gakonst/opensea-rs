# <h1 align="center"> opensea.rs </h1>

*Rust bindings & CLI to the Opensea API and Contracts*

![Github Actions](https://github.com/gakonst/opensea-rs/workflows/Tests/badge.svg)

## CLI Usage

Run `cargo r -- --help` to get the top level help menu:

```
opensea-cli 0.1.0
Choose what NFT subcommand you want to execute

USAGE:
    opensea-cli <SUBCOMMAND>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

SUBCOMMANDS:
    buy       Purchase 1 or more NFTs, with optional Flashbots support
    deploy    Deploy the Ethereum contract for doing consistency checks inside a Flashbots bundle
    help      Prints this message or the help of the given subcommand(s)
    prices    Get OpenSea orderbook information about the token
```

To view each individual subcommand's help menu, run: `opensea-cli <subcommand name> --help`

### Buying NFT(s)

Here's an example command for purchasing some ERC1155 NFTs using Flashbots:

```bash
cargo run buy \
    --nft.erc1155 \
    --nft.address "0xTheNFTAddress" \
    --nft.ids 1 --nft.ids 2 --nft.ids 3 \
    --eth.private_key "0xMyPrivateKey" \
    --eth.url http://localhost:8545 \
    --flashbots.bribe 1000000000000000000 \
    --flashbots.bribe_receiver 0xYourBriberContract
```

Instead of providing `nft.ids`, you can also provide a CSV file via the `--nft.ids_path` command,
where the first column contains the `id` of the NFT and the second column contains 
the `quantity` of purchased NFT.

Here's an ERC1155 example (which also requires passing the `--nft.erc1155` flag)

```
1,1
2,5
3,2
```

And an ERC721 example

```
1
2
3
```

**Flashbots Support**: This will proceed to create a Flashbots bundle with 4 transactions: 3 NFT take orders on
OpenSea, and a 4th transaction which sends the bribe to the briber contract while also doing
consistency checks that we have received the NFTs

**Public Mempool**: If you omit the Flashbots parameters, it'll proceed to submit the transactions normally
via the public mempool.

## Development

### Rust Toolchain

We use the stable Rust toolchain. Install by running: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`

### Building & testing

```
cargo check
cargo test
cargo doc --open
cargo build [--release]
```

## Features

* [x] Opensea API
* [x] Opensea Types (Orders etc.)
* [x] Opensea Contract clients
    * [x] ERC721
    * [x] ERC1155
    * [x] Fill a Sell order
    * [ ] Generalize the API to arbitrary Opensea marketplace schemas
* [x] CLI for operations
    * [x] Flashbots contract deployer
    * [x] Query prices
    * [x] Purchase NFT(s)
    * [ ] Sniping drops (pre-configuring the target and looping)

## Running ignored tests

1. Create a `hardhat.config.js` file and fork from mainnet at this block:

```
export default {
  networks: {
    hardhat: {
      forking: {
        url: "https://eth-mainnet.alchemyapi.io/v2/<YOUR API KEY>",
        blockNumber: 13037331,
      },
      hardfork: "london",
    }
  }
}
```

2. `cargo test --ignored`


## Acknowledgements

`Briber.sol` contract written by [`Anish Agnihotri`](https://github.com/Anish-Agnihotri/)
