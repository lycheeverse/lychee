//! TLS version configuration.
//!
//! Provides types for specifying minimum accepted TLS versions for network requests.

use reqwest::tls;
use serde::Deserialize;
use strum::{Display, EnumIter, EnumString, VariantNames};

#[derive(
    Debug, Deserialize, Default, Clone, Display, EnumIter, EnumString, VariantNames, PartialEq, Eq,
)]
#[non_exhaustive]
pub(crate) enum TlsVersion {
    #[serde(rename = "TLSv1_0")]
    #[strum(serialize = "TLSv1_0")]
    V1_0,
    #[serde(rename = "TLSv1_1")]
    #[strum(serialize = "TLSv1_1")]
    V1_1,
    #[serde(rename = "TLSv1_2")]
    #[strum(serialize = "TLSv1_2")]
    #[default]
    V1_2,
    #[serde(rename = "TLSv1_3")]
    #[strum(serialize = "TLSv1_3")]
    V1_3,
}

impl From<TlsVersion> for tls::Version {
    fn from(ver: TlsVersion) -> Self {
        match ver {
            TlsVersion::V1_0 => tls::Version::TLS_1_0,
            TlsVersion::V1_1 => tls::Version::TLS_1_1,
            TlsVersion::V1_2 => tls::Version::TLS_1_2,
            TlsVersion::V1_3 => tls::Version::TLS_1_3,
        }
    }
}
