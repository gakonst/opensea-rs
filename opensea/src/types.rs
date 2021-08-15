use crate::{constants, contracts};
use ethers::{
    core::utils::id,
    types::{Address, Bytes, H256, U256},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub enum Network {
    Mainnet,
    Rinkeby,
}

impl Network {
    pub fn url(&self) -> &str {
        match self {
            Network::Mainnet => constants::API_BASE_MAINNET,
            Network::Rinkeby => constants::API_BASE_RINKEBY,
        }
    }

    pub fn orderbook(&self) -> String {
        let url = self.url();
        format!("{}/wyvern/v{}", url, constants::ORDERBOOK_VERSION)
    }

    pub fn api(&self) -> String {
        let url = self.url();
        format!("{}/api/v{}", url, constants::ORDERBOOK_VERSION)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Asset {}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// The exact arguments required to provide to the smart contract
pub struct MinimalOrder {
    // addresses involved
    pub exchange: Address,
    pub maker: Address,
    pub taker: Address,
    pub fee_recipient: Address,
    pub target: Address,
    pub static_target: Address,
    pub payment_token: Address,

    // fees
    pub maker_relayer_fee: U256,
    pub taker_relayer_fee: U256,
    pub maker_protocol_fee: U256,
    pub taker_protocol_fee: U256,

    pub base_price: U256,
    pub current_price: U256,
    pub extra: U256,
    pub listing_time: U256,
    pub expiration_time: U256,
    pub salt: U256,

    pub fee_method: u8,
    pub side: u8,
    pub sale_kind: u8,
    pub how_to_call: u8,

    pub calldata: Bytes,

    pub replacement_pattern: Bytes,

    pub static_extradata: Bytes,

    pub v: u8,
    pub r: H256,
    pub s: H256,
}

impl From<Order> for MinimalOrder {
    fn from(order: Order) -> Self {
        Self {
            exchange: order.exchange,
            maker: order.maker.address,
            taker: order.taker.address,
            fee_recipient: order.fee_recipient.address,
            target: order.target,
            static_target: order.static_target,
            payment_token: order.payment_token,
            base_price: order.base_price,
            current_price: order.current_price,
            extra: order.extra,
            listing_time: order.listing_time.into(),
            expiration_time: order.expiration_time.into(),
            salt: order.salt,
            fee_method: order.fee_method,
            how_to_call: order.how_to_call,
            calldata: order.calldata,
            replacement_pattern: order.replacement_pattern,
            static_extradata: order.static_extradata,
            v: order.v as u8,
            r: order.r,
            s: order.s,
            maker_relayer_fee: order.maker_relayer_fee,
            taker_relayer_fee: order.taker_relayer_fee,
            maker_protocol_fee: order.maker_protocol_fee,
            taker_protocol_fee: order.taker_protocol_fee,
            sale_kind: order.sale_kind,
            side: order.side,
        }
    }
}

/// The response we get from the API
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Order {
    pub id: u64,
    pub asset: Asset,
    pub listing_time: u64,
    pub expiration_time: u64,
    pub order_hash: H256,
    pub v: u64,
    #[serde(deserialize_with = "h256_from_str")]
    pub r: H256,
    #[serde(deserialize_with = "h256_from_str")]
    pub s: H256,

    #[serde(deserialize_with = "u256_from_dec_str")]
    pub base_price: U256,

    #[serde(deserialize_with = "u256_from_dec_str")]
    pub current_price: U256,
    pub side: u8,
    pub sale_kind: u8,
    pub target: Address,
    pub how_to_call: u8,
    pub approved_on_chain: bool,
    pub cancelled: bool,
    pub finalized: bool,
    pub marked_invalid: bool,
    pub fee_recipient: User,
    pub maker: User,

    #[serde(deserialize_with = "u256_from_dec_str")]
    pub salt: U256,

    pub payment_token: Address,
    #[serde(deserialize_with = "u256_from_dec_str")]
    pub extra: U256,

    #[serde(deserialize_with = "u256_from_dec_str")]
    pub maker_protocol_fee: U256,
    #[serde(deserialize_with = "u256_from_dec_str")]
    pub maker_relayer_fee: U256,
    #[serde(deserialize_with = "u256_from_dec_str")]
    pub maker_referrer_fee: U256,

    #[serde(deserialize_with = "u256_from_dec_str")]
    pub taker_protocol_fee: U256,
    #[serde(deserialize_with = "u256_from_dec_str")]
    pub taker_relayer_fee: U256,

    pub calldata: Bytes,
    pub replacement_pattern: Bytes,

    pub static_target: Address,
    pub static_extradata: Bytes,

    pub exchange: Address,
    pub taker: User,

    #[serde(deserialize_with = "u256_from_dec_str")]
    pub quantity: U256,

    pub metadata: Metadata,

    pub fee_method: u8,
}

#[derive(Clone, Debug)]
pub struct BuyArgs {
    pub taker: Address,
    pub recipient: Address,
    pub token: Address,
    pub token_id: U256,
    pub timestamp: Option<u64>,
}

impl Order {
    pub fn match_sell(&self, args: BuyArgs) -> MinimalOrder {
        let mut order = MinimalOrder::from(self.clone());

        // buy order
        order.side = 0;
        // the order maker is our taker
        order.maker = args.taker;
        order.taker = self.maker.address;
        order.target = args.token;
        order.expiration_time = 0.into();
        order.extra = 0.into();
        order.salt = ethers::core::rand::random::<u64>().into();
        order.fee_recipient = Address::zero(); // *constants::OPENSEA_FEE_RECIPIENT;

        let schema = &self.metadata.schema;
        let calldata = if schema == "ERC721" {
            // TODO: abigen should emit this as a typesafe method over a "Typed" BaseContract
            let abi = ethers::contract::BaseContract::from(contracts::OPENSEA_ABI.clone());
            let sig = id("transferFrom(address,address,uint256)");
            let data = (Address::zero(), args.recipient, args.token_id);

            order.replacement_pattern = hex::decode("00000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").unwrap().into();
            abi.encode_with_selector(sig, data).unwrap()
        } else if schema == "ERC1155" {
            // safeTransferFrom(address,address,uint256,uint256,bytes), replacement for `from`
            order.replacement_pattern = hex::decode("00000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").unwrap().into();

            let abi = ethers::contract::BaseContract::from(contracts::OPENSEA_ABI.clone());
            let sig = id("safeTransferFrom(address,address,uint256,uint256,bytes)");
            let data = (
                Address::zero(),
                args.recipient,
                args.token_id,
                self.quantity,
                Vec::<u8>::new(),
            );
            abi.encode_with_selector(sig, data).unwrap()
        } else {
            panic!("Unsupported schema")
        };
        order.calldata = calldata;

        let listing_time = args
            .timestamp
            .unwrap_or_else(|| chrono::offset::Local::now().timestamp() as u64 - 100);
        order.listing_time = listing_time.into();

        order
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Metadata {
    asset: AssetId,
    schema: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssetId {
    #[serde(deserialize_with = "u256_from_dec_str")]
    id: U256,
    address: Address,
}

use serde::de;
pub fn u256_from_dec_str<'de, D>(deserializer: D) -> Result<U256, D::Error>
where
    D: de::Deserializer<'de>,
{
    let s: &str = de::Deserialize::deserialize(deserializer)?;
    U256::from_dec_str(s).map_err(de::Error::custom)
}

use std::str::FromStr;
pub fn h256_from_str<'de, D>(deserializer: D) -> Result<H256, D::Error>
where
    D: de::Deserializer<'de>,
{
    let s: &str = de::Deserialize::deserialize(deserializer)?;
    if s.starts_with("0x") {
        H256::from_str(s).map_err(de::Error::custom)
    } else {
        Ok(H256::zero())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub user: Option<Username>,
    pub profile_img_url: String,
    pub address: Address,
    pub config: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Username {
    username: Option<String>,
}

pub enum OrderSide {
    Buy,
    Sell,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deser_order() {
        let _order: Order = serde_json::from_str(include_str!("./../../order.json")).unwrap();
    }
}
