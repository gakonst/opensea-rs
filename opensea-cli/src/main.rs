use ethers::{prelude::*, utils::parse_units};
use opensea::{api::OpenSeaApiConfig, BuyArgs, Client};
use std::{convert::TryFrom, str::FromStr, sync::Arc};

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

    #[structopt(long, short, help = "The NFT id you want to buy", parse(from_str = parse_u256))]
    ids: Vec<U256>,

    #[structopt(long, short)]
    bribe_receiver: Option<Address>,

    #[structopt(long, short, parse(from_str = parse_u256))]
    bribe: Option<U256>,
}

fn parse_u256(s: &str) -> U256 {
    U256::from_dec_str(s).unwrap()
}

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
    if let Some(bribe_receiver) = opts.bribe_receiver {
        let bribe = opts.bribe.expect("no bribe amount set");

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

        let mut bundle = ethers_flashbots::BundleRequest::new();
        for id in opts.ids {
            let args = BuyArgs {
                token_id: id,
                taker,
                token: opts.address,
                recipient: taker,
                timestamp: Some(timestamp - 100),
            };

            let call = opensea.buy(args).await.unwrap();
            let signature = client.signer().sign_transaction(&call.tx).await?;
            let rlp = call.tx.rlp_signed(chain_id, &signature);
            bundle = bundle.push_transaction(rlp);
        }

        // TODO: Add any extra calldata for consistency checks
        let tx = Eip1559TransactionRequest::new()
            .to(bribe_receiver)
            .value(bribe)
            .into();
        let signature = client.signer().sign_transaction(&tx).await?;
        let rlp = tx.rlp_signed(chain_id, &signature);
        bundle = bundle.push_transaction(rlp);

        let simulated_bundle = client.inner().simulate_bundle(&bundle).await?;
        println!("Simulated bundle: {:?}", simulated_bundle);
        let res = client.inner().send_bundle(&bundle).await?;
        println!("Bundle executed: {:?}", res);
    } else {
        let provider = SignerMiddleware::new(provider, signer);
        let provider = Arc::new(provider);
        let client = Client::new(provider, OpenSeaApiConfig::default());

        let args = BuyArgs {
            token_id: *opts.ids.iter().next().unwrap(),
            taker,
            token: opts.address,
            recipient: taker,
            timestamp: Some(timestamp - 100),
        };

        // execute the call
        let call = client.buy(args).await.unwrap();

        // TODO: Automatic gas estimation for 1559 txs
        let call = call.gas_price(parse_units(100, 9).unwrap());

        let sent = call.send().await.unwrap();
        println!("Sent tx {:?}", *sent);

        // wait for it to be confirmed
        let receipt = sent.await.unwrap().unwrap();

        println!("Confirmed!");
        assert_eq!(receipt.status.unwrap(), 1.into());
    }

    Ok(())
}
