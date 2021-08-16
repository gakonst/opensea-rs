use ethers::{prelude::*, utils::parse_units};
use gumdrop::Options;
use opensea::{api::OpenSeaApiConfig, BuyArgs, Client};
use std::{convert::TryFrom, str::FromStr, sync::Arc};

#[derive(Debug, Options, Clone)]
struct Opts {
    help: bool,

    #[options(
        default = "http://localhost:8545",
        help = "The tracing / archival node's URL"
    )]
    url: String,

    #[options(help = "Your private key string")]
    private_key: String,

    #[options(help = "The NFT address you want to buy")]
    address: Address,

    #[options(
        help = "The NFT id you want to buy",
        parse(try_from_str = "U256::from_dec_str")
    )]
    id: U256,
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    let opts = Opts::parse_args_default_or_exit();

    let provider = Provider::try_from(opts.url)?;
    let chain_id = provider.get_chainid().await?.as_u64();
    let signer = LocalWallet::from_str(&opts.private_key)?.with_chain_id(chain_id);
    let taker = signer.address();
    let provider = SignerMiddleware::new(provider, signer);

    // set up the args
    let block = provider.get_block(BlockNumber::Latest).await?.unwrap();
    let timestamp = block.timestamp.as_u64();
    let args = BuyArgs {
        token_id: opts.id,
        taker,
        token: opts.address,
        recipient: taker,
        timestamp: Some(timestamp - 100),
    };

    // instantiate the client
    let provider = Arc::new(provider);
    let client = Client::new(provider, OpenSeaApiConfig::default());

    // execute the call
    let call = client.buy(args).await.unwrap();
    dbg!(&hex::encode(call.calldata().unwrap()));

    // TODO: Automatic gas estimation for 1559 txs
    let call = call.gas_price(parse_units(100, 9).unwrap());

    let sent = call.send().await.unwrap();
    println!("Sent tx {:?}", *sent);

    // wait for it to be confirmed
    let _receipt = sent.await.unwrap();

    println!("Confirmed!");

    Ok(())
}
