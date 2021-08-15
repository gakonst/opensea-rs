use crate::constants;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use ethers_core::types::{Address, Bytes, H256, U256};

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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Asset {}

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

    pub v: u64,
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
            extra: order.extra,
            listing_time: order.listing_time.into(),
            expiration_time: order.expiration_time.into(),
            salt: order.salt,
            fee_method: order.fee_method,
            how_to_call: order.how_to_call,
            calldata: order.calldata,
            replacement_pattern: order.replacement_pattern,
            static_extradata: order.static_extradata,
            v: order.v,
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
    id: u64,
    asset: Asset,
    listing_time: u64,
    expiration_time: u64,
    order_hash: H256,
    v: u64,
    r: H256,
    s: H256,
    #[serde(deserialize_with = "u256_from_dec_str")]
    base_price: U256,
    side: u8,
    sale_kind: u8,
    target: Address,
    how_to_call: u8,
    approved_on_chain: bool,
    cancelled: bool,
    finalized: bool,
    marked_invalid: bool,
    fee_recipient: User,
    maker: User,

    #[serde(deserialize_with = "u256_from_dec_str")]
    salt: U256,

    payment_token: Address,
    #[serde(deserialize_with = "u256_from_dec_str")]
    extra: U256,

    #[serde(deserialize_with = "u256_from_dec_str")]
    maker_protocol_fee: U256,
    #[serde(deserialize_with = "u256_from_dec_str")]
    maker_relayer_fee: U256,
    #[serde(deserialize_with = "u256_from_dec_str")]
    maker_referrer_fee: U256,

    #[serde(deserialize_with = "u256_from_dec_str")]
    taker_protocol_fee: U256,
    #[serde(deserialize_with = "u256_from_dec_str")]
    taker_relayer_fee: U256,

    calldata: Bytes,
    replacement_pattern: Bytes,

    static_target: Address,
    static_extradata: Bytes,

    exchange: Address,
    taker: User,

    #[serde(deserialize_with = "u256_from_dec_str")]
    quantity: U256,

    metadata: Metadata,

    fee_method: u8,
}

use once_cell::sync::Lazy;

pub static OPENSEA_FEE_RECIPIENT: Lazy<Address> = Lazy::new(|| "".parse().unwrap());

pub struct BuyArgs {
    pub taker: Address,
    pub recipient: Address,
    pub token: Address,
    pub token_id: U256,
    // for 1155s
    pub token_number: Option<U256>,
    #[cfg(test)]
    pub timestamp: Option<u64>,
}

impl Order {
    pub fn match_sell(&self, args: BuyArgs) -> MinimalOrder {
        let schema = &self.metadata.schema;
        use ethers_contract::BaseContract;
        // TODO: Add 1155 support
        let abi = ethers_core::abi::parse_abi(&[
            "function transferFrom(address from, address to, uint256 tokenId) public returns (bool)",
        ]).unwrap();
        let abi = BaseContract::from(abi);

        let calldata = if schema == "ERC721" {
            let sig = ethers_core::utils::id("transferFrom(address,address,uint256)");
            let data = (Address::zero(), args.recipient, args.token_id);
            abi.encode_with_selector(sig, data).unwrap()
        } else if schema == "ERC1155" {
            unimplemented!()
        } else {
            panic!("Unsupported schema")
        };

        let fee_recipient = if self.fee_recipient.address == Address::zero() {
            *OPENSEA_FEE_RECIPIENT
        } else {
            Address::zero()
        };

        #[cfg(test)]
        let listing_time = args
            .timestamp
            .unwrap_or_else(|| chrono::offset::Local::now().timestamp() as u64);
        #[cfg(not(test))]
        let listing_time = chrono::offset::Local::now().timestamp() as u64;
        // a bit in the past
        let listing_time = listing_time - 100;

        let mut order = MinimalOrder::from(self.clone());
        order.maker = args.taker;
        order.target = args.token;
        order.calldata = calldata;
        order.fee_recipient = fee_recipient;
        order.taker = self.maker.address;
        order.listing_time = listing_time.into();

        order.expiration_time = 0.into();
        order.extra = 0.into();

        order
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Metadata {
    asset: AssetId,
    schema: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AssetId {
    #[serde(deserialize_with = "u256_from_dec_str")]
    id: U256,
    address: Address,
}

use serde::de;
fn u256_from_dec_str<'de, D>(deserializer: D) -> Result<U256, D::Error>
where
    D: de::Deserializer<'de>,
{
    let s: &str = de::Deserialize::deserialize(deserializer)?;
    U256::from_dec_str(s).map_err(de::Error::custom)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct User {
    user: Username,
    profile_img_url: String,
    address: Address,
    config: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Username {
    username: Option<String>,
}

pub enum OrderSide {
    Buy,
    Sell,
}
