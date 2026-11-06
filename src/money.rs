//! Money: the currency a Profile counts in, and how amounts are written
//! wherever money is shown (the Flight budget and Fares).

use std::fmt;

use crate::sources::places;

/// An ISO 4217 currency code, such as `EUR` or `USD`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Currency(String);

/// The currency when Home's country doesn't say: the euro, as most early users are in Europe.
pub const DEFAULT_CURRENCY: &str = "EUR";

/// Countries whose currency isn't the euro, by English name.
#[rustfmt::skip]
const CURRENCIES: &[(&str, &str)] = &[
    ("United States", "USD"), ("Canada", "CAD"), ("Mexico", "MXN"),
    ("United Kingdom", "GBP"), ("Switzerland", "CHF"), ("Liechtenstein", "CHF"),
    ("Sweden", "SEK"), ("Norway", "NOK"), ("Denmark", "DKK"), ("Iceland", "ISK"),
    ("Poland", "PLN"), ("Czechia", "CZK"), ("Hungary", "HUF"), ("Romania", "RON"),
    ("Serbia", "RSD"), ("Albania", "ALL"), ("Bosnia and Herzegovina", "BAM"),
    ("North Macedonia", "MKD"), ("Moldova", "MDL"), ("Ukraine", "UAH"), ("Belarus", "BYN"),
    ("Turkey", "TRY"), ("Georgia", "GEL"), ("Armenia", "AMD"),
    ("Brazil", "BRL"), ("Argentina", "ARS"), ("Chile", "CLP"), ("Colombia", "COP"), ("Peru", "PEN"),
    ("Israel", "ILS"), ("United Arab Emirates", "AED"), ("Saudi Arabia", "SAR"), ("Qatar", "QAR"),
    ("Egypt", "EGP"), ("Morocco", "MAD"), ("Tunisia", "TND"), ("Libya", "LYD"),
    ("Pakistan", "PKR"), ("India", "INR"), ("Singapore", "SGD"), ("Japan", "JPY"),
    ("South Korea", "KRW"), ("China", "CNY"), ("Hong Kong", "HKD"), ("Taiwan", "TWD"),
    ("Thailand", "THB"), ("Vietnam", "VND"), ("Malaysia", "MYR"), ("Indonesia", "IDR"),
    ("Philippines", "PHP"), ("Australia", "AUD"), ("New Zealand", "NZD"), ("Fiji", "FJD"),
    ("South Africa", "ZAR"), ("Kenya", "KES"), ("Nigeria", "NGN"), ("Ghana", "GHS"),
    ("Rwanda", "RWF"),
];

/// Symbols written before an amount; other currencies are written with their code.
const SYMBOLS: &[(&str, &str)] = &[
    ("EUR", "€"),
    ("USD", "$"),
    ("GBP", "£"),
    ("JPY", "¥"),
    ("INR", "₹"),
    ("CAD", "CA$"),
    ("AUD", "A$"),
    ("NZD", "NZ$"),
];

impl Currency {
    /// Reads an ISO 4217 code, in any case; `None` unless it is three letters.
    pub fn parse(code: &str) -> Option<Self> {
        let code = code.trim().to_ascii_uppercase();
        (code.len() == 3 && code.bytes().all(|b| b.is_ascii_uppercase())).then_some(Self(code))
    }

    /// The currency of a country (by any name [`places::country_name`] knows),
    /// or else the euro.
    pub fn of_country(country: Option<&str>) -> Self {
        let country = country.and_then(places::country_name);
        let code = country
            .and_then(|c| CURRENCIES.iter().find(|(name, _)| *name == c))
            .map_or(DEFAULT_CURRENCY, |(_, code)| *code);
        Self(code.to_owned())
    }

    pub fn code(&self) -> &str {
        &self.0
    }

    /// Writes a whole amount in this currency: `€180`, `$1,250`, `CHF 95`.
    pub fn format(&self, amount: u64) -> String {
        let digits = group_thousands(amount);
        match SYMBOLS.iter().find(|(code, _)| *code == self.0) {
            Some((_, symbol)) => format!("{symbol}{digits}"),
            None => format!("{} {digits}", self.0),
        }
    }
}

impl Default for Currency {
    fn default() -> Self {
        Self(DEFAULT_CURRENCY.to_owned())
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// `1250000` becomes `1,250,000`.
fn group_thousands(amount: u64) -> String {
    let digits = amount.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_three_letters() {
        assert_eq!(Currency::parse(" usd ").unwrap().code(), "USD");
        assert_eq!(Currency::parse("EURO"), None);
        assert_eq!(Currency::parse("U$D"), None);
        assert_eq!(Currency::parse(""), None);
    }

    #[test]
    fn the_currency_follows_homes_country() {
        assert_eq!(Currency::of_country(Some("United States")).code(), "USD");
        assert_eq!(Currency::of_country(Some("USA")).code(), "USD");
        assert_eq!(Currency::of_country(Some("Belgium")).code(), "EUR");
        assert_eq!(Currency::of_country(Some("Bulgaria")).code(), "EUR");
        assert_eq!(Currency::of_country(Some("Schweiz")).code(), "CHF");
        assert_eq!(Currency::of_country(Some("UK")).code(), "GBP");
        assert_eq!(Currency::of_country(Some("Atlantis")).code(), "EUR");
        assert_eq!(Currency::of_country(None).code(), "EUR");
    }

    #[test]
    fn amounts_are_written_with_a_symbol_or_the_code() {
        let format = |code: &str, amount| Currency::parse(code).unwrap().format(amount);
        assert_eq!(format("EUR", 180), "€180");
        assert_eq!(format("USD", 1250), "$1,250");
        assert_eq!(format("GBP", 95), "£95");
        assert_eq!(format("CAD", 300), "CA$300");
        assert_eq!(format("CHF", 95), "CHF 95");
        assert_eq!(format("SEK", 1250000), "SEK 1,250,000");
        assert_eq!(format("EUR", 0), "€0");
    }
}
