pub mod constants;

pub mod types;
use api::OpenSeaApiConfig;
use ethers::{
    contract::builders::ContractCall,
    prelude::{Address, U256},
    providers::Middleware,
    utils::parse_units,
};
pub use types::BuyArgs;
use types::Order;

pub mod api;
pub use api::{OpenSeaApi, OpenSeaApiError, OrderRequest};

mod contracts;
pub use contracts::OpenSea;

use std::sync::Arc;
use thiserror::Error;
use types::MinimalOrder;

pub async fn get_n_cheapest_orders(
    api: &OpenSeaApi,
    contract_address: Address,
    token_id: U256,
    num: usize,
) -> Result<Vec<Order>, ClientError> {
    // get the order
    let req = OrderRequest {
        side: 1,
        token_id: token_id.to_string(),
        contract_address,
        // use max limit
        limit: 50,
    };

    // get the cheapest orders above 1e16 Wei. Used to filter out "noise"
    // bids which exist in OpenSea, presumably a bug?
    let mut orders = api
        .get_orders(req)
        .await?
        .into_iter()
        .filter(|order| order.base_price > parse_units("1", 16).unwrap())
        .collect::<Vec<_>>();
    orders.sort_by(|o1, o2| o1.current_price.cmp(&o2.current_price));

    // get at most `orders.len()` items
    let len = std::cmp::min(num, orders.len());
    Ok(orders.into_iter().take(len).collect())
}

#[derive(Clone)]
pub struct Client<M> {
    pub api: OpenSeaApi,
    pub contracts: OpenSea<M>,
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error(transparent)]
    OpenSeaApiError(#[from] OpenSeaApiError),
}

impl<M: Middleware> Client<M> {
    pub fn new(provider: Arc<M>, cfg: OpenSeaApiConfig) -> Self {
        Self {
            api: OpenSeaApi::new(cfg),
            contracts: OpenSea::new(*constants::OPENSEA_ADDRESS, provider),
        }
    }

    pub async fn buy(
        &self,
        args: BuyArgs,
        n: usize,
    ) -> Result<Vec<ContractCall<M, ()>>, ClientError> {
        println!(
            "Querying Opensea API for {} orders of token {}",
            n, args.token_id
        );
        let sells = get_n_cheapest_orders(&self.api, args.token, args.token_id, n).await?;

        let mut calls = Vec::new();
        for sell in sells {
            println!(
                "[Token Id = {:?}] Maker: {:?}. Price: {:?}",
                args.token_id, sell.maker.address, sell.current_price,
            );

            // make its corresponding buy
            let buy = sell.match_sell(args.clone());
            let sell = MinimalOrder::from(sell);
            let call = self.atomic_match(buy, sell).await?;
            calls.push(call);
        }

        Ok(calls)
    }

    pub async fn buy_one(&self, args: BuyArgs) -> Result<ContractCall<M, ()>, ClientError> {
        let sell = get_n_cheapest_orders(&self.api, args.token, args.token_id, 1).await?[0].clone();
        // make its corresponding buy
        let buy = sell.match_sell(args.clone());
        let sell = MinimalOrder::from(sell);
        self.atomic_match(buy, sell).await
    }

    pub async fn atomic_match(
        &self,
        buy: MinimalOrder,
        sell: MinimalOrder,
    ) -> Result<ContractCall<M, ()>, ClientError> {
        // make the arguments in the format the contracts expect them
        let addrs = [
            buy.exchange,
            buy.maker,
            buy.taker,
            buy.fee_recipient,
            buy.target,
            buy.static_target,
            buy.payment_token,
            sell.exchange,
            sell.maker,
            sell.taker,
            sell.fee_recipient,
            sell.target,
            sell.static_target,
            sell.payment_token,
        ];
        let uints = [
            buy.maker_relayer_fee,
            buy.taker_relayer_fee,
            buy.maker_protocol_fee,
            buy.taker_protocol_fee,
            buy.base_price,
            buy.extra,
            buy.listing_time,
            buy.expiration_time,
            buy.salt,
            sell.maker_relayer_fee,
            sell.taker_relayer_fee,
            sell.maker_protocol_fee,
            sell.taker_protocol_fee,
            sell.base_price,
            sell.extra,
            sell.listing_time,
            sell.expiration_time,
            sell.salt,
        ];

        // passing it u8 returns an InvalidData error due to ethabi interpreting
        // them wrongly, so we need to convert them to u256
        // to work :shrug:
        let methods = [
            ethers::types::U256::from(buy.fee_method),
            buy.side.into(),
            buy.sale_kind.into(),
            buy.how_to_call.into(),
            sell.fee_method.into(),
            sell.side.into(),
            sell.sale_kind.into(),
            sell.how_to_call.into(),
        ];
        let vs: [U256; 2] = [0.into(), sell.v.into()];

        // TODO: This should be [H256; 5] in Abigen
        let rss_metadata = [[0; 32], [0; 32], sell.r.0, sell.s.0, [0; 32]];

        // get the call
        let call = self
            .contracts
            // Abigen error, doesn't generate a correct signature for function with underscore
            // in its name
            .method(
                "atomicMatch_",
                (
                    addrs,
                    uints,
                    methods,
                    buy.calldata.to_vec(),
                    sell.calldata.to_vec(),
                    buy.replacement_pattern.to_vec(),
                    sell.replacement_pattern.to_vec(),
                    buy.static_extradata.to_vec(),
                    sell.static_extradata.to_vec(),
                    vs,
                    rss_metadata,
                ),
            )
            .unwrap();

        // set the value
        let call = call.value(buy.current_price);

        // set the gas
        // let gas = call.estimate_gas().await.expect("could not estimate gas");
        // TODO: Why does gas estimation not work?
        let call = call.gas(300_000);

        Ok(call)
    }
}

#[cfg(test)]
mod tests {
    use std::{convert::TryFrom, sync::Arc};

    use ethers::{prelude::BlockNumber, providers::Provider, types::Address, utils::parse_units};

    use super::*;
    use crate::api::OpenSeaApiConfig;

    ethers::contract::abigen!(
        NFT,
        r#"[
        function ownerOf(uint256) view returns (address)
        function balanceOf(address,uint256) view returns (uint256)
    ]"#
    );

    #[tokio::test]
    #[ignore]
    async fn can_buy_an_nft() {
        let provider = Provider::try_from("http://localhost:8545").unwrap();
        let provider = Arc::new(provider);

        let accounts = provider.get_accounts().await.unwrap();
        let taker = accounts[0].clone();
        let id = 1126.into();

        let address = "0x91f7bb6900d65d004a659f34205beafc3b4e136c"
            .parse::<Address>()
            .unwrap();
        let nft = NFT::new(address, provider.clone());

        let block = provider
            .get_block(BlockNumber::Latest)
            .await
            .unwrap()
            .unwrap();
        let timestamp = block.timestamp.as_u64();

        // set up the args
        let args = BuyArgs {
            token_id: id,
            taker,
            token: address,
            recipient: taker,
            timestamp: Some(timestamp - 100),
        };

        // instantiate the client
        let client = Client::new(provider.clone(), OpenSeaApiConfig::default());

        // execute the call
        let call = client.buy(args, 1).await.unwrap()[0].clone();
        let call = call.gas_price(parse_units(100, 9).unwrap());
        let sent = call.send().await.unwrap();

        // wait for it to be confirmed
        let _receipt = sent.await.unwrap();
        // check the owner matches
        let owner = nft.owner_of(id).call().await.unwrap();
        assert_eq!(owner, taker);
    }

    #[tokio::test]
    #[ignore]
    async fn can_buy_an_erc1155() {
        let provider = Provider::try_from("http://localhost:8545").unwrap();
        let provider = Arc::new(provider);

        let accounts = provider.get_accounts().await.unwrap();

        let taker = accounts[0].clone();

        let address = "0x47e22659d9ae152975e6cbfa2eed5dc8b75ac545"
            .parse::<Address>()
            .unwrap();
        let nft = NFT::new(address, provider.clone());
        let token_id = 1.into();

        let block = provider
            .get_block(BlockNumber::Latest)
            .await
            .unwrap()
            .unwrap();
        let timestamp = block.timestamp.as_u64();

        // set up the args
        let args = BuyArgs {
            token_id,
            taker,
            token: address,
            recipient: taker,
            timestamp: Some(timestamp - 100),
        };

        // instantiate the client
        let client = Client::new(provider.clone(), OpenSeaApiConfig::default());

        // execute the call
        let call = client.buy(args, 1).await.unwrap()[0].clone();
        let call = call.gas_price(parse_units(100, 9).unwrap());
        let sent = call.send().await.unwrap();

        // wait for it to be confirmed
        let receipt = sent.await.unwrap();
        dbg!(receipt);
        // check the owner matches
        let num = nft.balance_of(taker, token_id).call().await.unwrap();
        assert_eq!(num, 1.into());
    }
}
