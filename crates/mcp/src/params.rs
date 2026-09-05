//! The argument types the tools accept.
//!
//! Two things are worth the extra code here. Money is taken as an [`Amount`]
//! that accepts a JSON number *or* a string, because a caller writing
//! `"0.15"` should not silently get a different value from one writing `0.15`.
//! And dates are taken as [`Deadline`], which accepts a plain `2027-01-01` as
//! well as a full timestamp, because that is what a model actually writes.

use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;
use safe_invest_core::model::{AssetKind, PlayerKind};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer};
use std::str::FromStr;

/// A monetary amount or a quantity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Amount(pub Decimal);

impl<'de> Deserialize<'de> for Amount {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;

        let value = serde_json::Value::deserialize(deserializer)?;
        let decimal = match &value {
            serde_json::Value::String(text) => Decimal::from_str(text.trim())
                .map_err(|_| D::Error::custom(format!("montant illisible : {text}")))?,
            serde_json::Value::Number(number) => Decimal::from_str(&number.to_string())
                .ok()
                .or_else(|| number.as_f64().and_then(Decimal::from_f64))
                .ok_or_else(|| D::Error::custom("montant hors des valeurs représentables"))?,
            other => return Err(D::Error::custom(format!("montant attendu, reçu {other}"))),
        };
        Ok(Self(decimal))
    }
}

impl JsonSchema for Amount {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Amount".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "Un montant ou une quantité. Accepte un nombre (1000, 0.25) ou une chaîne (\"0.25\") pour préserver les décimales exactes.",
            "anyOf": [
                { "type": "number" },
                { "type": "string", "pattern": "^-?[0-9]+(\\.[0-9]+)?$" }
            ]
        })
    }
}

/// A deadline, as a date or a full timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deadline(pub jiff::Timestamp);

impl<'de> Deserialize<'de> for Deadline {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;

        let text = String::deserialize(deserializer)?;
        let trimmed = text.trim();

        if let Ok(timestamp) = trimmed.parse::<jiff::Timestamp>() {
            return Ok(Self(timestamp));
        }

        // A bare date means the end of that day, so "by 2027-01-01" includes
        // the whole first of January rather than expiring at midnight.
        if let Ok(date) = trimmed.parse::<jiff::civil::Date>()
            && let Ok(zoned) = date
                .to_datetime(jiff::civil::time(23, 59, 59, 0))
                .in_tz("UTC")
        {
            return Ok(Self(zoned.timestamp()));
        }

        Err(D::Error::custom(format!(
            "date illisible : « {trimmed} ». Utilisez 2027-01-31 ou 2027-01-31T18:00:00Z."
        )))
    }
}

impl JsonSchema for Deadline {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Deadline".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "Date limite, au format 2027-01-31 (fin de journée UTC) ou 2027-01-31T18:00:00Z."
        })
    }
}

/// Asset kind as the tools name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Crypto,
    Stock,
    Etf,
}

impl From<Kind> for AssetKind {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::Crypto => Self::Crypto,
            Kind::Stock => Self::Stock,
            Kind::Etf => Self::Etf,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Player {
    /// A person plays; trades need no justification.
    Human,
    /// An AI plays; every trade must carry a `rationale`.
    Ai,
}

impl From<Player> for PlayerKind {
    fn from(player: Player) -> Self {
        match player {
            Player::Human => Self::Human,
            Player::Ai => Self::Ai,
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test that trips is a test that failed"
)]
mod tests {
    use super::*;

    #[test]
    fn an_amount_reads_the_same_from_a_number_and_from_a_string() {
        let from_number: Amount = serde_json::from_str("0.25").unwrap();
        let from_string: Amount = serde_json::from_str("\"0.25\"").unwrap();
        assert_eq!(from_number, from_string);
    }

    #[test]
    fn a_string_amount_keeps_decimals_a_float_would_round() {
        let precise: Amount = serde_json::from_str("\"0.12345678\"").unwrap();
        assert_eq!(precise.0, Decimal::from_str("0.12345678").unwrap());
    }

    #[test]
    fn a_bad_amount_is_refused_rather_than_read_as_zero() {
        assert!(serde_json::from_str::<Amount>("\"beaucoup\"").is_err());
        assert!(serde_json::from_str::<Amount>("null").is_err());
        assert!(serde_json::from_str::<Amount>("true").is_err());
    }

    #[test]
    fn a_bare_date_means_the_end_of_that_day() {
        let deadline: Deadline = serde_json::from_str("\"2027-01-31\"").unwrap();
        assert_eq!(
            deadline.0.strftime("%Y-%m-%dT%H:%M:%SZ").to_string(),
            "2027-01-31T23:59:59Z"
        );
    }

    #[test]
    fn a_full_timestamp_is_taken_as_written() {
        let deadline: Deadline = serde_json::from_str("\"2027-01-31T18:00:00Z\"").unwrap();
        assert_eq!(
            deadline.0.strftime("%Y-%m-%dT%H:%M:%SZ").to_string(),
            "2027-01-31T18:00:00Z"
        );
    }

    #[test]
    fn a_date_that_makes_no_sense_says_what_to_write_instead() {
        let error = serde_json::from_str::<Deadline>("\"le mois prochain\"").unwrap_err();
        assert!(error.to_string().contains("2027-01-31"));
    }
}
