pub mod constants;
pub mod types;

use ethers_core::types::Address;
use reqwest::{
    header::{self, HeaderMap},
    Client, ClientBuilder,
};
use serde::{Deserialize, Serialize};

use types::{Network, OpenSeaApiConfig, OpenSeaApiError, Order};

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
}

//   return api.getOrder({ side: OrderSide.Sell, token_id: tokenId.toNumber(), asset_contract_address: address })
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderRequest {
    pub side: u64, // 0 for buy order
    pub token_id: u64,
    pub contract_address: Address,
    pub limit: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OrderResponse {
    count: u64,
    orders: Vec<Order>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn it_works() {
        let api = OpenSeaApi::new(OpenSeaApiConfig::default());

        let req = OrderRequest {
            side: 1,
            token_id: 468,
            contract_address: "0x772c9181b0596229ce5ba898772ce45188284379"
                .parse()
                .unwrap(),
            limit: 1,
        };
        let orders = api.get_orders(req).await.unwrap();
        dbg!(&orders);
    }
}
