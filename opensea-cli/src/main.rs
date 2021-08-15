use structopt::StructOpt;

mod opts;
use opts::{Opts, Subcommands};

pub mod contracts;
use contracts::{buy, deploy, prices};

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    let opts = Opts::from_args();
    match opts.sub {
        Subcommands::Buy(inner) => {
            buy(inner).await?;
        }
        Subcommands::Deploy(inner) => {
            deploy(inner).await?;
        }
        Subcommands::Prices(inner) => {
            prices(inner.nft).await?;
        }
    };

    Ok(())
}
