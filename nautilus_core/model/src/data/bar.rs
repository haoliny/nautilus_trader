// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2024 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
//  You may not use this file except in compliance with the License.
//  You may obtain a copy of the License at https://www.gnu.org/licenses/lgpl-3.0.en.html
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.
// -------------------------------------------------------------------------------------------------

//! Bar aggregate structures, data types and functionality.

use std::{
    collections::HashMap,
    fmt::{Debug, Display, Formatter},
    hash::Hash,
    str::FromStr,
};

use derive_builder::Builder;
use indexmap::IndexMap;
use nautilus_core::{nanos::UnixNanos, serialization::Serializable};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::GetTsInit;
use crate::{
    enums::{AggregationSource, BarAggregation, PriceType},
    identifiers::instrument_id::InstrumentId,
    types::{price::Price, quantity::Quantity},
};

/// Represents a bar aggregation specification including a step, aggregation
/// method/rule and price type.
#[repr(C)]
#[derive(Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize, Builder)]
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "nautilus_trader.core.nautilus_pyo3.model")
)]
#[cfg_attr(feature = "trivial_copy", derive(Copy))]
pub struct BarSpecification {
    /// The step for binning samples for bar aggregation.
    pub step: usize,
    /// The type of bar aggregation.
    pub aggregation: BarAggregation,
    /// The price type to use for aggregation.
    pub price_type: PriceType,
}

impl BarSpecification {
    /// Creates a new [`BarSpecification`] instance.
    #[must_use]
    pub fn new(step: usize, aggregation: BarAggregation, price_type: PriceType) -> Self {
        Self {
            step,
            aggregation,
            price_type,
        }
    }
}

impl Display for BarSpecification {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}-{}", self.step, self.aggregation, self.price_type)
    }
}

/// Represents a bar type including the instrument ID, bar specification and
/// aggregation source.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "nautilus_trader.core.nautilus_pyo3.model")
)]
pub struct BarType {
    /// The bar types instrument ID.
    pub instrument_id: InstrumentId,
    /// The bar types specification.
    pub spec: BarSpecification,
    /// The bar types aggregation source.
    pub aggregation_source: AggregationSource,
}

impl BarType {
    /// Creates a new [`BarType`] instance.
    #[must_use]
    pub fn new(
        instrument_id: InstrumentId,
        spec: BarSpecification,
        aggregation_source: AggregationSource,
    ) -> Self {
        Self {
            instrument_id,
            spec,
            aggregation_source,
        }
    }
}

#[derive(thiserror::Error, Debug)]
#[error("Error parsing `BarType` from '{input}', invalid token: '{token}' at position {position}")]
pub struct BarTypeParseError {
    input: String,
    token: String,
    position: usize,
}

impl FromStr for BarType {
    type Err = BarTypeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // TODO: Requires handling some trait related thing
        #[allow(clippy::needless_collect)]
        let pieces: Vec<&str> = s.rsplitn(5, '-').collect();
        let rev_pieces: Vec<&str> = pieces.into_iter().rev().collect();
        if rev_pieces.len() != 5 {
            return Err(BarTypeParseError {
                input: s.to_string(),
                token: String::new(),
                position: 0,
            });
        }

        let instrument_id =
            InstrumentId::from_str(rev_pieces[0]).map_err(|_| BarTypeParseError {
                input: s.to_string(),
                token: rev_pieces[0].to_string(),
                position: 0,
            })?;

        let step = rev_pieces[1].parse().map_err(|_| BarTypeParseError {
            input: s.to_string(),
            token: rev_pieces[1].to_string(),
            position: 1,
        })?;
        let aggregation =
            BarAggregation::from_str(rev_pieces[2]).map_err(|_| BarTypeParseError {
                input: s.to_string(),
                token: rev_pieces[2].to_string(),
                position: 2,
            })?;
        let price_type = PriceType::from_str(rev_pieces[3]).map_err(|_| BarTypeParseError {
            input: s.to_string(),
            token: rev_pieces[3].to_string(),
            position: 3,
        })?;
        let aggregation_source =
            AggregationSource::from_str(rev_pieces[4]).map_err(|_| BarTypeParseError {
                input: s.to_string(),
                token: rev_pieces[4].to_string(),
                position: 4,
            })?;

        Ok(Self {
            instrument_id,
            spec: BarSpecification {
                step,
                aggregation,
                price_type,
            },
            aggregation_source,
        })
    }
}

impl From<&str> for BarType {
    fn from(input: &str) -> Self {
        Self::from_str(input).unwrap()
    }
}

impl Display for BarType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}-{}-{}",
            self.instrument_id, self.spec, self.aggregation_source
        )
    }
}

impl Serialize for BarType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for BarType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

/// Represents an aggregated bar.
#[repr(C)]
#[derive(Clone, Copy, Hash, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "nautilus_trader.core.nautilus_pyo3.model")
)]
pub struct Bar {
    /// The bar type for this bar.
    pub bar_type: BarType,
    /// The bars open price.
    pub open: Price,
    /// The bars high price.
    pub high: Price,
    /// The bars low price.
    pub low: Price,
    /// The bars close price.
    pub close: Price,
    /// The bars volume.
    pub volume: Quantity,
    /// The UNIX timestamp (nanoseconds) when the data event occurred.
    pub ts_event: UnixNanos,
    /// The UNIX timestamp (nanoseconds) when the struct was initialized.
    pub ts_init: UnixNanos,
}

impl Bar {
    /// Creates a new [`Bar`] instance.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        bar_type: BarType,
        open: Price,
        high: Price,
        low: Price,
        close: Price,
        volume: Quantity,
        ts_event: UnixNanos,
        ts_init: UnixNanos,
    ) -> Self {
        Self {
            bar_type,
            open,
            high,
            low,
            close,
            volume,
            ts_event,
            ts_init,
        }
    }

    /// Returns the metadata for the type, for use with serialization formats.
    #[must_use]
    pub fn get_metadata(
        bar_type: &BarType,
        price_precision: u8,
        size_precision: u8,
    ) -> HashMap<String, String> {
        let mut metadata = HashMap::new();
        let instrument_id = bar_type.instrument_id;
        metadata.insert("bar_type".to_string(), bar_type.to_string());
        metadata.insert("instrument_id".to_string(), instrument_id.to_string());
        metadata.insert("price_precision".to_string(), price_precision.to_string());
        metadata.insert("size_precision".to_string(), size_precision.to_string());
        metadata
    }

    /// Returns the field map for the type, for use with Arrow schemas.
    #[must_use]
    pub fn get_fields() -> IndexMap<String, String> {
        let mut metadata = IndexMap::new();
        metadata.insert("open".to_string(), "Int64".to_string());
        metadata.insert("high".to_string(), "Int64".to_string());
        metadata.insert("low".to_string(), "Int64".to_string());
        metadata.insert("close".to_string(), "Int64".to_string());
        metadata.insert("volume".to_string(), "UInt64".to_string());
        metadata.insert("ts_event".to_string(), "UInt64".to_string());
        metadata.insert("ts_init".to_string(), "UInt64".to_string());
        metadata
    }
}

impl Display for Bar {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{},{},{},{},{},{},{}",
            self.bar_type, self.open, self.high, self.low, self.close, self.volume, self.ts_event
        )
    }
}

impl Serializable for Bar {}

impl GetTsInit for Bar {
    fn ts_init(&self) -> UnixNanos {
        self.ts_init
    }
}

////////////////////////////////////////////////////////////////////////////////
// Tests
////////////////////////////////////////////////////////////////////////////////
#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::{
        enums::BarAggregation,
        identifiers::{symbol::Symbol, venue::Venue},
    };

    #[rstest]
    fn test_bar_spec_string_reprs() {
        let bar_spec = BarSpecification {
            step: 1,
            aggregation: BarAggregation::Minute,
            price_type: PriceType::Bid,
        };
        assert_eq!(bar_spec.to_string(), "1-MINUTE-BID");
        assert_eq!(format!("{bar_spec}"), "1-MINUTE-BID");
    }

    #[rstest]
    fn test_bar_type_parse_valid() {
        let input = "BTCUSDT-PERP.BINANCE-1-MINUTE-LAST-EXTERNAL";
        let bar_type = BarType::from_str(input).unwrap();

        assert_eq!(
            bar_type.instrument_id,
            InstrumentId::from("BTCUSDT-PERP.BINANCE")
        );
        assert_eq!(
            bar_type.spec,
            BarSpecification {
                step: 1,
                aggregation: BarAggregation::Minute,
                price_type: PriceType::Last,
            }
        );
        assert_eq!(bar_type.aggregation_source, AggregationSource::External);
        assert_eq!(bar_type, BarType::from(input));
    }

    #[rstest]
    fn test_bar_type_parse_invalid_token_pos_0() {
        let input = "BTCUSDT-PERP-1-MINUTE-LAST-INTERNAL";
        let result = BarType::from_str(input);

        assert_eq!(
            result.unwrap_err().to_string(),
            format!("Error parsing `BarType` from '{input}', invalid token: 'BTCUSDT-PERP' at position 0")
        );
    }

    #[rstest]
    fn test_bar_type_parse_invalid_token_pos_1() {
        let input = "BTCUSDT-PERP.BINANCE-INVALID-MINUTE-LAST-INTERNAL";
        let result = BarType::from_str(input);

        assert_eq!(
            result.unwrap_err().to_string(),
            format!(
                "Error parsing `BarType` from '{input}', invalid token: 'INVALID' at position 1"
            )
        );
    }

    #[rstest]
    fn test_bar_type_parse_invalid_token_pos_2() {
        let input = "BTCUSDT-PERP.BINANCE-1-INVALID-LAST-INTERNAL";
        let result = BarType::from_str(input);

        assert_eq!(
            result.unwrap_err().to_string(),
            format!(
                "Error parsing `BarType` from '{input}', invalid token: 'INVALID' at position 2"
            )
        );
    }

    #[rstest]
    fn test_bar_type_parse_invalid_token_pos_3() {
        let input = "BTCUSDT-PERP.BINANCE-1-MINUTE-INVALID-INTERNAL";
        let result = BarType::from_str(input);

        assert_eq!(
            result.unwrap_err().to_string(),
            format!(
                "Error parsing `BarType` from '{input}', invalid token: 'INVALID' at position 3"
            )
        );
    }

    #[rstest]
    fn test_bar_type_parse_invalid_token_pos_4() {
        let input = "BTCUSDT-PERP.BINANCE-1-MINUTE-BID-INVALID";
        let result = BarType::from_str(input);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            format!(
                "Error parsing `BarType` from '{input}', invalid token: 'INVALID' at position 4"
            )
        );
    }

    #[rstest]
    fn test_bar_type_equality() {
        let instrument_id1 = InstrumentId {
            symbol: Symbol::new("AUD/USD").unwrap(),
            venue: Venue::new("SIM").unwrap(),
        };
        let instrument_id2 = InstrumentId {
            symbol: Symbol::new("GBP/USD").unwrap(),
            venue: Venue::new("SIM").unwrap(),
        };
        let bar_spec = BarSpecification {
            step: 1,
            aggregation: BarAggregation::Minute,
            price_type: PriceType::Bid,
        };
        let bar_type1 = BarType {
            instrument_id: instrument_id1,
            spec: bar_spec,
            aggregation_source: AggregationSource::External,
        };
        let bar_type2 = BarType {
            instrument_id: instrument_id1,
            spec: bar_spec,
            aggregation_source: AggregationSource::External,
        };
        let bar_type3 = BarType {
            instrument_id: instrument_id2,
            spec: bar_spec,
            aggregation_source: AggregationSource::External,
        };
        assert_eq!(bar_type1, bar_type1);
        assert_eq!(bar_type1, bar_type2);
        assert_ne!(bar_type1, bar_type3);
    }

    #[rstest]
    fn test_bar_type_comparison() {
        let instrument_id1 = InstrumentId {
            symbol: Symbol::new("AUD/USD").unwrap(),
            venue: Venue::new("SIM").unwrap(),
        };

        let instrument_id2 = InstrumentId {
            symbol: Symbol::new("GBP/USD").unwrap(),
            venue: Venue::new("SIM").unwrap(),
        };
        let bar_spec = BarSpecification {
            step: 1,
            aggregation: BarAggregation::Minute,
            price_type: PriceType::Bid,
        };
        let bar_type1 = BarType {
            instrument_id: instrument_id1,
            spec: bar_spec,
            aggregation_source: AggregationSource::External,
        };
        let bar_type2 = BarType {
            instrument_id: instrument_id1,
            spec: bar_spec,
            aggregation_source: AggregationSource::External,
        };
        let bar_type3 = BarType {
            instrument_id: instrument_id2,
            spec: bar_spec,
            aggregation_source: AggregationSource::External,
        };

        assert!(bar_type1 <= bar_type2);
        assert!(bar_type1 < bar_type3);
        assert!(bar_type3 > bar_type1);
        assert!(bar_type3 >= bar_type1);
    }

    #[rstest]
    fn test_bar_equality() {
        let instrument_id = InstrumentId {
            symbol: Symbol::new("AUDUSD").unwrap(),
            venue: Venue::new("SIM").unwrap(),
        };
        let bar_spec = BarSpecification {
            step: 1,
            aggregation: BarAggregation::Minute,
            price_type: PriceType::Bid,
        };
        let bar_type = BarType {
            instrument_id,
            spec: bar_spec,
            aggregation_source: AggregationSource::External,
        };
        let bar1 = Bar {
            bar_type,
            open: Price::from("1.00001"),
            high: Price::from("1.00004"),
            low: Price::from("1.00002"),
            close: Price::from("1.00003"),
            volume: Quantity::from("100000"),
            ts_event: UnixNanos::default(),
            ts_init: UnixNanos::from(1),
        };

        let bar2 = Bar {
            bar_type,
            open: Price::from("1.00000"),
            high: Price::from("1.00004"),
            low: Price::from("1.00002"),
            close: Price::from("1.00003"),
            volume: Quantity::from("100000"),
            ts_event: UnixNanos::default(),
            ts_init: UnixNanos::from(1),
        };
        assert_eq!(bar1, bar1);
        assert_ne!(bar1, bar2);
    }

    #[rstest]
    fn test_json_serialization() {
        let bar = Bar::default();
        let serialized = bar.as_json_bytes().unwrap();
        let deserialized = Bar::from_json_bytes(serialized).unwrap();
        assert_eq!(deserialized, bar);
    }

    #[rstest]
    fn test_msgpack_serialization() {
        let bar = Bar::default();
        let serialized = bar.as_msgpack_bytes().unwrap();
        let deserialized = Bar::from_msgpack_bytes(serialized).unwrap();
        assert_eq!(deserialized, bar);
    }
}
