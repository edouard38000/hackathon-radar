//! A small built-in table of cities and the airports people there fly from,
//! so `init` can suggest Home's airports without the network. A city that
//! isn't here just means the person types the codes themselves.

use crate::sources::places;

/// A city, its country (as Countries names it) and its nearest airports,
/// nearest or busiest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Nearby {
    pub city: &'static str,
    pub country: &'static str,
    pub airports: &'static [&'static str],
}

const fn near(
    city: &'static str,
    country: &'static str,
    airports: &'static [&'static str],
) -> Nearby {
    Nearby {
        city,
        country,
        airports,
    }
}

const US: &str = "United States";
const UK: &str = "United Kingdom";

#[rustfmt::skip]
const TABLE: &[Nearby] = &[
    // Benelux
    near("Brussels", "Belgium", &["BRU", "CRL"]),
    near("Antwerp", "Belgium", &["BRU", "CRL"]),
    near("Ghent", "Belgium", &["BRU", "CRL"]),
    near("Leuven", "Belgium", &["BRU", "CRL"]),
    near("Mechelen", "Belgium", &["BRU", "CRL"]),
    near("Liège", "Belgium", &["BRU", "CRL", "LGG"]),
    near("Bruges", "Belgium", &["BRU", "CRL"]),
    near("Charleroi", "Belgium", &["CRL", "BRU"]),
    near("Namur", "Belgium", &["CRL", "BRU"]),
    near("Mons", "Belgium", &["CRL", "BRU"]),
    near("Louvain-la-Neuve", "Belgium", &["BRU", "CRL"]),
    near("Amsterdam", "Netherlands", &["AMS"]),
    near("Rotterdam", "Netherlands", &["AMS", "RTM"]),
    near("The Hague", "Netherlands", &["AMS", "RTM"]),
    near("Utrecht", "Netherlands", &["AMS", "EIN"]),
    near("Eindhoven", "Netherlands", &["EIN", "AMS"]),
    near("Delft", "Netherlands", &["AMS", "RTM"]),
    near("Leiden", "Netherlands", &["AMS"]),
    near("Groningen", "Netherlands", &["AMS", "GRQ"]),
    near("Maastricht", "Netherlands", &["MST", "EIN", "BRU"]),
    near("Luxembourg", "Luxembourg", &["LUX"]),
    // France
    near("Paris", "France", &["CDG", "ORY"]),
    near("Lyon", "France", &["LYS"]),
    near("Marseille", "France", &["MRS"]),
    near("Toulouse", "France", &["TLS"]),
    near("Nice", "France", &["NCE"]),
    near("Bordeaux", "France", &["BOD"]),
    near("Lille", "France", &["LIL", "BRU", "CDG"]),
    near("Nantes", "France", &["NTE"]),
    near("Strasbourg", "France", &["SXB", "FRA"]),
    near("Grenoble", "France", &["LYS", "GVA"]),
    near("Montpellier", "France", &["MPL"]),
    near("Rennes", "France", &["RNS", "NTE"]),
    // Germany, Austria, Switzerland
    near("Berlin", "Germany", &["BER"]),
    near("Munich", "Germany", &["MUC"]),
    near("Hamburg", "Germany", &["HAM"]),
    near("Frankfurt", "Germany", &["FRA"]),
    near("Cologne", "Germany", &["CGN", "DUS"]),
    near("Düsseldorf", "Germany", &["DUS", "CGN"]),
    near("Bonn", "Germany", &["CGN", "DUS"]),
    near("Stuttgart", "Germany", &["STR"]),
    near("Karlsruhe", "Germany", &["FKB", "STR", "FRA"]),
    near("Heidelberg", "Germany", &["FRA"]),
    near("Darmstadt", "Germany", &["FRA"]),
    near("Aachen", "Germany", &["DUS", "CGN", "MST"]),
    near("Leipzig", "Germany", &["LEJ"]),
    near("Dresden", "Germany", &["DRS"]),
    near("Hanover", "Germany", &["HAJ"]),
    near("Nuremberg", "Germany", &["NUE"]),
    near("Vienna", "Austria", &["VIE"]),
    near("Graz", "Austria", &["GRZ"]),
    near("Salzburg", "Austria", &["SZG"]),
    near("Innsbruck", "Austria", &["INN"]),
    near("Zurich", "Switzerland", &["ZRH"]),
    near("Geneva", "Switzerland", &["GVA"]),
    near("Lausanne", "Switzerland", &["GVA"]),
    near("Basel", "Switzerland", &["BSL"]),
    near("Bern", "Switzerland", &["ZRH", "BRN"]),
    // Southern Europe
    near("Milan", "Italy", &["MXP", "LIN", "BGY"]),
    near("Rome", "Italy", &["FCO", "CIA"]),
    near("Turin", "Italy", &["TRN"]),
    near("Bologna", "Italy", &["BLQ"]),
    near("Florence", "Italy", &["FLR", "PSA"]),
    near("Naples", "Italy", &["NAP"]),
    near("Venice", "Italy", &["VCE", "TSF"]),
    near("Madrid", "Spain", &["MAD"]),
    near("Barcelona", "Spain", &["BCN"]),
    near("Valencia", "Spain", &["VLC"]),
    near("Seville", "Spain", &["SVQ"]),
    near("Malaga", "Spain", &["AGP"]),
    near("Bilbao", "Spain", &["BIO"]),
    near("Lisbon", "Portugal", &["LIS"]),
    near("Porto", "Portugal", &["OPO"]),
    near("Athens", "Greece", &["ATH"]),
    // United Kingdom and Ireland
    near("London", UK, &["LHR", "LGW", "STN", "LTN", "LCY"]),
    near("Cambridge", UK, &["STN", "LTN"]),
    near("Oxford", UK, &["LHR"]),
    near("Manchester", UK, &["MAN"]),
    near("Birmingham", UK, &["BHX"]),
    near("Bristol", UK, &["BRS"]),
    near("Edinburgh", UK, &["EDI"]),
    near("Glasgow", UK, &["GLA"]),
    near("Dublin", "Ireland", &["DUB"]),
    near("Cork", "Ireland", &["ORK"]),
    // Northern and Eastern Europe
    near("Copenhagen", "Denmark", &["CPH"]),
    near("Aarhus", "Denmark", &["AAR", "BLL"]),
    near("Stockholm", "Sweden", &["ARN"]),
    near("Gothenburg", "Sweden", &["GOT"]),
    near("Malmö", "Sweden", &["CPH", "MMX"]),
    near("Lund", "Sweden", &["CPH", "MMX"]),
    near("Oslo", "Norway", &["OSL"]),
    near("Helsinki", "Finland", &["HEL"]),
    near("Reykjavik", "Iceland", &["KEF"]),
    near("Tallinn", "Estonia", &["TLL"]),
    near("Riga", "Latvia", &["RIX"]),
    near("Vilnius", "Lithuania", &["VNO"]),
    near("Warsaw", "Poland", &["WAW", "WMI"]),
    near("Kraków", "Poland", &["KRK"]),
    near("Wrocław", "Poland", &["WRO"]),
    near("Gdańsk", "Poland", &["GDN"]),
    near("Prague", "Czechia", &["PRG"]),
    near("Brno", "Czechia", &["BRQ", "VIE"]),
    near("Bratislava", "Slovakia", &["BTS", "VIE"]),
    near("Budapest", "Hungary", &["BUD"]),
    near("Ljubljana", "Slovenia", &["LJU"]),
    near("Zagreb", "Croatia", &["ZAG"]),
    near("Belgrade", "Serbia", &["BEG"]),
    near("Bucharest", "Romania", &["OTP"]),
    near("Cluj-Napoca", "Romania", &["CLJ"]),
    near("Sofia", "Bulgaria", &["SOF"]),
    near("Istanbul", "Turkey", &["IST", "SAW"]),
    // North America
    near("San Francisco", US, &["SFO", "OAK", "SJC"]),
    near("Oakland", US, &["OAK", "SFO"]),
    near("Berkeley", US, &["OAK", "SFO"]),
    near("San Jose", US, &["SJC", "SFO"]),
    near("Palo Alto", US, &["SFO", "SJC"]),
    near("Mountain View", US, &["SJC", "SFO"]),
    near("Los Angeles", US, &["LAX", "BUR"]),
    near("San Diego", US, &["SAN"]),
    near("Seattle", US, &["SEA"]),
    near("Portland", US, &["PDX"]),
    near("New York", US, &["JFK", "LGA", "EWR"]),
    near("Boston", US, &["BOS"]),
    near("Washington", US, &["DCA", "IAD", "BWI"]),
    near("Philadelphia", US, &["PHL"]),
    near("Pittsburgh", US, &["PIT"]),
    near("Chicago", US, &["ORD", "MDW"]),
    near("Austin", US, &["AUS"]),
    near("Dallas", US, &["DFW", "DAL"]),
    near("Houston", US, &["IAH", "HOU"]),
    near("Atlanta", US, &["ATL"]),
    near("Miami", US, &["MIA", "FLL"]),
    near("Denver", US, &["DEN"]),
    near("Phoenix", US, &["PHX"]),
    near("Minneapolis", US, &["MSP"]),
    near("Detroit", US, &["DTW"]),
    near("Ann Arbor", US, &["DTW"]),
    near("Salt Lake City", US, &["SLC"]),
    near("Las Vegas", US, &["LAS"]),
    near("Toronto", "Canada", &["YYZ", "YTZ"]),
    near("Waterloo", "Canada", &["YKF", "YYZ"]),
    near("Montreal", "Canada", &["YUL"]),
    near("Vancouver", "Canada", &["YVR"]),
    near("Ottawa", "Canada", &["YOW"]),
    near("Calgary", "Canada", &["YYC"]),
    near("Mexico City", "Mexico", &["MEX"]),
    // Elsewhere
    near("Tel Aviv", "Israel", &["TLV"]),
    near("Dubai", "United Arab Emirates", &["DXB"]),
    near("Bangalore", "India", &["BLR"]),
    near("Singapore", "Singapore", &["SIN"]),
    near("Hong Kong", "Hong Kong", &["HKG"]),
    near("Tokyo", "Japan", &["HND", "NRT"]),
    near("Seoul", "South Korea", &["ICN", "GMP"]),
    near("Sydney", "Australia", &["SYD"]),
    near("Melbourne", "Australia", &["MEL"]),
];

/// The airports near a city, from any spelling of it the places table knows
/// (`Bruxelles`, `München`), or `None` when the city isn't in the table.
pub fn near_city(city: &str) -> Option<&'static Nearby> {
    let folded = places::fold(city);
    if folded.is_empty() {
        return None;
    }
    let canonical = places::known_city(city).map(|(name, _)| places::fold(name));
    TABLE.iter().find(|n| {
        let name = places::fold(n.city);
        name == folded || canonical.as_deref() == Some(name.as_str())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggests_airports_from_any_spelling() {
        let brussels = near_city("Bruxelles").unwrap();
        assert_eq!(brussels.city, "Brussels");
        assert_eq!(brussels.country, "Belgium");
        assert_eq!(brussels.airports, ["BRU", "CRL"]);
        assert_eq!(near_city(" munchen ").unwrap().airports, ["MUC"]);
        assert_eq!(
            near_city("new york").unwrap().airports,
            ["JFK", "LGA", "EWR"]
        );
        assert_eq!(near_city("Smallville"), None);
        assert_eq!(near_city(""), None);
    }

    #[test]
    fn every_entry_is_well_formed() {
        for entry in TABLE {
            assert!(!entry.airports.is_empty(), "{}", entry.city);
            for code in entry.airports {
                assert!(
                    code.len() == 3 && code.bytes().all(|b| b.is_ascii_uppercase()),
                    "{}: {code}",
                    entry.city
                );
            }
            // Its country is one the places table knows by that name.
            assert_eq!(
                places::country_name(entry.country),
                Some(entry.country),
                "{}",
                entry.city
            );
            // Where the places table knows the city too, they agree on the country.
            if let Some((_, country)) = places::known_city(entry.city) {
                assert_eq!(country, entry.country, "{}", entry.city);
            }
        }
    }
}
