pub const ORDERBOOK_VERSION: u64 = 1;
pub const API_VERSION: u64 = 1;
pub const ORDERBOOK_PATH: &str = "/wyvern/v${ORDERBOOK_VERSION}";
pub const API_PATH: &str = "/api/v${ORDERBOOK_VERSION}";

pub const API_BASE_MAINNET: &str = "https://api.opensea.io";
pub const API_BASE_RINKEBY: &str = "https://rinkeby-api.opensea.io";
pub const SITE_HOST_MAINNET: &str = "https://opensea.io";
pub const SITE_HOST_RINKEBY: &str = "https://rinkeby.opensea.io";

use ethers::types::Address;
use once_cell::sync::Lazy;

pub static OPENSEA_FEE_RECIPIENT: Lazy<Address> = Lazy::new(|| {
    "0x5b3256965e7c3cf26e11fcaf296dfc8807c01073"
        .parse()
        .unwrap()
});

pub static OPENSEA_ADDRESS: Lazy<Address> = Lazy::new(|| {
    "0x7be8076f4ea4a4ad08075c2508e481d6c946d12b"
        .parse()
        .unwrap()
});
