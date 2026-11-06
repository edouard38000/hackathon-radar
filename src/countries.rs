//! Countries: the countries or regions a person will travel to at their own
//! cost. A Profile lists them by name, and a region's name (`Schengen`,
//! `European Union`, `North America`) stands for its member countries, or for
//! US states (`US West Coast`), so the rules only ever compare countries and states.

use crate::sources::places::{self, UNITED_STATES};

/// One place a Hackathon may be in to pass Countries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// A whole country, by its English name, such as `Belgium`.
    Country(String),
    /// One US state, by its code, such as `CA`.
    UsState(&'static str),
}

/// A region a Profile can name, and what it stands for.
struct Region {
    name: &'static str,
    /// Other names for it (already folded, see [`places::fold`]).
    aliases: &'static [&'static str],
    members: Members,
}

enum Members {
    Countries(&'static [&'static str]),
    UsStates(&'static [&'static str]),
}

/// The Schengen Area, with Bulgaria and Romania (full members since 2025).
#[rustfmt::skip]
const SCHENGEN: &[&str] = &[
    "Austria", "Belgium", "Bulgaria", "Croatia", "Czechia", "Denmark", "Estonia", "Finland",
    "France", "Germany", "Greece", "Hungary", "Iceland", "Italy", "Latvia", "Liechtenstein",
    "Lithuania", "Luxembourg", "Malta", "Netherlands", "Norway", "Poland", "Portugal", "Romania",
    "Slovakia", "Slovenia", "Spain", "Sweden", "Switzerland",
];

/// The 27 member states of the European Union.
#[rustfmt::skip]
const EUROPEAN_UNION: &[&str] = &[
    "Austria", "Belgium", "Bulgaria", "Croatia", "Cyprus", "Czechia", "Denmark", "Estonia",
    "Finland", "France", "Germany", "Greece", "Hungary", "Ireland", "Italy", "Latvia", "Lithuania",
    "Luxembourg", "Malta", "Netherlands", "Poland", "Portugal", "Romania", "Slovakia", "Slovenia",
    "Spain", "Sweden",
];

/// The European Economic Area: the EU, Iceland, Liechtenstein and Norway.
#[rustfmt::skip]
const EEA: &[&str] = &[
    "Austria", "Belgium", "Bulgaria", "Croatia", "Cyprus", "Czechia", "Denmark", "Estonia",
    "Finland", "France", "Germany", "Greece", "Hungary", "Ireland", "Italy", "Latvia", "Lithuania",
    "Luxembourg", "Malta", "Netherlands", "Poland", "Portugal", "Romania", "Slovakia", "Slovenia",
    "Spain", "Sweden", "Iceland", "Liechtenstein", "Norway",
];

/// Europe as the Europe Source pack covers it: the EEA, the United Kingdom,
/// Switzerland, the microstates and the rest of the continent up to Ukraine,
/// Turkey and the Caucasus.
#[rustfmt::skip]
pub const EUROPE: &[&str] = &[
    "Austria", "Belgium", "Bulgaria", "Croatia", "Cyprus", "Czechia", "Denmark", "Estonia",
    "Finland", "France", "Germany", "Greece", "Hungary", "Ireland", "Italy", "Latvia", "Lithuania",
    "Luxembourg", "Malta", "Netherlands", "Poland", "Portugal", "Romania", "Slovakia", "Slovenia",
    "Spain", "Sweden", "Iceland", "Liechtenstein", "Norway", "Switzerland", "United Kingdom",
    "Andorra", "Monaco", "San Marino", "Vatican City", "Albania", "Bosnia and Herzegovina",
    "Montenegro", "North Macedonia", "Kosovo", "Serbia", "Moldova", "Ukraine", "Belarus", "Turkey",
    "Georgia", "Armenia",
];

/// The countries the North America Source pack covers.
pub const NORTH_AMERICA: &[&str] = &["United States", "Canada", "Mexico"];

const BENELUX: &[&str] = &["Belgium", "Netherlands", "Luxembourg"];
const DACH: &[&str] = &["Germany", "Austria", "Switzerland"];
const NORDICS: &[&str] = &["Denmark", "Finland", "Iceland", "Norway", "Sweden"];

const US_WEST_COAST: &[&str] = &["CA", "OR", "WA"];
/// The states on the Atlantic coast, and Washington, D.C.
const US_EAST_COAST: &[&str] = &[
    "ME", "NH", "MA", "RI", "CT", "NY", "NJ", "PA", "DE", "MD", "DC", "VA", "NC", "SC", "GA", "FL",
];
/// The US Census Bureau's Northeast region.
const US_NORTHEAST: &[&str] = &["CT", "ME", "MA", "NH", "RI", "VT", "NJ", "NY", "PA"];

#[rustfmt::skip]
const REGIONS: &[Region] = &[
    Region { name: "Schengen", aliases: &["schengen area", "schengen zone"], members: Members::Countries(SCHENGEN) },
    Region { name: "European Union", aliases: &["eu"], members: Members::Countries(EUROPEAN_UNION) },
    Region { name: "EEA", aliases: &["european economic area"], members: Members::Countries(EEA) },
    Region { name: "Europe", aliases: &[], members: Members::Countries(EUROPE) },
    Region { name: "Benelux", aliases: &[], members: Members::Countries(BENELUX) },
    Region { name: "DACH", aliases: &[], members: Members::Countries(DACH) },
    Region { name: "Nordics", aliases: &["nordic countries"], members: Members::Countries(NORDICS) },
    Region { name: "North America", aliases: &[], members: Members::Countries(NORTH_AMERICA) },
    Region { name: "US West Coast", aliases: &["west coast", "us west"], members: Members::UsStates(US_WEST_COAST) },
    Region { name: "US East Coast", aliases: &["east coast", "us east"], members: Members::UsStates(US_EAST_COAST) },
    Region { name: "US Northeast", aliases: &["northeast"], members: Members::UsStates(US_NORTHEAST) },
];

/// The names of the regions a Profile's Countries can use.
pub fn region_names() -> impl Iterator<Item = &'static str> {
    REGIONS.iter().map(|r| r.name)
}

/// A Profile's Countries with every region expanded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Countries {
    places: Vec<Place>,
}

impl Countries {
    /// Expands the names a Profile lists: a region becomes its members, a
    /// country or US state (`California`, `TX`) itself, by its English name;
    /// any other name is kept as it is written, as a country.
    pub fn expand<S: AsRef<str>>(names: &[S]) -> Self {
        let mut places = Vec::new();
        for name in names {
            for place in expand_one(name.as_ref()) {
                if !places.contains(&place) {
                    places.push(place);
                }
            }
        }
        Self { places }
    }

    pub fn places(&self) -> &[Place] {
        &self.places
    }

    /// Whether a Hackathon in this country, and in this US state when it is in
    /// the United States, passes. A US Hackathon whose state is unknown only
    /// passes when the whole United States is included.
    pub fn includes(&self, country: &str, us_state: Option<&str>) -> bool {
        let country = english_name(country);
        self.places.iter().any(|place| match place {
            Place::Country(c) => same_country(c, country),
            Place::UsState(code) => country == UNITED_STATES && us_state == Some(*code),
        })
    }

    /// Whether every Hackathon in this country passes, whatever its state.
    pub fn includes_country(&self, country: &str) -> bool {
        let country = english_name(country);
        self.places
            .iter()
            .any(|place| matches!(place, Place::Country(c) if same_country(c, country)))
    }
}

/// A country's English name, or the name as written when it isn't a known country.
fn english_name(country: &str) -> &str {
    places::country_name(country).unwrap_or(country.trim())
}

fn same_country(a: &str, b: &str) -> bool {
    places::fold(a) == places::fold(b)
}

fn expand_one(name: &str) -> Vec<Place> {
    let folded = places::fold(name);
    if let Some(region) = REGIONS
        .iter()
        .find(|r| places::fold(r.name) == folded || r.aliases.contains(&folded.as_str()))
    {
        return match region.members {
            Members::Countries(members) => members
                .iter()
                .map(|c| Place::Country((*c).to_owned()))
                .collect(),
            Members::UsStates(states) => states.iter().map(|s| Place::UsState(s)).collect(),
        };
    }
    if let Some(country) = places::country_name(name) {
        return vec![Place::Country(country.to_owned())];
    }
    // A US state on its own, such as `California` or `TX`. Georgia, which
    // `country_name` already read as the country, stays the country.
    if let Some((UNITED_STATES, code)) = places::subdivision(name.trim()) {
        return vec![Place::UsState(code)];
    }
    vec![Place::Country(name.trim().to_owned())]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn countries(names: &[&str]) -> Vec<String> {
        Countries::expand(names)
            .places()
            .iter()
            .map(|p| match p {
                Place::Country(c) => c.clone(),
                Place::UsState(s) => format!("US-{s}"),
            })
            .collect()
    }

    #[test]
    fn schengen_expands_to_its_29_members() {
        let schengen = Countries::expand(&["Schengen"]);
        assert_eq!(schengen.places().len(), 29);
        for member in [
            "France",
            "Switzerland",
            "Norway",
            "Iceland",
            "Romania",
            "Bulgaria",
        ] {
            assert!(schengen.includes(member, None), "{member}");
        }
        for outside in ["Ireland", "Cyprus", "United Kingdom", "United States"] {
            assert!(!schengen.includes(outside, None), "{outside}");
        }
        // Spellings and codes from Sources still match.
        assert!(schengen.includes("Deutschland", None));
        assert!(schengen.includes("CZ", None));
        assert!(Countries::expand(&["schengen area"]).includes("Malta", None));
    }

    #[test]
    fn the_european_union_expands_to_its_27_members() {
        let eu = Countries::expand(&["European Union"]);
        assert_eq!(eu.places().len(), 27);
        assert!(eu.includes("Ireland", None));
        assert!(eu.includes("Cyprus", None));
        assert!(!eu.includes("Switzerland", None));
        assert!(!eu.includes("Norway", None));
        assert_eq!(Countries::expand(&["EU"]), eu);
    }

    #[test]
    fn the_united_states_includes_every_state() {
        let us = Countries::expand(&["United States"]);
        assert_eq!(countries(&["United States"]), ["United States"]);
        assert!(us.includes("United States", Some("CA")));
        assert!(us.includes("United States", Some("NY")));
        assert!(us.includes("USA", None));
        assert!(us.includes("US", None));
        assert!(!us.includes("Canada", None));
        assert_eq!(Countries::expand(&["USA"]), us);
    }

    #[test]
    fn a_us_region_expands_to_its_states() {
        assert_eq!(countries(&["US West Coast"]), ["US-CA", "US-OR", "US-WA"]);
        let west = Countries::expand(&["US West Coast"]);
        assert!(west.includes("United States", Some("CA")));
        assert!(west.includes("United States", Some("WA")));
        assert!(!west.includes("United States", Some("NY")));
        // A US Hackathon whose state is unknown is left out.
        assert!(!west.includes("United States", None));
        assert!(!west.includes_country("United States"));
        // Only US states: a Canadian city on the West Coast isn't in it.
        assert!(!west.includes("Canada", Some("CA")));

        let east = Countries::expand(&["US East Coast"]);
        assert!(east.includes("United States", Some("MA")));
        assert!(east.includes("United States", Some("DC")));
        assert!(!east.includes("United States", Some("CA")));
    }

    #[test]
    fn north_america_expands_to_its_countries() {
        assert_eq!(
            countries(&["North America"]),
            ["United States", "Canada", "Mexico"]
        );
    }

    #[test]
    fn regions_countries_and_states_mix_without_repeats() {
        assert_eq!(
            countries(&["Benelux", "belgium", "California", "TX", "Georgia"]),
            [
                "Belgium",
                "Netherlands",
                "Luxembourg",
                "US-CA",
                "US-TX",
                "Georgia"
            ]
        );
        let mixed = Countries::expand(&["France", "California"]);
        assert!(mixed.includes("France", None));
        assert!(mixed.includes("United States", Some("CA")));
        assert!(!mixed.includes("United States", Some("OR")));
    }

    #[test]
    fn unknown_names_are_kept_as_countries() {
        assert_eq!(countries(&[" Atlantis "]), ["Atlantis"]);
        assert!(Countries::expand(&["Atlantis"]).includes("atlantis", None));
    }

    #[test]
    fn every_region_member_is_a_known_country() {
        for region in REGIONS {
            if let Members::Countries(members) = region.members {
                for member in members {
                    assert_eq!(
                        places::country_name(member),
                        Some(*member),
                        "{} lists {member}",
                        region.name
                    );
                }
            }
        }
    }
}
