use ethers::{prelude::*, types::transaction::eip2718::TypedTransaction, utils::parse_units};
use opensea::{api::OpenSeaApiConfig, BuyArgs, Client};
use std::io::{self, BufRead};
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

    #[structopt(
        long,
        help = "The file containing the NFT id(s) you want to buy"
    )]
    ids_path: Option<PathBuf>,

    #[structopt(long)]
    bribe_receiver: Option<Address>,

    #[structopt(long, parse(from_str = parse_u256))]
    bribe: Option<U256>,
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

    let chain_id = provider.get_chainid().await?.as_u64();
    let signer = LocalWallet::from_str(&opts.private_key)?.with_chain_id(chain_id);
    let taker = signer.address();

    // set up the args
    let block = provider.get_block(BlockNumber::Latest).await?.unwrap();
    let timestamp = block.timestamp.as_u64();

    // Add signer and Flashbots middleware
    if let Some(bribe) = opts.bribe {
        let bundle_signer = LocalWallet::new(&mut ethers::core::rand::thread_rng());
        let provider = FlashbotsMiddleware::new(
            provider,
            url::Url::parse("https://relay.flashbots.net")?,
            bundle_signer,
        );

        let provider = SignerMiddleware::new(provider, signer);
        let provider = Arc::new(provider);
        let opensea = Client::new(provider, OpenSeaApiConfig::default());
        let client = (*opensea.contracts).client();

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

        // 1. construct the transactions w/ pre-calculated nonces
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

        let mut nonce = client.get_transaction_count(taker, None).await?;
        // TODO: Can we convert this to an iterator?
        let txs = {
            let mut txs = Vec::new();
            for id in ids {
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

        // 2. if an address is explicitly specified to receive the bribe, add an extra
        // tx to the bundle, if not, spread the tx fee evenly across all txs
        let txs = match opts.bribe_receiver {
            Some(bribe_receiver) => {
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

        // 3. Create the signed txs bundle
        let bundle = {
            let mut bundle = ethers_flashbots::BundleRequest::new();
            for tx in txs {
                let tx = tx.into();
                let signature = client.signer().sign_transaction(&tx).await?;
                let rlp = tx.rlp_signed(chain_id, &signature);
                bundle = bundle.push_transaction(rlp);
            }
            bundle
        };

        // set the block bundle
        let num = block.number.unwrap();
        let bundle = bundle.set_block(num).set_simulation_block(num);

        // 4. Send it!
        let simulated_bundle = client.inner().simulate_bundle(&bundle).await?;
        println!("Simulated bundle: {:?}", simulated_bundle);
        let pending_bundle = client.inner().send_bundle(&bundle).await?;
        let res = pending_bundle.await?;
        println!("Bundle executed: {:?}", res);
    } else {
        let provider = SignerMiddleware::new(provider, signer);
        let provider = Arc::new(provider);
        let opensea = Client::new(provider.clone(), OpenSeaApiConfig::default());

        let args = BuyArgs {
            token_id: *opts.ids.get(0).unwrap(),
            taker,
            token: opts.address,
            recipient: taker,
            timestamp: Some(timestamp - 100),
        };

        let nft = NFT::new(opts.address, provider);

        let balance = nft.balance_of(args.recipient, args.token_id).call().await?;
        println!(
            "BalanceOf owner of NFT {:?} before is {:?}",
            args.token_id, balance
        );

        // execute the call
        let call = opensea.buy(args.clone()).await.unwrap();

        // TODO: Automatic gas estimation for 1559 txs
        let call = call.gas_price(parse_units(100, 9).unwrap());

        let sent = call.send().await.unwrap();
        println!("Sent tx {:?}", *sent);

        // wait for it to be confirmed
        let receipt = sent.await.unwrap().unwrap();

        println!("Confirmed!");
        assert_eq!(receipt.status.unwrap(), 1.into());

        let balance = nft.balance_of(args.recipient, args.token_id).call().await?;
        println!(
            "BalanceOf owner of NFT {:?} after is {:?}",
            args.token_id, balance
        );
    }

    Ok(())
}
