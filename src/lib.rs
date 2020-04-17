//! This library can be used to get tax rates for addresses in WA state! Meant to be super simple.
//! 
//! It gets data from DOR using its [XML URL interface defined here](https://dor.wa.gov/find-taxes-rates/retail-sales-tax/destination-based-sales-tax-and-streamlined-sales-tax/wa-sales-tax-rate-lookup-url-interface).
//! 
//! Note that this needs [`tokio`](https://crates.io/crates/tokio), as [`reqwest`](https://crates.io/crates/reqwest) needs `tokio`!

#[macro_use]
extern crate log;

use reqwest::Error as ReqwestError;
use std::convert::TryFrom;
use strong_xml::{XmlRead, XmlWrite};
use url::form_urlencoded;


const DOR_ADDR_PREFIX: &'static str = "https://webgis.dor.wa.gov/webapi/AddressRates.aspx?output=xml";

/// These codes are taken from [the DOR spec](https://dor.wa.gov/find-taxes-rates/retail-sales-tax/destination-based-sales-tax-and-streamlined-sales-tax/wa-sales-tax-rate-lookup-url-interface);
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Code {
    AddrFound,
    AddrNotFoundZipFound,
    AdrrUpdatedAndFoundValidate,
    AddrUpdatedAndZipFoundValidate,
    AddrCorrectedAndFoundValidate,
    Zip5FoundNoAddrOrZip4,
    NoAddrNoZips,
    InvalidLongLat,
    InternalError,
}

impl Code {
    /// True when the returned values are garbage, as in the tax rate could be -1 or something
    pub fn is_error(&self) -> bool {
        use Code::*;
        match self {
            NoAddrNoZips => true,
            InvalidLongLat => true,
            InternalError => true,
            _ => false,
        }
    }
    pub fn retryable(&self) -> bool {
        &Code::InternalError == self
    }
}

use std::str::FromStr;

impl FromStr for Code {
    type Err = &'static str;
    fn from_str(s: &std::primitive::str) -> Result<Self, Self::Err> {
        let num: u8 = s.parse().map_err(|_| "did not use integer as code")?;
        Self::try_from(num)
    }
}

impl TryFrom<u8> for Code {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        use Code::*;
        Ok(match value {
            0 => AddrFound,
            1 => AddrNotFoundZipFound,
            2 => AdrrUpdatedAndFoundValidate,
            3 => AddrUpdatedAndZipFoundValidate,
            4 => AddrCorrectedAndFoundValidate,
            5 => Zip5FoundNoAddrOrZip4,
            6 => NoAddrNoZips,
            7 => InvalidLongLat,
            9 => InternalError,
            _ => return Err("code provided not valid"),
        })
    }
}

/// Error retreiving tax info. DOR errors most likely mean bad input, as in a weird address
#[derive(Debug)]
pub enum TaxInfoError {
    Http(ReqwestError),
    /// DOR gave a code that means there as an error. We return the raw TaxInfo object in case
    /// you'd like to inspect it
    Dor((Code, TaxInfo)),
    Internal(&'static str),
    NoMoreRetries,
}

impl TaxInfoError {
    pub fn retryable(&self) -> bool {
        match self {
            TaxInfoError::NoMoreRetries => false,
            TaxInfoError::Dor((code,  _)) => code.retryable(),
            TaxInfoError::Http(re) => re.status().map(|s| {
                s.is_server_error()
            }).unwrap_or(true),
            TaxInfoError::Internal(_) => false,
        }
    }
}

impl From<ReqwestError> for TaxInfoError {
    fn from(re: ReqwestError) -> Self {
        Self::Http(re)
    }
}


/// The Address parsed by DOR, returned as part of TaxInfo
#[derive(XmlWrite, XmlRead, PartialEq, Debug)]
#[xml(tag = "addressline")]
pub struct Address {
    #[xml(attr = "househigh")]
    pub househigh: Option<u32>,
    #[xml(attr = "houselow")]
    pub houselow: Option<u32>,
    #[xml(attr = "evenodd")]
    pub evenodd: Option<String>,
    #[xml(attr = "street")]
    pub street: Option<String>,
    #[xml(attr = "zip")]
    pub zip: Option<u32>,
    #[xml(attr = "plus4")]
    pub plus4: Option<u32>,
    #[xml(attr = "period")]
    pub period: Option<String>,
    #[xml(attr = "rta")]
    pub rta: Option<String>,
    #[xml(attr = "ptba")]
    pub ptba: Option<String>,
    #[xml(attr = "cez")]
    pub cez: Option<String>,
}

/// Tax Rate information, returned as part of TaxInfo
#[derive(XmlWrite, XmlRead, PartialEq, Debug)]
#[xml(tag = "rate")]
pub struct TaxRate {
    #[xml(attr = "name")]
    pub name: String,
    #[xml(attr = "code")]
    pub code: String,
    #[xml(attr = "localrate")]
    pub localrate: f32,
    #[xml(attr = "staterate")]
    pub staterate: f32,
}


/// Tax Info provided by WA State DOR
/// 
/// See [the DOR website](https://dor.wa.gov/find-taxes-rates/retail-sales-tax/destination-based-sales-tax-and-streamlined-sales-tax/wa-sales-tax-rate-lookup-url-interface) for specifics.
#[derive(XmlRead, PartialEq, Debug)]
#[xml(tag = "response")]
pub struct TaxInfo {
    #[xml(attr = "loccode")]
    pub loccode: i32,
    #[xml(attr = "rate")]
    pub rate: f32,
    #[xml(attr = "code")]
    pub code: Code,
    #[xml(attr = "localrate")]
    pub localrate: f32,
    #[xml(attr = "debughint")]
    pub debughint: Option<String>,
    // Children
    #[xml(child = "addressline")]
    pub address: Option<Address>,
    #[xml(child = "rate")]
    pub taxrate: Option<TaxRate>,
}

const MAX_ATTEMPTS: usize = 3;
const DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(2_500);

/// Has retries, reasonable timeouts, defaults, fully ready to go.
pub async fn get(addr: &str, city: &str, zip: &str) -> Result<TaxInfo, TaxInfoError> {
    let mut remaining_attempts = MAX_ATTEMPTS;
    while remaining_attempts > 0 {
        remaining_attempts -= 1;
        match tokio::time::timeout(DEFAULT_TIMEOUT, get_basic(addr, city, zip)).await {
            Ok(Ok(r)) => return Ok(r),
            Ok(Err(e)) => if !e.retryable() {
                return Err(e);
            }
            Err(_) => {
                // continue
            }
        }
    }
    Err(TaxInfoError::NoMoreRetries)
}

/// No retries, just one attempt, no timeout, nothing
pub async fn get_basic(addr: &str, city: &str, zip: &str) -> Result<TaxInfo, TaxInfoError> {
    let request: String = form_urlencoded::Serializer::new(DOR_ADDR_PREFIX.to_string())
        .append_pair("addr", addr)
        .append_pair("city", city)
        .append_pair("zip", zip)
        .finish();

    debug!("URL to GET from dor {}", request);
    let raw_string = reqwest::get(&request).await?.text().await?;

    debug!("raw string from DOR {}", raw_string);

    match TaxInfo::from_str(&raw_string) {
        Ok(rti) => {
            if rti.code.is_error() {
                Err(TaxInfoError::Dor((rti.code, rti)))
            } else {
                Ok(rti)
            }
        }
        Err(_e) => Err(TaxInfoError::Internal("Error parsing response from DOR")),
    }
}