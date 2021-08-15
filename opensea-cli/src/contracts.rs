use ethers::{prelude::*, types::transaction::eip2718::TypedTransaction};
use ethers_flashbots::{BundleRequest, FlashbotsMiddleware};
use opensea::{api::OpenSeaApiConfig, BuyArgs, Client};
use std::sync::Arc;

use crate::opts::{BuyOpts, DeployOpts, NftOpts};

ethers::contract::abigen!(
    NFT,
    r#"[
        function ownerOf(uint256) view returns (address)
        function balanceOf(address,uint256) view returns (uint256)
    ]"#
);

impl<M: Middleware + 'static> NFT<M> {
    /// Helper function for logging information about the owner(s) of the nfts
    pub async fn log(
        &self,
        ids: &[U256],
        recipient: Address,
        erc1155: bool,
    ) -> color_eyre::Result<()> {
        for id in ids {
            if erc1155 {
                let balance = self.balance_of(recipient, *id).call().await?;
                println!(
                    "{:?} owns {:?} ERC1155 NFTs with token id {:?}",
                    recipient, balance, id
                );
            } else {
                let owner = self.owner_of(*id).call().await?;
                println!("Owner of ERC721 NFTs with token id {:?}: {:?}", id, owner);
            }
        }

        Ok(())
    }
}

ethers::contract::abigen!(
    Briber,
    r#"[
        function verifyOwnershipAndPay721(address _nftContract, address _owner, uint256[] calldata _nftIds) external payable
        function verifyOwnershipAndPay1155(address _nftContract, address _owner, uint256[] calldata _nftIds, uint256[] calldata _expectedBalances ) external payable
    ]"#
);

/// Deploys the `briber.sol` contract.
pub async fn deploy(opts: DeployOpts) -> color_eyre::Result<Address> {
    // instantiate the provider with the signer
    let provider = {
        let provider = opts.eth.provider()?;
        let chain_id = provider.get_chainid().await?.as_u64();
        let signer = opts.eth.signer()?;
        let signer = signer.with_chain_id(chain_id);
        let provider = SignerMiddleware::new(provider.clone(), signer);
        Arc::new(provider)
    };

    // compile the briber contract each time
    let solc = ethers::utils::Solc::new("briber.sol").build()?;
    let contract = solc.get("NFTRevert").expect("could not find contract");
    let briber = ContractFactory::new(contract.abi.clone(), contract.bytecode.clone(), provider);

    // deploy it
    let call = briber.deploy(())?;
    let contract = call.send().await?;
    println!("Bribe contract deployed: {:?}", contract.address());

    Ok(contract.address())
}

use opensea::{get_n_cheapest_orders, OpenSeaApi};
/// Queries the Opensea API the prices about an NFT and prints all prices as csv
pub async fn prices(opts: NftOpts) -> color_eyre::Result<()> {
    let api = OpenSeaApi::new(OpenSeaApiConfig::default());
    println!("token_id,price");
    let (ids, quantities) = opts.tokens()?;
    for (id, _) in ids.iter().zip(&quantities) {
        let orders = get_n_cheapest_orders(&api, opts.address, *id, 10).await?;
        for order in orders {
            println!("{:?},{:?}", *id, order.current_price);
        }
    }
    Ok(())
}

/// Builds a list of unsigned transactions for purchasing the specified token ids
/// at the specified quantities
async fn create_transactions<M: Middleware + 'static>(
    opensea: &Client<M>,
    ids: &[U256],
    quantities: &[usize],
    max_base_fee: U256,
    taker: Address,
    args: &BuyArgs,
) -> color_eyre::Result<(Vec<Eip1559TransactionRequest>, U256)> {
    let mut nonce = opensea
        .contracts
        .client()
        .get_transaction_count(taker, Some(BlockNumber::Pending.into()))
        .await?;
    let mut txs = Vec::new();
    for (id, quantity) in ids.iter().zip(quantities) {
        let mut args = args.clone();
        args.token_id = id.into();
        let buy_calls = opensea.buy(args, *quantity).await?;

        for call in buy_calls {
            // get the 1559 inner tx to configure the basefee
            let mut tx = match call.tx {
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
    }
    Ok((txs, nonce))
}

// Create the signed txs bundle
async fn sign_bundle<M: Middleware + 'static, S: Signer + 'static>(
    provider: Arc<SignerMiddleware<M, S>>,
    txs: &[Eip1559TransactionRequest],
    ids: &[U256],
) -> color_eyre::Result<BundleRequest> {
    let mut bundle = ethers_flashbots::BundleRequest::new();
    let mut sum = U256::from(0);
    for (i, tx) in txs.iter().enumerate() {
        if let Some(id) = ids.get(i) {
            println!(
                "[TokenId = {:?}] Signing bundle tx with {:?} Wei (max-priority-fee: {:?}, max-total-fee: {:?}, gas-limit: {:?})",
                id,
                tx.value.unwrap_or_default(),
                tx.max_priority_fee_per_gas.unwrap_or_default(),
                tx.max_fee_per_gas.unwrap_or_default(),
                tx.gas.unwrap_or_default(),
            );
        } else {
            println!(
                "Signing bribe tx with {:?} Wei (max-priority-fee: {:?}, max-total-fee: {:?}, gas-limit: {:?})",
                tx.value.unwrap_or_default(),
                tx.max_priority_fee_per_gas.unwrap_or_default(),
                tx.max_fee_per_gas.unwrap_or_default(),
                tx.gas.unwrap_or_default(),
            );
        }

        sum += tx.value.unwrap_or_default();

        let tx = tx.clone().into();
        let signature = provider.signer().sign_transaction(&tx).await?;
        let chain_id = provider.signer().chain_id();
        let rlp = tx.rlp_signed(chain_id, &signature);
        bundle = bundle.push_transaction(rlp);
    }
    println!("Total Wei required: {:?}", sum);
    Ok(bundle)
}

/// Purchases a set of tokens
pub async fn buy(opts: BuyOpts) -> color_eyre::Result<()> {
    // connect to the chain
    let provider = opts.eth.provider()?;
    let chain_id = provider.get_chainid().await?.as_u64();

    // read-only connection to the nft
    let nft = NFT::new(opts.nft.address, provider.clone());

    // configure the signer's chain id
    let signer = opts.eth.signer()?.with_chain_id(chain_id);
    let taker = signer.address();

    println!("Sending txs from {:?}", taker);
    println!("Balance: {:?}", provider.get_balance(taker, None).await?);

    // set up the args
    let block = provider.get_block(BlockNumber::Latest).await?.unwrap();
    let timestamp = block.timestamp.as_u64();

    let args = BuyArgs {
        token_id: 0.into(),
        taker,
        token: opts.nft.address,
        recipient: taker,
        timestamp: Some(timestamp - 100),
    };

    // get the max basefee 5 blocks in the future, just in case
    let base_fee = block.base_fee_per_gas.expect("No basefee found");
    println!("Current base fee {:?}", base_fee);
    let mut max_base_fee = base_fee;
    for _ in 0..5 {
        max_base_fee *= 1125;
        max_base_fee /= 1000;
    }
    println!("Max base fee {:?}", max_base_fee);

    // read the token ids
    let (ids, quantities) = opts.nft.tokens()?;
    println!("Ids: {:?}", ids);
    println!("Quantities: {:?}", quantities);

    let opensea = Client::new(provider.clone(), OpenSeaApiConfig::default());

    // 1. construct the transactions w/ pre-calculated nonces

    let (txs, next_nonce) =
        create_transactions(&opensea, &ids, &quantities, max_base_fee, taker, &args).await?;

    println!("Querying current owners...");
    nft.log(&ids, args.recipient, opts.nft.erc1155).await?;

    if let Some(bribe) = opts.flashbots.bribe {
        println!(
            "Using Flashbots. Bribe {:?}. Bribe Receiver {:?}",
            bribe, opts.flashbots.bribe_receiver
        );

        // Add signer and Flashbots middleware. The signer middleware MUST be
        // inside the Flashbots Middleware, as shown in the docs:
        // https://github.com/onbjerg/ethers-flashbots/blob/4a4e7a52b27122aedded6cd770545aefe06683f1/examples/advanced.rs#L19-L26
        let bundle_signer = LocalWallet::new(&mut ethers::core::rand::thread_rng());
        let provider = FlashbotsMiddleware::new(
            provider,
            url::Url::parse("https://relay.flashbots.net")?,
            bundle_signer,
        );
        let provider = SignerMiddleware::new(provider, signer);
        let provider = Arc::new(provider);

        // if an address is explicitly specified to receive the bribe, add an extra
        // tx to the bundle, if not, spread the tx fee evenly across all txs' fee field
        let mut txs = txs;
        match opts.flashbots.bribe_receiver {
            Some(bribe_receiver) => {
                println!(
                    "Adding bribe tx to the bundle. Bribe Receiver {:?}, Amount: {:?}",
                    bribe_receiver, bribe
                );

                // Construct the bribe transaction
                let mut tx = Eip1559TransactionRequest::new()
                    .to(bribe_receiver)
                    // TODO: Can we remove this?
                    .gas(200_000)
                    .max_fee_per_gas(max_base_fee)
                    // use the bumped nonce
                    .nonce(next_nonce)
                    .value(bribe);

                // briber.sol has a different method call depending on erc1155 or 721s
                // being sniped
                let briber = Briber::new(bribe_receiver, provider.clone());
                if opts.nft.erc1155 {
                    tx.data = briber
                        .verify_ownership_and_pay_1155(
                            args.token,
                            args.recipient,
                            ids.clone(),
                            quantities.iter().cloned().map(Into::into).collect(),
                        )
                        .calldata();
                } else {
                    tx.data = briber
                        .verify_ownership_and_pay_721(args.token, args.recipient, ids.clone())
                        .calldata();
                };

                txs.push(tx);
            }
            None => {
                let priority_fee_per_tx = bribe / opts.nft.ids.len();
                println!(
                    "Splitting bribe across {:?} txs in the bundle. Amount per tx: {:?}",
                    opts.nft.ids.len(),
                    priority_fee_per_tx
                );

                txs.iter_mut().for_each(|tx| {
                    // bump the max base fee by the priority fee
                    if let Some(ref mut max_fee_per_gas) = tx.max_fee_per_gas {
                        *max_fee_per_gas += priority_fee_per_tx;
                    }
                    tx.max_priority_fee_per_gas = Some(priority_fee_per_tx);
                })
            }
        };

        let bundle = sign_bundle(provider.clone(), &txs, &ids).await?;

        if opts.dry_run {
            return Ok(());
        }

        // set the block bundle
        let num = provider.get_block_number().await?;
        let bundle = bundle.set_block(num + 5).set_simulation_block(num);
        println!(
            "Current block {:?}. Waiting for bundle until block {:?}",
            num,
            num + 5
        );

        // 4. Send it!
        println!("Simulating bundle");
        let simulated_bundle = provider.inner().simulate_bundle(&bundle).await?;
        println!("Simulated bundle: {:?}", simulated_bundle);
        let pending_bundle = provider.inner().send_bundle(&bundle).await?;
        let res = pending_bundle.await?;
        println!("Bundle executed: {:?}", res);
    } else {
        let provider = SignerMiddleware::new(provider, signer);
        let provider = Arc::new(provider);

        for (tx, id) in txs.into_iter().zip(&ids) {
            let tx: TransactionRequest = tx.into();

            if opts.dry_run {
                return Ok(());
            }

            println!(
                "[Token Id = {:?}] Sending tx with {:?} Wei ",
                id,
                tx.value.unwrap()
            );
            let pending_tx = provider.send_transaction(tx, None).await?;
            println!("[Token Id = {:?}] Sent tx {:?}", id, *pending_tx);
        }
    }

    println!("== Ownership after ==");
    nft.log(&ids, args.recipient, opts.nft.erc1155).await?;

    Ok(())
}
