const DOR_ADDR_PREFIX: &'static str = "https://webgis.dor.wa.gov/webapi/AddressRates.aspx?output=text";
const DOR_RESPONSE_TEXT_REGEX: &'static str = r"LocationCode=(?P<loc>\d*)\sRate=(?P<rate>\.\d+)\sResultCode=(?P<code>\d+)";

use reqwest::Error as ReqwestError;
use std::convert::TryFrom;

#[derive(Debug, PartialEq)]
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

#[derive(Debug)]
pub enum TaxInfoError {
    Http(ReqwestError),
    Dor(Code),
    Internal(&'static str),
    NoMoreRetries,
}

impl TaxInfoError {
    pub fn retryable(&self) -> bool {
        match self {
            TaxInfoError::NoMoreRetries => false,
            TaxInfoError::Dor(code) => code.retryable(),
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

#[derive(Debug)]
pub struct TaxInfo {
    location_code: u32,
    rate: f32,
    code: Code
}

const MAX_ATTEMPTS: usize = 3;
const DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1_500);

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
    let raw_string = reqwest::get(&format!("{}&addr={}&city={}&zip={}",
            DOR_ADDR_PREFIX,
            addr,
            city,
            zip)).await?.text().await?;

    println!("raw string {}", raw_string);

    let regex = regex::Regex::new(DOR_RESPONSE_TEXT_REGEX).unwrap();
    if let Some(c) = regex.captures(&raw_string) {
        let loc = c.name("loc").ok_or_else(|| TaxInfoError::Internal("Expected location field not found in DOR response"))?.as_str();
        let rate = c.name("rate").ok_or_else(|| TaxInfoError::Internal("Expected rate field not found in DOR response"))?.as_str();
        let code = c.name("code").ok_or_else(|| TaxInfoError::Internal("Expected code field not found in DOR response"))?.as_str();

        println!("loc {} rate {} code {}", loc, rate, code);
        

        let code: u8 = code.parse().map_err(|_| TaxInfoError::Internal("Could not parse number from result code"))?;
        let code: Code = Code::try_from(code).map_err(|_| TaxInfoError::Internal("Could not parse a valid Code from provided code"))?;
        
        if code.is_error() {
            return Err(TaxInfoError::Dor(code));
        }

        let location_code: u32 = loc.parse().map_err(|_| TaxInfoError::Internal("Could not parse number from location code"))?;
        let rate: f32 = rate.parse().map_err(|_| TaxInfoError::Internal("Could not parse number from rate"))?;

        Ok(TaxInfo {
            code: code,
            rate: rate,
            location_code: location_code,
        })
    } else {
        Err(TaxInfoError::Internal("Response did not match expected regex"))
    }
}