use ethers::types::Address;
use reqwest::{
    header::{self, HeaderMap},
    Client, ClientBuilder,
};
use serde::{Deserialize, Serialize};

use crate::types::{Network, Order};
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct OpenSeaApi {
    client: Client,
    network: Network,
}

impl OpenSeaApi {
    pub fn new(cfg: OpenSeaApiConfig) -> Self {
        let mut builder = ClientBuilder::new();
        if let Some(api_key) = cfg.api_key {
            let mut headers = HeaderMap::new();
            headers.insert(
                "X-API-KEY",
                header::HeaderValue::from_str(&api_key).unwrap(),
            );
            builder = builder.default_headers(headers)
        }
        let client = builder.build().unwrap();

        Self {
            client,
            network: cfg.network,
        }
    }

    pub async fn get_orders(&self, req: OrderRequest) -> Result<Vec<Order>, OpenSeaApiError> {
        let orderbook = self.network.orderbook();
        let url = format!("{}/orders", orderbook);

        // convert the request to a url encoded order
        let mut map = std::collections::HashMap::new();
        map.insert("side", serde_json::to_value(req.side)?);
        map.insert("token_id", serde_json::to_value(req.token_id)?);
        map.insert(
            "asset_contract_address",
            serde_json::to_value(req.contract_address)?,
        );
        map.insert("limit", serde_json::to_value(req.limit)?);

        let res = self.client.get(url).query(&map).send().await?;
        let text = res.text().await?;
        let resp: OrderResponse = serde_json::from_str(&text)?;

        Ok(resp.orders)
    }

    pub async fn get_order(&self, mut req: OrderRequest) -> Result<Order, OpenSeaApiError> {
        req.limit = 1;
        let res = self.get_orders(req.clone()).await?;
        let order = res
            .into_iter()
            .next()
            .ok_or(OpenSeaApiError::OrderNotFound {
                contract: req.contract_address,
                id: req.token_id,
            })?;
        Ok(order)
    }
}

//   return api.getOrder({ side: OrderSide.Sell, token_id: tokenId.toNumber(), asset_contract_address: address })
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderRequest {
    pub side: u64, // 0 for buy order
    pub token_id: String,
    pub contract_address: Address,
    pub limit: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OrderResponse {
    count: u64,
    orders: Vec<Order>,
}

#[derive(Clone, Debug)]
pub struct OpenSeaApiConfig {
    pub api_key: Option<String>,
    pub network: Network,
}

impl Default for OpenSeaApiConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            network: Network::Mainnet,
        }
    }
}

#[derive(Debug, Error)]
pub enum OpenSeaApiError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    #[error("Order not found (token: {contract}, id: {id}")]
    OrderNotFound { contract: Address, id: String },
}

#[cfg(test)]
mod tests {
    use crate::types::MinimalOrder;

    use super::*;

    #[tokio::test]
    async fn can_get_order() {
        let api = OpenSeaApi::new(OpenSeaApiConfig::default());

        let req = OrderRequest {
            side: 1,
            token_id: 2292.to_string(),
            contract_address: "0x7d256d82b32d8003d1ca1a1526ed211e6e0da9e2"
                .parse()
                .unwrap(),
            limit: 99,
        };
        let addr = req.contract_address;
        let order = api.get_order(req).await.unwrap();
        let order = MinimalOrder::from(order);
        assert_eq!(order.target, addr);
        assert_eq!(order.maker_relayer_fee, 600.into());
        assert_eq!(order.taker_relayer_fee, 0.into());
        assert_eq!(order.maker_protocol_fee, 0.into());
        assert_eq!(order.taker_protocol_fee, 0.into());
    }
}
