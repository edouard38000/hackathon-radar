//! Cities and countries, so every Source names places the same way: English
//! country names (as the Profile's Countries use them) and one spelling per city.
//!
//! The table holds the cities Hackathons are most often listed in. It also
//! places a listing that only gives coordinates (Luma hides most addresses
//! until you register) at the nearest known city.

/// A known city: its usual English name, its country, where it is, and other
/// spellings Sources use for it (already folded, see [`fold`]).
struct City {
    name: &'static str,
    country: &'static str,
    lat: f64,
    lon: f64,
    aliases: &'static [&'static str],
}

const fn city(
    name: &'static str,
    country: &'static str,
    lat: f64,
    lon: f64,
    aliases: &'static [&'static str],
) -> City {
    City {
        name,
        country,
        lat,
        lon,
        aliases,
    }
}

#[rustfmt::skip]
const CITIES: &[City] = &[
    // Belgium
    city("Brussels", "Belgium", 50.85, 4.35, &["bruxelles", "brussel"]),
    city("Antwerp", "Belgium", 51.22, 4.40, &["antwerpen", "anvers"]),
    city("Ghent", "Belgium", 51.05, 3.72, &["gent", "gand"]),
    city("Leuven", "Belgium", 50.88, 4.70, &["louvain"]),
    city("Liège", "Belgium", 50.63, 5.57, &["liege", "luik", "luttich"]),
    city("Bruges", "Belgium", 51.21, 3.22, &["brugge"]),
    city("Kortrijk", "Belgium", 50.83, 3.26, &["courtrai"]),
    city("Mons", "Belgium", 50.45, 3.95, &["bergen"]),
    city("Namur", "Belgium", 50.47, 4.87, &["namen"]),
    city("Charleroi", "Belgium", 50.41, 4.44, &[]),
    city("Hasselt", "Belgium", 50.93, 5.34, &[]),
    city("Mechelen", "Belgium", 51.03, 4.48, &["malines"]),
    city("Louvain-la-Neuve", "Belgium", 50.67, 4.61, &[]),
    // Netherlands
    city("Amsterdam", "Netherlands", 52.37, 4.90, &[]),
    city("Rotterdam", "Netherlands", 51.92, 4.48, &[]),
    city("The Hague", "Netherlands", 52.08, 4.30, &["den haag", "s gravenhage", "la haye"]),
    city("Utrecht", "Netherlands", 52.09, 5.12, &[]),
    city("Eindhoven", "Netherlands", 51.44, 5.48, &[]),
    city("Delft", "Netherlands", 52.01, 4.36, &[]),
    city("Leiden", "Netherlands", 52.16, 4.49, &[]),
    city("Groningen", "Netherlands", 53.22, 6.57, &[]),
    city("Maastricht", "Netherlands", 50.85, 5.69, &[]),
    city("Enschede", "Netherlands", 52.22, 6.89, &[]),
    city("Nijmegen", "Netherlands", 51.84, 5.86, &[]),
    // Luxembourg
    city("Luxembourg", "Luxembourg", 49.61, 6.13, &["luxembourg city", "luxemburg"]),
    // France
    city("Paris", "France", 48.86, 2.35, &[]),
    city("Lille", "France", 50.63, 3.06, &[]),
    city("Lyon", "France", 45.76, 4.84, &[]),
    city("Marseille", "France", 43.30, 5.37, &[]),
    city("Toulouse", "France", 43.60, 1.44, &[]),
    city("Bordeaux", "France", 44.84, -0.58, &[]),
    city("Nantes", "France", 47.22, -1.55, &[]),
    city("Strasbourg", "France", 48.57, 7.75, &[]),
    city("Grenoble", "France", 45.19, 5.72, &[]),
    city("Montpellier", "France", 43.61, 3.88, &[]),
    city("Nice", "France", 43.70, 7.27, &[]),
    city("Rennes", "France", 48.11, -1.68, &[]),
    city("Sophia Antipolis", "France", 43.62, 7.05, &[]),
    // Germany
    city("Berlin", "Germany", 52.52, 13.40, &[]),
    city("Munich", "Germany", 48.14, 11.58, &["munchen", "muenchen"]),
    city("Hamburg", "Germany", 53.55, 10.00, &[]),
    city("Cologne", "Germany", 50.94, 6.96, &["koln", "koeln"]),
    city("Frankfurt", "Germany", 50.11, 8.68, &["frankfurt am main"]),
    city("Stuttgart", "Germany", 48.78, 9.18, &[]),
    city("Düsseldorf", "Germany", 51.23, 6.78, &["dusseldorf", "duesseldorf"]),
    city("Karlsruhe", "Germany", 49.01, 8.40, &[]),
    city("Heidelberg", "Germany", 49.40, 8.67, &[]),
    city("Darmstadt", "Germany", 49.87, 8.65, &[]),
    city("Aachen", "Germany", 50.78, 6.08, &[]),
    city("Leipzig", "Germany", 51.34, 12.37, &[]),
    city("Dresden", "Germany", 51.05, 13.74, &[]),
    city("Hanover", "Germany", 52.37, 9.73, &["hannover"]),
    city("Nuremberg", "Germany", 49.45, 11.08, &["nurnberg", "nuernberg"]),
    city("Erlangen", "Germany", 49.60, 11.00, &[]),
    city("Bonn", "Germany", 50.73, 7.10, &[]),
    city("Potsdam", "Germany", 52.39, 13.06, &[]),
    city("Mannheim", "Germany", 49.49, 8.47, &[]),
    city("Freiburg", "Germany", 47.99, 7.84, &["freiburg im breisgau"]),
    city("Bremen", "Germany", 53.08, 8.80, &[]),
    city("Dortmund", "Germany", 51.51, 7.47, &[]),
    city("Essen", "Germany", 51.46, 7.01, &[]),
    city("Kiel", "Germany", 54.32, 10.12, &[]),
    city("Tübingen", "Germany", 48.52, 9.06, &["tubingen", "tuebingen"]),
    city("Heilbronn", "Germany", 49.14, 9.22, &[]),
    // Switzerland and Austria
    city("Zurich", "Switzerland", 47.38, 8.54, &["zuerich"]),
    city("Geneva", "Switzerland", 46.20, 6.14, &["geneve", "genf"]),
    city("Lausanne", "Switzerland", 46.52, 6.63, &[]),
    city("Basel", "Switzerland", 47.56, 7.59, &["bale"]),
    city("Bern", "Switzerland", 46.95, 7.45, &["berne"]),
    city("St. Gallen", "Switzerland", 47.42, 9.37, &["st gallen", "sankt gallen"]),
    city("Lucerne", "Switzerland", 47.05, 8.31, &["luzern"]),
    city("Vienna", "Austria", 48.21, 16.37, &["wien"]),
    city("Graz", "Austria", 47.07, 15.44, &[]),
    city("Linz", "Austria", 48.31, 14.29, &[]),
    city("Innsbruck", "Austria", 47.27, 11.40, &[]),
    city("Salzburg", "Austria", 47.81, 13.06, &[]),
    // United Kingdom and Ireland
    city("London", "United Kingdom", 51.51, -0.13, &[]),
    city("Manchester", "United Kingdom", 53.48, -2.24, &[]),
    city("Edinburgh", "United Kingdom", 55.95, -3.19, &[]),
    city("Glasgow", "United Kingdom", 55.86, -4.25, &[]),
    city("Cambridge", "United Kingdom", 52.21, 0.12, &[]),
    city("Oxford", "United Kingdom", 51.75, -1.26, &[]),
    city("Bristol", "United Kingdom", 51.45, -2.59, &[]),
    city("Birmingham", "United Kingdom", 52.49, -1.89, &[]),
    city("Leeds", "United Kingdom", 53.80, -1.55, &[]),
    city("Sheffield", "United Kingdom", 53.38, -1.47, &[]),
    city("Brighton", "United Kingdom", 50.82, -0.14, &[]),
    city("Belfast", "United Kingdom", 54.60, -5.93, &[]),
    city("Cardiff", "United Kingdom", 51.48, -3.18, &[]),
    city("Dublin", "Ireland", 53.35, -6.26, &[]),
    city("Cork", "Ireland", 51.90, -8.47, &[]),
    city("Galway", "Ireland", 53.27, -9.05, &[]),
    // Nordics and Baltics
    city("Copenhagen", "Denmark", 55.68, 12.57, &["kobenhavn"]),
    city("Aarhus", "Denmark", 56.16, 10.20, &["arhus"]),
    city("Stockholm", "Sweden", 59.33, 18.07, &[]),
    city("Gothenburg", "Sweden", 57.71, 11.97, &["goteborg"]),
    city("Malmö", "Sweden", 55.60, 13.00, &["malmo"]),
    city("Lund", "Sweden", 55.70, 13.19, &[]),
    city("Oslo", "Norway", 59.91, 10.75, &[]),
    city("Trondheim", "Norway", 63.43, 10.40, &[]),
    city("Bergen", "Norway", 60.39, 5.32, &[]),
    city("Helsinki", "Finland", 60.17, 24.94, &["helsingfors"]),
    city("Espoo", "Finland", 60.21, 24.66, &[]),
    city("Turku", "Finland", 60.45, 22.27, &["abo"]),
    city("Tampere", "Finland", 61.50, 23.79, &[]),
    city("Oulu", "Finland", 65.01, 25.47, &[]),
    city("Reykjavik", "Iceland", 64.15, -21.94, &[]),
    city("Tallinn", "Estonia", 59.44, 24.75, &[]),
    city("Tartu", "Estonia", 58.38, 26.72, &[]),
    city("Riga", "Latvia", 56.95, 24.11, &[]),
    city("Vilnius", "Lithuania", 54.69, 25.28, &[]),
    city("Kaunas", "Lithuania", 54.90, 23.89, &[]),
    // Southern Europe
    city("Madrid", "Spain", 40.42, -3.70, &[]),
    city("Barcelona", "Spain", 41.39, 2.17, &[]),
    city("Valencia", "Spain", 39.47, -0.38, &[]),
    city("Seville", "Spain", 37.39, -5.98, &["sevilla"]),
    city("Bilbao", "Spain", 43.26, -2.93, &[]),
    city("Málaga", "Spain", 36.72, -4.42, &["malaga"]),
    city("Lisbon", "Portugal", 38.72, -9.14, &["lisboa"]),
    city("Porto", "Portugal", 41.15, -8.61, &["oporto"]),
    city("Braga", "Portugal", 41.55, -8.42, &[]),
    city("Rome", "Italy", 41.90, 12.50, &["roma"]),
    city("Milan", "Italy", 45.46, 9.19, &["milano"]),
    city("Turin", "Italy", 45.07, 7.69, &["torino"]),
    city("Bologna", "Italy", 44.49, 11.34, &[]),
    city("Florence", "Italy", 43.77, 11.26, &["firenze"]),
    city("Naples", "Italy", 40.85, 14.27, &["napoli"]),
    city("Venice", "Italy", 45.44, 12.32, &["venezia"]),
    city("Trento", "Italy", 46.07, 11.12, &[]),
    city("Pisa", "Italy", 43.72, 10.40, &[]),
    city("Athens", "Greece", 37.98, 23.73, &["athina"]),
    city("Thessaloniki", "Greece", 40.64, 22.94, &[]),
    city("Valletta", "Malta", 35.90, 14.51, &[]),
    city("Birkirkara", "Malta", 35.90, 14.46, &[]),
    city("Nicosia", "Cyprus", 35.17, 33.36, &[]),
    city("Limassol", "Cyprus", 34.68, 33.04, &[]),
    // Central and Eastern Europe
    city("Warsaw", "Poland", 52.23, 21.01, &["warszawa"]),
    city("Kraków", "Poland", 50.06, 19.94, &["krakow", "cracow"]),
    city("Wrocław", "Poland", 51.11, 17.04, &["wroclaw"]),
    city("Gdańsk", "Poland", 54.35, 18.65, &["gdansk"]),
    city("Poznań", "Poland", 52.41, 16.93, &["poznan"]),
    city("Łódź", "Poland", 51.76, 19.46, &["lodz"]),
    city("Prague", "Czechia", 50.08, 14.44, &["praha", "prag"]),
    city("Brno", "Czechia", 49.20, 16.61, &[]),
    city("Bratislava", "Slovakia", 48.15, 17.11, &[]),
    city("Budapest", "Hungary", 47.50, 19.04, &[]),
    city("Ljubljana", "Slovenia", 46.06, 14.51, &[]),
    city("Zagreb", "Croatia", 45.81, 15.98, &[]),
    city("Split", "Croatia", 43.51, 16.44, &[]),
    city("Belgrade", "Serbia", 44.79, 20.45, &["beograd"]),
    city("Novi Sad", "Serbia", 45.27, 19.83, &[]),
    city("Bucharest", "Romania", 44.43, 26.10, &["bucuresti"]),
    city("Cluj-Napoca", "Romania", 46.77, 23.60, &["cluj napoca", "cluj"]),
    city("Iași", "Romania", 47.16, 27.59, &["iasi"]),
    city("Timișoara", "Romania", 45.75, 21.23, &["timisoara"]),
    city("Sofia", "Bulgaria", 42.70, 23.32, &[]),
    city("Plovdiv", "Bulgaria", 42.14, 24.75, &[]),
    city("Kyiv", "Ukraine", 50.45, 30.52, &["kiev"]),
    city("Lviv", "Ukraine", 49.84, 24.03, &["lvov"]),
    city("Chișinău", "Moldova", 47.01, 28.86, &["chisinau"]),
    city("Istanbul", "Turkey", 41.01, 28.98, &[]),
    city("Ankara", "Turkey", 39.93, 32.86, &[]),
    city("Tbilisi", "Georgia", 41.72, 44.79, &[]),
    city("Yerevan", "Armenia", 40.18, 44.51, &[]),
    // North America
    city("San Francisco", "United States", 37.77, -122.42, &["sf"]),
    city("Palo Alto", "United States", 37.44, -122.14, &[]),
    city("Mountain View", "United States", 37.39, -122.08, &[]),
    city("San Jose", "United States", 37.34, -121.89, &[]),
    city("Santa Clara", "United States", 37.35, -121.96, &[]),
    city("Berkeley", "United States", 37.87, -122.27, &[]),
    city("Los Angeles", "United States", 34.05, -118.24, &["la"]),
    city("San Diego", "United States", 32.72, -117.16, &[]),
    city("Seattle", "United States", 47.61, -122.33, &[]),
    city("Portland", "United States", 45.52, -122.68, &[]),
    city("New York", "United States", 40.71, -74.01, &["new york city", "nyc", "brooklyn", "manhattan"]),
    city("Boston", "United States", 42.36, -71.06, &[]),
    city("Philadelphia", "United States", 39.95, -75.17, &[]),
    city("Washington", "United States", 38.91, -77.04, &["washington dc", "washington d c"]),
    city("Chicago", "United States", 41.88, -87.63, &[]),
    city("Austin", "United States", 30.27, -97.74, &[]),
    city("Houston", "United States", 29.76, -95.37, &[]),
    city("Dallas", "United States", 32.78, -96.80, &[]),
    city("Atlanta", "United States", 33.75, -84.39, &[]),
    city("Miami", "United States", 25.76, -80.19, &[]),
    city("Denver", "United States", 39.74, -104.99, &[]),
    city("Pittsburgh", "United States", 40.44, -79.99, &[]),
    city("Toronto", "Canada", 43.65, -79.38, &[]),
    city("Waterloo", "Canada", 43.46, -80.52, &[]),
    city("Montreal", "Canada", 45.50, -73.57, &["montréal"]),
    city("Vancouver", "Canada", 49.28, -123.12, &[]),
    city("Ottawa", "Canada", 45.42, -75.70, &[]),
    city("Calgary", "Canada", 51.05, -114.07, &[]),
    city("Mexico City", "Mexico", 19.43, -99.13, &["ciudad de mexico", "cdmx"]),
    // Elsewhere
    city("Tel Aviv", "Israel", 32.09, 34.78, &[]),
    city("Dubai", "United Arab Emirates", 25.20, 55.27, &[]),
    city("Singapore", "Singapore", 1.35, 103.82, &[]),
    city("Tokyo", "Japan", 35.68, 139.69, &[]),
    city("Seoul", "South Korea", 37.57, 126.98, &[]),
    city("Bangalore", "India", 12.97, 77.59, &["bengaluru"]),
    city("Sydney", "Australia", -33.87, 151.21, &[]),
    city("Melbourne", "Australia", -37.81, 144.96, &[]),
    city("São Paulo", "Brazil", -23.55, -46.63, &["sao paulo"]),
    city("Cape Town", "South Africa", -33.92, 18.42, &[]),
    city("Nairobi", "Kenya", -1.29, 36.82, &[]),
    city("Lagos", "Nigeria", 6.52, 3.38, &[]),
];

/// Country names by ISO 3166-1 alpha-2 code, with other names Sources use
/// (already folded, see [`fold`]).
#[rustfmt::skip]
const COUNTRIES: &[(&str, &str, &[&str])] = &[
    ("AT", "Austria", &["osterreich", "oesterreich"]),
    ("BE", "Belgium", &["belgie", "belgique", "belgien"]),
    ("BG", "Bulgaria", &[]),
    ("HR", "Croatia", &["hrvatska"]),
    ("CY", "Cyprus", &[]),
    ("CZ", "Czechia", &["czech republic"]),
    ("DK", "Denmark", &["danmark"]),
    ("EE", "Estonia", &["eesti"]),
    ("FI", "Finland", &["suomi"]),
    ("FR", "France", &[]),
    ("DE", "Germany", &["deutschland"]),
    ("GR", "Greece", &["hellas"]),
    ("HU", "Hungary", &["magyarorszag"]),
    ("IS", "Iceland", &[]),
    ("IE", "Ireland", &["eire"]),
    ("IT", "Italy", &["italia"]),
    ("LV", "Latvia", &[]),
    ("LI", "Liechtenstein", &[]),
    ("LT", "Lithuania", &[]),
    ("LU", "Luxembourg", &["luxemburg"]),
    ("MT", "Malta", &[]),
    ("MD", "Moldova", &[]),
    ("NL", "Netherlands", &["the netherlands", "nederland", "holland"]),
    ("NO", "Norway", &["norge"]),
    ("PL", "Poland", &["polska"]),
    ("PT", "Portugal", &[]),
    ("RO", "Romania", &[]),
    ("RS", "Serbia", &[]),
    ("SK", "Slovakia", &[]),
    ("SI", "Slovenia", &[]),
    ("ES", "Spain", &["espana"]),
    ("SE", "Sweden", &["sverige"]),
    ("CH", "Switzerland", &["schweiz", "suisse", "svizzera"]),
    ("TR", "Turkey", &["turkiye"]),
    ("UA", "Ukraine", &[]),
    ("GB", "United Kingdom", &["uk", "great britain", "england", "scotland", "wales", "northern ireland"]),
    ("GE", "Georgia", &[]),
    ("AM", "Armenia", &[]),
    ("US", "United States", &["usa", "united states of america"]),
    ("CA", "Canada", &[]),
    ("MX", "Mexico", &[]),
    ("BR", "Brazil", &["brasil"]),
    ("IL", "Israel", &[]),
    ("AE", "United Arab Emirates", &["uae"]),
    ("SG", "Singapore", &[]),
    ("JP", "Japan", &[]),
    ("KR", "South Korea", &["korea"]),
    ("IN", "India", &[]),
    ("AU", "Australia", &[]),
    ("ZA", "South Africa", &[]),
    ("KE", "Kenya", &[]),
    ("NG", "Nigeria", &[]),
    ("FJ", "Fiji", &[]),
];

/// How far from a known city's centre coordinates may be and still count as that city.
pub const NEAREST_CITY_KM: f64 = 40.0;

/// Lower-cases, removes accents and turns punctuation into single spaces, so
/// `Zürich`, `ZURICH` and `zurich,` compare equal.
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => out.push('a'),
            'ç' | 'ć' | 'č' => out.push('c'),
            'ď' | 'đ' => out.push('d'),
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => out.push('e'),
            'ğ' => out.push('g'),
            'ì' | 'í' | 'î' | 'ï' | 'ı' | 'ī' => out.push('i'),
            'ł' | 'ľ' => out.push('l'),
            'ñ' | 'ń' | 'ň' => out.push('n'),
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ő' | 'ō' => out.push('o'),
            'ř' => out.push('r'),
            'ś' | 'š' | 'ş' | 'ș' => out.push('s'),
            'ť' | 'ţ' | 'ț' => out.push('t'),
            'ù' | 'ú' | 'û' | 'ü' | 'ů' | 'ű' | 'ū' => out.push('u'),
            'ý' | 'ÿ' => out.push('y'),
            'ź' | 'ż' | 'ž' => out.push('z'),
            'ß' => out.push_str("ss"),
            'æ' => out.push_str("ae"),
            'œ' => out.push_str("oe"),
            c if c.is_alphanumeric() => out.push(c),
            _ => out.push(' '),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The English name of a country, from its name in English or a nearby
/// language, or its two-letter code. `None` when it isn't a known country.
pub fn country_name(text: &str) -> Option<&'static str> {
    let trimmed = text.trim();
    if trimmed.len() == 2 {
        let code = trimmed.to_ascii_uppercase();
        if let Some((_, name, _)) = COUNTRIES.iter().find(|(c, _, _)| *c == code) {
            return Some(name);
        }
    }
    let folded = fold(trimmed);
    COUNTRIES
        .iter()
        .find(|(_, name, aliases)| fold(name) == folded || aliases.contains(&folded.as_str()))
        .map(|(_, name, _)| *name)
}

fn find_city(text: &str) -> Option<&'static City> {
    let folded = fold(text);
    if folded.is_empty() {
        return None;
    }
    CITIES
        .iter()
        .find(|c| fold(c.name) == folded || c.aliases.contains(&folded.as_str()))
}

/// A known city's usual name and country, from any spelling of it.
pub fn known_city(text: &str) -> Option<(&'static str, &'static str)> {
    find_city(text).map(|c| (c.name, c.country))
}

/// The key two city names compare by: the usual name of a known city, or else
/// the folded name, so `Gent` and `Ghent` match.
pub fn city_key(text: &str) -> String {
    match find_city(text) {
        Some(city) => fold(city.name),
        None => fold(text),
    }
}

/// The known city nearest to these coordinates, if one is within [`NEAREST_CITY_KM`].
pub fn nearest_city(lat: f64, lon: f64) -> Option<(&'static str, &'static str)> {
    CITIES
        .iter()
        .map(|c| (distance_km(lat, lon, c.lat, c.lon), c))
        .filter(|(km, _)| *km <= NEAREST_CITY_KM)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, c)| (c.name, c.country))
}

/// Whether two cities are both known and more than `km` apart, so listings
/// in them can't be the same Hackathon. Unknown cities are never far apart.
pub fn far_apart(a: &str, b: &str, km: f64) -> bool {
    match (find_city(a), find_city(b)) {
        (Some(a), Some(b)) => distance_km(a.lat, a.lon, b.lat, b.lon) > km,
        _ => false,
    }
}

/// Great-circle distance (haversine).
fn distance_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * 6371.0 * a.sqrt().asin()
}

/// Reads the city and country from an address such as
/// `Lanolin Fabrik, Salzufer 15/16, 10587 Berlin, Germany`: the country is the
/// last part when it names one, and the city the part before it, without its
/// postcode. A known city fills in a missing country and gets its usual name.
pub fn from_address(address: &str) -> (Option<String>, Option<String>) {
    let mut parts: Vec<&str> = address
        .split([',', '\n'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    let mut country = parts.last().and_then(|last| country_name(last));
    if country.is_some() {
        parts.pop();
    }
    // A known city anywhere in the address wins, nearest the end first, since
    // a venue's name comes before its city.
    for part in parts.iter().rev() {
        let without_postcode = strip_postcode(part);
        if let Some((name, city_country)) = known_city(&without_postcode) {
            return (
                Some(name.to_owned()),
                Some(country.unwrap_or(city_country).to_owned()),
            );
        }
        // `Berlin, Berlin` and `Paris, Île-de-France` name a region after the city.
        if country.is_none()
            && let Some(name) = country_name(&without_postcode)
        {
            country = Some(name);
        }
    }
    let city = parts
        .last()
        .map(|last| strip_postcode(last))
        .filter(|city| !city.is_empty() && !city.chars().any(|c| c.is_ascii_digit()));
    (city, country.map(str::to_owned))
}

/// `10587 Berlin` and `London EC2A 4NE` become `Berlin` and `London`: words
/// holding a digit are postcodes or street numbers.
fn strip_postcode(part: &str) -> String {
    part.split_whitespace()
        .filter(|w| !w.chars().any(|c| c.is_ascii_digit()))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_accents_case_and_punctuation() {
        assert_eq!(fold("Zürich"), "zurich");
        assert_eq!(fold("  {Tech: Europe}  Hack! "), "tech europe hack");
        assert_eq!(fold("Łódź"), "lodz");
        assert_eq!(fold("Straße"), "strasse");
    }

    #[test]
    fn country_names_come_from_codes_and_other_languages() {
        assert_eq!(country_name("DE"), Some("Germany"));
        assert_eq!(country_name("gb"), Some("United Kingdom"));
        assert_eq!(country_name("Deutschland"), Some("Germany"));
        assert_eq!(country_name("The Netherlands"), Some("Netherlands"));
        assert_eq!(country_name("UK"), Some("United Kingdom"));
        assert_eq!(country_name("Belgique"), Some("Belgium"));
        assert_eq!(country_name("Atlantis"), None);
    }

    #[test]
    fn cities_match_by_any_spelling() {
        assert_eq!(known_city("München"), Some(("Munich", "Germany")));
        assert_eq!(known_city("ZÜRICH"), Some(("Zurich", "Switzerland")));
        assert_eq!(city_key("Gent"), city_key("Ghent"));
        assert_eq!(city_key("Bruxelles"), "brussels");
        assert_eq!(city_key("Somewhere Else"), "somewhere else");
    }

    #[test]
    fn coordinates_find_the_nearest_known_city() {
        // Big Berlin Hack's venue, in Neukölln.
        assert_eq!(nearest_city(52.4818, 13.4404), Some(("Berlin", "Germany")));
        // Garching, just north of Munich.
        assert_eq!(nearest_city(48.2671, 11.6624), Some(("Munich", "Germany")));
        // The middle of the North Sea.
        assert_eq!(nearest_city(56.0, 3.0), None);
    }

    #[test]
    fn addresses_give_city_and_country() {
        let cases = [
            (
                "Lanolin Fabrik, Salzufer 15/16, 10587 Berlin, Germany",
                Some("Berlin"),
                Some("Germany"),
            ),
            (
                "J floor, Josefstrasse 206, 8005 Zürich, Switzerland",
                Some("Zurich"),
                Some("Switzerland"),
            ),
            (
                "Google, 6 Pancras Square, London N1C 4AG, UK",
                Some("London"),
                Some("United Kingdom"),
            ),
            ("Berlin, Berlin\nGermany", Some("Berlin"), Some("Germany")),
            ("Kupittaa Campus, Turku", Some("Turku"), Some("Finland")),
            (
                "Some Hall, Smalltown, France",
                Some("Smalltown"),
                Some("France"),
            ),
            ("Amy Gutman Hall", Some("Amy Gutman Hall"), None),
            ("Fiji", None, Some("Fiji")),
        ];
        for (address, city, country) in cases {
            assert_eq!(
                from_address(address),
                (city.map(str::to_owned), country.map(str::to_owned)),
                "{address}"
            );
        }
    }
}
