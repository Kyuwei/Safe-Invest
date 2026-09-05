//! A small built-in list of well-known assets.
//!
//! It gives the Market screen something to show before anyone types, keeps
//! search working with no network, and — the practical reason — carries the
//! provider-specific ids (CoinGecko's slug, Yahoo's suffixed ticker) that a
//! bare symbol does not.

use safe_invest_core::model::{Asset, AssetKind};

struct Entry {
    symbol: &'static str,
    name: &'static str,
    kind: AssetKind,
    provider_id: &'static str,
}

const CATALOG: &[Entry] = &[
    // Crypto — provider_id is the CoinGecko slug.
    Entry {
        symbol: "BTC",
        name: "Bitcoin",
        kind: AssetKind::Crypto,
        provider_id: "bitcoin",
    },
    Entry {
        symbol: "ETH",
        name: "Ethereum",
        kind: AssetKind::Crypto,
        provider_id: "ethereum",
    },
    Entry {
        symbol: "SOL",
        name: "Solana",
        kind: AssetKind::Crypto,
        provider_id: "solana",
    },
    Entry {
        symbol: "XRP",
        name: "XRP",
        kind: AssetKind::Crypto,
        provider_id: "ripple",
    },
    Entry {
        symbol: "ADA",
        name: "Cardano",
        kind: AssetKind::Crypto,
        provider_id: "cardano",
    },
    Entry {
        symbol: "DOGE",
        name: "Dogecoin",
        kind: AssetKind::Crypto,
        provider_id: "dogecoin",
    },
    Entry {
        symbol: "AVAX",
        name: "Avalanche",
        kind: AssetKind::Crypto,
        provider_id: "avalanche-2",
    },
    Entry {
        symbol: "DOT",
        name: "Polkadot",
        kind: AssetKind::Crypto,
        provider_id: "polkadot",
    },
    Entry {
        symbol: "LINK",
        name: "Chainlink",
        kind: AssetKind::Crypto,
        provider_id: "chainlink",
    },
    Entry {
        symbol: "LTC",
        name: "Litecoin",
        kind: AssetKind::Crypto,
        provider_id: "litecoin",
    },
    // Shares — provider_id is the Yahoo ticker.
    Entry {
        symbol: "AAPL",
        name: "Apple",
        kind: AssetKind::Stock,
        provider_id: "AAPL",
    },
    Entry {
        symbol: "MSFT",
        name: "Microsoft",
        kind: AssetKind::Stock,
        provider_id: "MSFT",
    },
    Entry {
        symbol: "GOOGL",
        name: "Alphabet",
        kind: AssetKind::Stock,
        provider_id: "GOOGL",
    },
    Entry {
        symbol: "AMZN",
        name: "Amazon",
        kind: AssetKind::Stock,
        provider_id: "AMZN",
    },
    Entry {
        symbol: "NVDA",
        name: "NVIDIA",
        kind: AssetKind::Stock,
        provider_id: "NVDA",
    },
    Entry {
        symbol: "TSLA",
        name: "Tesla",
        kind: AssetKind::Stock,
        provider_id: "TSLA",
    },
    Entry {
        symbol: "META",
        name: "Meta Platforms",
        kind: AssetKind::Stock,
        provider_id: "META",
    },
    Entry {
        symbol: "MC.PA",
        name: "LVMH",
        kind: AssetKind::Stock,
        provider_id: "MC.PA",
    },
    Entry {
        symbol: "OR.PA",
        name: "L'Oréal",
        kind: AssetKind::Stock,
        provider_id: "OR.PA",
    },
    Entry {
        symbol: "AIR.PA",
        name: "Airbus",
        kind: AssetKind::Stock,
        provider_id: "AIR.PA",
    },
    Entry {
        symbol: "TTE.PA",
        name: "TotalEnergies",
        kind: AssetKind::Stock,
        provider_id: "TTE.PA",
    },
    Entry {
        symbol: "SAN.PA",
        name: "Sanofi",
        kind: AssetKind::Stock,
        provider_id: "SAN.PA",
    },
    // Trackers — the diversified starting point a beginner should meet first.
    Entry {
        symbol: "CW8.PA",
        name: "Amundi MSCI World",
        kind: AssetKind::Etf,
        provider_id: "CW8.PA",
    },
    Entry {
        symbol: "VWCE.DE",
        name: "Vanguard FTSE All-World",
        kind: AssetKind::Etf,
        provider_id: "VWCE.DE",
    },
    Entry {
        symbol: "IWDA.AS",
        name: "iShares Core MSCI World",
        kind: AssetKind::Etf,
        provider_id: "IWDA.AS",
    },
    Entry {
        symbol: "ESE.PA",
        name: "BNP S&P 500",
        kind: AssetKind::Etf,
        provider_id: "ESE.PA",
    },
    Entry {
        symbol: "SPY",
        name: "SPDR S&P 500",
        kind: AssetKind::Etf,
        provider_id: "SPY",
    },
    Entry {
        symbol: "QQQ",
        name: "Invesco QQQ (Nasdaq 100)",
        kind: AssetKind::Etf,
        provider_id: "QQQ",
    },
];

fn to_asset(entry: &Entry) -> Asset {
    Asset {
        symbol: entry.symbol.to_owned(),
        name: entry.name.to_owned(),
        kind: entry.kind,
        provider_id: Some(entry.provider_id.to_owned()),
        logo_url: None,
    }
}

/// Everything in the catalogue, optionally narrowed to one kind.
pub fn popular(kind: Option<AssetKind>) -> Vec<Asset> {
    CATALOG
        .iter()
        .filter(|e| kind.is_none_or(|k| e.kind == k))
        .map(to_asset)
        .collect()
}

/// Case-insensitive match on symbol or name. An empty query lists everything,
/// which is what the Market screen wants on first paint.
pub fn search(query: &str, kind: Option<AssetKind>) -> Vec<Asset> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return popular(kind);
    }

    let mut found: Vec<(u8, Asset)> = CATALOG
        .iter()
        .filter(|e| kind.is_none_or(|k| e.kind == k))
        .filter_map(|e| {
            let symbol = e.symbol.to_lowercase();
            let name = e.name.to_lowercase();
            // Rank exact hits above prefixes above substrings, so typing "eth"
            // offers Ethereum before anything that merely contains "eth".
            let rank = if symbol == needle {
                0
            } else if symbol.starts_with(&needle) || name.starts_with(&needle) {
                1
            } else if symbol.contains(&needle) || name.contains(&needle) {
                2
            } else {
                return None;
            };
            Some((rank, to_asset(e)))
        })
        .collect();

    found.sort_by_key(|(rank, asset)| (*rank, asset.symbol.clone()));
    found.into_iter().map(|(_, asset)| asset).collect()
}

/// The catalogue entry for a symbol, used to recover a provider id when the
/// caller only knows the ticker.
pub fn lookup(kind: AssetKind, symbol: &str) -> Option<Asset> {
    let wanted = Asset::normalize(symbol);
    CATALOG
        .iter()
        .find(|e| e.kind == kind && e.symbol.eq_ignore_ascii_case(&wanted))
        .map(to_asset)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::unchecked_time_subtraction,
    reason = "a test that trips is a test that failed"
)]
mod tests {
    use super::*;

    #[test]
    fn an_exact_symbol_wins_over_a_partial_name() {
        let found = search("eth", None);
        assert_eq!(found.first().unwrap().symbol, "ETH");
    }

    #[test]
    fn search_can_be_narrowed_to_one_kind() {
        assert!(
            search("", Some(AssetKind::Etf))
                .iter()
                .all(|a| a.kind == AssetKind::Etf)
        );
    }

    #[test]
    fn an_empty_query_lists_the_catalogue() {
        assert_eq!(search("  ", None).len(), popular(None).len());
    }

    #[test]
    fn every_entry_carries_a_provider_id() {
        assert!(popular(None).iter().all(|a| a.provider_id.is_some()));
    }

    #[test]
    fn lookup_is_case_insensitive() {
        assert_eq!(
            lookup(AssetKind::Crypto, "btc")
                .unwrap()
                .provider_id
                .unwrap(),
            "bitcoin"
        );
    }

    #[test]
    fn nothing_matches_gibberish() {
        assert!(search("zzzznotathing", None).is_empty());
    }
}
