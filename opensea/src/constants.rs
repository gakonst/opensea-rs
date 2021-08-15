pub const ORDERBOOK_VERSION: u64 = 1;
pub const API_VERSION: u64 = 1;
pub const ORDERBOOK_PATH: &str = "/wyvern/v${ORDERBOOK_VERSION}";
pub const API_PATH: &str = "/api/v${ORDERBOOK_VERSION}";

pub const API_BASE_MAINNET: &str = "https://api.opensea.io";
pub const API_BASE_RINKEBY: &str = "https://rinkeby-api.opensea.io";
pub const SITE_HOST_MAINNET: &str = "https://opensea.io";
pub const SITE_HOST_RINKEBY: &str = "https://rinkeby.opensea.io";

use ethers_core::types::Address;
use once_cell::sync::Lazy;

pub static OPENSEA_FEE_RECIPIENT: Lazy<Address> = Lazy::new(|| "".parse().unwrap());
