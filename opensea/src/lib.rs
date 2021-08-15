pub mod constants;

pub mod types;
use api::OpenSeaApiConfig;
use ethers::{contract::builders::ContractCall, prelude::U256, providers::Middleware};
pub use types::BuyArgs;

pub mod api;
pub use api::{OpenSeaApi, OpenSeaApiError, OrderRequest};

mod contracts;
pub use contracts::OpenSea;

use std::sync::Arc;
use thiserror::Error;
use types::MinimalOrder;

pub struct Client<M> {
    api: OpenSeaApi,
    contracts: OpenSea<M>,
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
    pub async fn buy(&self, args: BuyArgs) -> Result<ContractCall<M, ()>, ClientError> {
        // get the order
        let req = OrderRequest {
            side: 1,
            token_id: args.token_id.as_u64(),
            contract_address: args.token,
            limit: 1,
        };
        let sell = self.api.get_order(req).await?;

        // make its corresponding buy
        let buy = sell.match_sell(args);
        let sell = MinimalOrder::from(sell);

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

        // set the price
        let call = call.value(buy.base_price);

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
            token_number: None,
        };
        dbg!(&args);

        // instantiate the client
        let client = Client::new(provider.clone(), OpenSeaApiConfig::default());

        // execute the call
        let call = client.buy(args).await.unwrap();
        dbg!(&hex::encode(call.calldata().unwrap()));
        let call = call.gas_price(parse_units(100, 9).unwrap());
        let sent = call.send().await.unwrap();

        // wait for it to be confirmed
        let _receipt = sent.await.unwrap();
        // check the owner matches
        let owner = nft.owner_of(id).call().await.unwrap();
        assert_eq!(owner, taker);
    }
}
