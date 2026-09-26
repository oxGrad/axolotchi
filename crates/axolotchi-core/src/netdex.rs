//! Flavor text for the vendors a device's OUI prefix resolves to, plus
//! tracking which ones Axo has met. Pure static data plus a lookup, in
//! keeping with `axolotchi-core` staying IO-free: the vendor name itself
//! comes from `axolotchi-store`'s OUI table, resolved by the caller before
//! it builds the `Event`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DexEntry {
    pub vendor: &'static str,
    pub flavor: &'static str,
}

const ENTRIES: &[DexEntry] = &[
    DexEntry {
        vendor: "Raspberry Pi Foundation",
        flavor: "A tiny, tireless workhorse. Often found quietly running everything.",
    },
    DexEntry {
        vendor: "Raspberry Pi Trading Ltd",
        flavor: "A tiny, tireless workhorse. Often found quietly running everything.",
    },
    DexEntry {
        vendor: "Espressif Inc.",
        flavor: "A curious little sensor, usually up to something over WiFi.",
    },
    DexEntry {
        vendor: "Google Inc.",
        flavor: "Sleek, quiet, and always listening for its name.",
    },
    DexEntry {
        vendor: "Nest Labs Inc.",
        flavor: "Keeps the temperature just right, and keeps notes on you.",
    },
];

const UNKNOWN: DexEntry = DexEntry {
    vendor: "Unknown Wanderer",
    flavor: "A mysterious guest whose maker Axo hasn't catalogued yet.",
};

/// Looks up flavor text for a resolved vendor name, falling back to a
/// generic entry for anything not in the dex yet (or no vendor at all).
pub fn lookup(vendor: Option<&str>) -> DexEntry {
    vendor
        .and_then(|v| ENTRIES.iter().find(|entry| entry.vendor == v))
        .copied()
        .unwrap_or(UNKNOWN)
}

/// Every entry in the dex, for rendering the full catalog (e.g. the Netdex
/// modal) alongside `State::discovered_vendors` to know which are still
/// silhouettes.
pub fn entries() -> &'static [DexEntry] {
    ENTRIES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vendor_returns_its_flavor_text() {
        let entry = lookup(Some("Espressif Inc."));
        assert_eq!(entry.vendor, "Espressif Inc.");
        assert!(!entry.flavor.is_empty());
    }

    #[test]
    fn unknown_vendor_falls_back_to_unknown_wanderer() {
        assert_eq!(lookup(Some("Acme Gadgets")), UNKNOWN);
    }

    #[test]
    fn no_vendor_falls_back_to_unknown_wanderer() {
        assert_eq!(lookup(None), UNKNOWN);
    }

    #[test]
    fn every_entry_has_non_empty_flavor_text() {
        for entry in ENTRIES {
            assert!(
                !entry.flavor.is_empty(),
                "{} has no flavor text",
                entry.vendor
            );
        }
    }
}
