use ethers::{prelude::*, types::transaction::eip2718::TypedTransaction, utils::parse_units};
use opensea::{api::OpenSeaApiConfig, BuyArgs, Client};
use std::io::BufRead;
use std::{convert::TryFrom, fs::File, path::PathBuf, str::FromStr, sync::Arc};

use structopt::StructOpt;

use ethers_flashbots::FlashbotsMiddleware;

#[derive(StructOpt, Debug, Clone)]
struct Opts {
    #[structopt(long, short, help = "The tracing / archival node's URL")]
    url: String,

    #[structopt(long, short, help = "Your private key string")]
    private_key: String,

    #[structopt(long, short, help = "The NFT address you want to buy")]
    address: Address,

    #[structopt(long, help = "The NFT id(s) you want to buy", parse(from_str = parse_u256))]
    ids: Vec<U256>,

    #[structopt(long, help = "The file containing the NFT id(s) you want to buy")]
    ids_path: Option<PathBuf>,

    #[structopt(long)]
    bribe_receiver: Option<Address>,

    #[structopt(long, parse(from_str = parse_u256))]
    bribe: Option<U256>,

    #[structopt(
        long,
        help = "Whether you're buying an ERC721 or an ERC1155 (true for 1155)"
    )]
    erc1155: bool,
}

fn parse_u256(s: &str) -> U256 {
    U256::from_dec_str(s).unwrap()
}

ethers::contract::abigen!(
    NFT,
    r#"[
        function ownerOf(uint256) view returns (address)
        function balanceOf(address,uint256) view returns (uint256)
    ]"#
);

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    let opts = Opts::from_args();

    let provider = Provider::try_from(opts.url.as_str())?;
    let provider = Arc::new(provider);
    let nft = NFT::new(opts.address, provider.clone());

    let chain_id = provider.get_chainid().await?.as_u64();
    let signer = LocalWallet::from_str(&opts.private_key)?.with_chain_id(chain_id);
    let taker = signer.address();

    // set up the args
    let block = provider.get_block(BlockNumber::Latest).await?.unwrap();
    let timestamp = block.timestamp.as_u64();

    let args = BuyArgs {
        token_id: 0.into(),
        taker,
        token: opts.address,
        recipient: taker,
        timestamp: Some(timestamp - 100),
    };

    let base_fee = block.base_fee_per_gas.expect("No basefee found");
    // get the max basefee 5 blocks in the future
    let mut max_base_fee = base_fee;
    for _ in 0..5 {
        max_base_fee *= 1125 / 1000;
    }

    // read the token ids
    let ids = if let Some(ids_path) = opts.ids_path {
        let file = File::open(ids_path)?;
        let lines = std::io::BufReader::new(file).lines();
        let mut ids = Vec::new();
        for line in lines {
            let line = line?;
            let id = U256::from_dec_str(&line)?;
            ids.push(id);
        }
        ids
    } else {
        opts.ids.clone()
    };

    // 1. construct the transactions w/ pre-calculated nonces
    let mut nonce = provider.get_transaction_count(taker, None).await?;
    let opensea = Client::new(provider.clone(), OpenSeaApiConfig::default());
    // TODO: Can we convert this to an iterator?
    let txs = {
        let mut txs = Vec::new();
        for id in &ids {
            let mut args = args.clone();
            args.token_id = id.into();
            let tx = opensea.buy(args).await.unwrap().tx;

            // get the 1559 inner tx to configure the basefee
            let mut tx = match tx {
                TypedTransaction::Eip1559(inner) => inner,
                _ => panic!("Did not expect non-1559 tx"),
            };

            // initialize the max base fee value, without any priority fee
            tx.max_fee_per_gas = Some(max_base_fee);

            // set & increment the nonce
            tx.nonce = Some(nonce);
            nonce += 1.into();

            txs.push(tx)
        }
        txs
    };

    println!("Querying current owners...");

    for id in &ids {
        if opts.erc1155 {
            let balance = nft.balance_of(args.recipient, *id).call().await?;
            println!(
                "Before: {:?} owns {:?} ERC1155 NFTs with token id {:?}",
                args.recipient, balance, id
            );
        } else {
            let owner = nft.owner_of(*id).call().await?;
            println!(
                "Before: Owner of ERC721 NFTs with token id {:?}: {:?}",
                id, owner
            );
        }
    }

    if let Some(bribe) = opts.bribe {
        // Add signer and Flashbots middleware
        let bundle_signer = LocalWallet::new(&mut ethers::core::rand::thread_rng());
        let provider = FlashbotsMiddleware::new(
            provider,
            url::Url::parse("https://relay.flashbots.net")?,
            bundle_signer,
        );
        let provider = SignerMiddleware::new(provider, signer);
        let provider = Arc::new(provider);

        // if an address is explicitly specified to receive the bribe, add an extra
        // tx to the bundle, if not, spread the tx fee evenly across all txs
        let txs = match opts.bribe_receiver {
            Some(bribe_receiver) => {
                println!(
                    "Adding bribe tx to the bundle. Bribe Receiver {:?}, Amount: {:?}",
                    bribe_receiver, bribe
                );
                let mut txs = txs;
                let tx = Eip1559TransactionRequest::new()
                    .to(bribe_receiver)
                    .value(bribe)
                    .into();
                txs.push(tx);
                txs
            }
            None => {
                let priority_fee_per_tx = bribe / opts.ids.len();
                println!(
                    "Splitting bribe across {:?} txs in the bundle. Amount per tx: {:?}",
                    opts.ids.len(),
                    priority_fee_per_tx
                );
                txs.into_iter()
                    .map(|mut tx| {
                        // bump the max base fee by the priority fee
                        tx.max_fee_per_gas
                            .as_mut()
                            .map(|max_base_fee| *max_base_fee += priority_fee_per_tx);
                        tx.max_priority_fee_per_gas = Some(priority_fee_per_tx);
                        tx
                    })
                    .collect()
            }
        };

        // Create the signed txs bundle
        let bundle = {
            let mut bundle = ethers_flashbots::BundleRequest::new();
            for (tx, id) in txs.into_iter().zip(&ids) {
                println!(
                    "[TokenId = {:?}] Signing bundle tx with {:?} ETH",
                    id,
                    tx.value.unwrap()
                );
                let tx = tx.into();
                let signature = provider.signer().sign_transaction(&tx).await?;
                let rlp = tx.rlp_signed(chain_id, &signature);
                bundle = bundle.push_transaction(rlp);
            }
            bundle
        };

        // set the block bundle
        let num = block.number.unwrap();
        let bundle = bundle.set_block(num).set_simulation_block(num);

        // 4. Send it!
        let simulated_bundle = provider.inner().simulate_bundle(&bundle).await?;
        println!("Simulated bundle: {:?}", simulated_bundle);
        let pending_bundle = provider.inner().send_bundle(&bundle).await?;
        let res = pending_bundle.await?;
        println!("Bundle executed: {:?}", res);
    } else {
        let provider = SignerMiddleware::new(provider, signer);
        let provider = Arc::new(provider);

        for (tx, id) in txs.into_iter().zip(&ids) {
            let fee = parse_units(100, 9).unwrap();
            let tx = tx.max_fee_per_gas(fee);

            println!(
                "[Token Id = {:?}] Sending tx with {:?} ETH ",
                id,
                tx.value.unwrap()
            );
            let pending_tx = provider.send_transaction(tx, None).await?;
            println!("[Token Id = {:?}] Sent tx {:?}", id, *pending_tx);
            let receipt = pending_tx.await?.unwrap();
            assert_eq!(receipt.status, Some(1.into()));
        }
    }

    for id in &ids {
        if opts.erc1155 {
            let balance = nft.balance_of(args.recipient, *id).call().await?;
            println!(
                "After: {:?} owns {:?} ERC1155 NFTs with token id {:?}",
                args.recipient, balance, id
            );
        } else {
            let owner = nft.owner_of(*id).call().await?;
            println!(
                "After: Owner of ERC721 NFTs with token id {:?}: {:?}",
                id, owner
            );
        }
    }

    Ok(())
}
