//! The Profile: one person's settings, kept in a human-readable TOML file with no secrets.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub home: Home,
    /// Country names, such as `Belgium` or `France`.
    pub countries: Vec<String>,
    /// Where Alerts go. Without one, Alerts are printed instead of sent.
    pub alert_channel: Option<AlertChannelSetting>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Home {
    pub city: String,
    /// IATA codes of the airports the person flies out of, upper-cased.
    pub airports: Vec<String>,
}

/// The Alert channel a Profile names, with its settings. Secrets never live here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlertChannelSetting {
    Ntfy(NtfySetting),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NtfySetting {
    /// The server's base URL, without a trailing slash.
    pub server: String,
    pub topic: String,
}

pub const DEFAULT_NTFY_SERVER: &str = "https://ntfy.sh";

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error(
        "no Profile at {}; create one (see examples/profile.toml) or pass --profile <PATH>",
        path.display()
    )]
    NotFound { path: PathBuf },
    #[error("could not read the Profile at {}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error("the Profile at {} is not valid TOML: {message}", path.display())]
    Syntax { path: PathBuf, message: String },
    #[error("the Profile at {} is invalid:\n{}", path.display(), Problems(problems))]
    Invalid {
        path: PathBuf,
        problems: Vec<String>,
    },
}

struct Problems<'a>(&'a [String]);

impl fmt::Display for Problems<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let lines: Vec<String> = self.0.iter().map(|p| format!("  - {p}")).collect();
        f.write_str(&lines.join("\n"))
    }
}

// The file's shape. Unknown keys are refused so a typo is reported instead of ignored.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileFile {
    home: HomeFile,
    countries: Vec<String>,
    alert_channel: Option<AlertChannelFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HomeFile {
    city: String,
    airports: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AlertChannelFile {
    kind: String,
    topic: Option<String>,
    server: Option<String>,
}

impl Profile {
    pub fn load(path: &Path) -> Result<Self, ProfileError> {
        let text = std::fs::read_to_string(path).map_err(|source| {
            if source.kind() == io::ErrorKind::NotFound {
                ProfileError::NotFound {
                    path: path.to_owned(),
                }
            } else {
                ProfileError::Read {
                    path: path.to_owned(),
                    source,
                }
            }
        })?;
        Self::from_toml(&text, path)
    }

    /// Parses and validates Profile text; `path` only labels errors.
    pub fn from_toml(text: &str, path: &Path) -> Result<Self, ProfileError> {
        let file: ProfileFile = toml::from_str(text).map_err(|e| ProfileError::Syntax {
            path: path.to_owned(),
            message: e.to_string().trim_end().to_owned(),
        })?;
        let mut problems = Vec::new();

        let city = file.home.city.trim().to_owned();
        if city.is_empty() {
            problems.push("home.city is empty; give the city you travel from".to_owned());
        }

        let airports: Vec<String> = file
            .home
            .airports
            .iter()
            .map(|a| a.trim().to_ascii_uppercase())
            .collect();
        if airports.is_empty() {
            problems.push(
                "home.airports is empty; list at least one airport code, such as \"BRU\""
                    .to_owned(),
            );
        }
        for (raw, code) in file.home.airports.iter().zip(&airports) {
            if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_uppercase()) {
                problems.push(format!(
                    "home.airports: {raw:?} is not a 3-letter IATA airport code"
                ));
            }
        }

        let countries: Vec<String> = file.countries.iter().map(|c| c.trim().to_owned()).collect();
        if countries.is_empty() {
            problems.push(
                "countries is empty; list at least one country, such as \"Belgium\"".to_owned(),
            );
        }
        if countries.iter().any(String::is_empty) {
            problems.push("countries has an empty entry".to_owned());
        }

        let alert_channel = file
            .alert_channel
            .and_then(|channel| alert_channel(channel, &mut problems));

        if !problems.is_empty() {
            return Err(ProfileError::Invalid {
                path: path.to_owned(),
                problems,
            });
        }
        Ok(Profile {
            home: Home { city, airports },
            countries,
            alert_channel,
        })
    }

    pub fn includes_country(&self, country: &str) -> bool {
        let country = country.trim();
        self.countries
            .iter()
            .any(|c| c.to_lowercase() == country.to_lowercase())
    }
}

fn alert_channel(
    file: AlertChannelFile,
    problems: &mut Vec<String>,
) -> Option<AlertChannelSetting> {
    match file.kind.trim() {
        "ntfy" => ntfy(file, problems).map(AlertChannelSetting::Ntfy),
        other => {
            problems.push(format!(
                "alert_channel.kind: {other:?} is not an Alert channel; use \"ntfy\""
            ));
            None
        }
    }
}

fn ntfy(file: AlertChannelFile, problems: &mut Vec<String>) -> Option<NtfySetting> {
    let before = problems.len();

    let topic = file.topic.as_deref().map(str::trim).unwrap_or_default();
    if topic.is_empty() {
        problems.push(
            "alert_channel.topic is missing; ntfy needs a topic, such as \"hackathon-radar-7f3k9q\""
                .to_owned(),
        );
    } else if topic.len() > 64
        || !topic
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        // ntfy's own rule for topic names, which also keeps the topic safe in a URL.
        problems.push(format!(
            "alert_channel.topic: {topic:?} is not an ntfy topic; use up to 64 letters, digits, - or _"
        ));
    }

    let server = file
        .server
        .as_deref()
        .map(|s| s.trim().trim_end_matches('/'))
        .unwrap_or(DEFAULT_NTFY_SERVER);
    let has_scheme = server.starts_with("https://") || server.starts_with("http://");
    if !has_scheme || server.contains(char::is_whitespace) {
        problems.push(format!(
            "alert_channel.server: {server:?} is not a URL; leave it out to use {DEFAULT_NTFY_SERVER}"
        ));
    }

    (problems.len() == before).then(|| NtfySetting {
        server: server.to_owned(),
        topic: topic.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Profile, ProfileError> {
        Profile::from_toml(text, Path::new("profile.toml"))
    }

    #[test]
    fn reads_a_valid_profile() {
        let profile = parse(
            r#"
            countries = ["Belgium", " France "]

            [home]
            city = "Brussels"
            airports = ["bru", "CRL"]
            "#,
        )
        .unwrap();
        assert_eq!(
            profile,
            Profile {
                home: Home {
                    city: "Brussels".into(),
                    airports: vec!["BRU".into(), "CRL".into()],
                },
                countries: vec!["Belgium".into(), "France".into()],
                alert_channel: None,
            }
        );
        assert!(profile.includes_country("belgium"));
        assert!(!profile.includes_country("Germany"));
    }

    #[test]
    fn lists_every_problem_at_once() {
        let err = parse(
            r#"
            countries = []

            [home]
            city = " "
            airports = ["BRUX"]
            "#,
        )
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "the Profile at profile.toml is invalid:\n\
             \x20 - home.city is empty; give the city you travel from\n\
             \x20 - home.airports: \"BRUX\" is not a 3-letter IATA airport code\n\
             \x20 - countries is empty; list at least one country, such as \"Belgium\""
        );
    }

    #[test]
    fn requires_an_airport() {
        let err = parse("countries = [\"Belgium\"]\n[home]\ncity = \"Lyon\"\nairports = []\n")
            .unwrap_err();
        assert!(err.to_string().contains("home.airports is empty"), "{err}");
    }

    #[test]
    fn missing_keys_are_named() {
        let err = parse("[home]\ncity = \"Lyon\"\nairports = [\"LYS\"]\n").unwrap_err();
        assert!(matches!(err, ProfileError::Syntax { .. }));
        assert!(err.to_string().contains("countries"), "{err}");
    }

    #[test]
    fn unknown_keys_are_refused() {
        let err = parse(
            "countries = [\"France\"]\ncountry = \"France\"\n[home]\ncity = \"Lyon\"\nairports = [\"LYS\"]\n",
        )
        .unwrap_err();
        assert!(err.to_string().contains("country"), "{err}");
    }

    #[test]
    fn a_missing_file_says_how_to_fix_it() {
        let err = Profile::load(Path::new("/nonexistent/profile.toml")).unwrap_err();
        assert!(matches!(err, ProfileError::NotFound { .. }));
        assert!(err.to_string().contains("--profile"), "{err}");
    }

    const HOME: &str = "countries = [\"Belgium\"]\n[home]\ncity = \"Lyon\"\nairports = [\"LYS\"]\n";

    #[test]
    fn ntfy_defaults_to_ntfy_sh() {
        let profile = parse(&format!(
            "{HOME}[alert_channel]\nkind = \"ntfy\"\ntopic = \"radar-42\"\n"
        ))
        .unwrap();
        assert_eq!(
            profile.alert_channel,
            Some(AlertChannelSetting::Ntfy(NtfySetting {
                server: "https://ntfy.sh".into(),
                topic: "radar-42".into(),
            }))
        );
    }

    #[test]
    fn ntfy_takes_a_self_hosted_server() {
        let profile = parse(&format!(
            "{HOME}[alert_channel]\nkind = \"ntfy\"\ntopic = \"radar\"\nserver = \"https://ntfy.example.org/\"\n"
        ))
        .unwrap();
        assert_eq!(
            profile.alert_channel,
            Some(AlertChannelSetting::Ntfy(NtfySetting {
                server: "https://ntfy.example.org".into(),
                topic: "radar".into(),
            }))
        );
    }

    #[test]
    fn ntfy_needs_a_valid_topic_and_server() {
        let missing = parse(&format!("{HOME}[alert_channel]\nkind = \"ntfy\"\n")).unwrap_err();
        assert!(
            missing
                .to_string()
                .contains("alert_channel.topic is missing"),
            "{missing}"
        );

        let bad = parse(&format!(
            "{HOME}[alert_channel]\nkind = \"ntfy\"\ntopic = \"my topic/x\"\nserver = \"ntfy.sh\"\n"
        ))
        .unwrap_err();
        assert_eq!(
            bad.to_string(),
            "the Profile at profile.toml is invalid:\n\
             \x20 - alert_channel.topic: \"my topic/x\" is not an ntfy topic; use up to 64 letters, digits, - or _\n\
             \x20 - alert_channel.server: \"ntfy.sh\" is not a URL; leave it out to use https://ntfy.sh"
        );
    }

    #[test]
    fn unknown_alert_channels_and_keys_are_refused() {
        let kind = parse(&format!("{HOME}[alert_channel]\nkind = \"pigeon\"\n")).unwrap_err();
        assert!(
            kind.to_string()
                .contains("\"pigeon\" is not an Alert channel"),
            "{kind}"
        );

        // A token is a secret, so it has no place in the Profile.
        let token = parse(&format!(
            "{HOME}[alert_channel]\nkind = \"ntfy\"\ntopic = \"radar\"\ntoken = \"tk_x\"\n"
        ))
        .unwrap_err();
        assert!(matches!(token, ProfileError::Syntax { .. }), "{token}");
    }

    #[test]
    fn the_example_profile_is_valid() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/profile.toml");
        let text = std::fs::read_to_string(&path).unwrap();
        Profile::from_toml(&text, &path).unwrap();
        // Its commented-out ntfy block is valid too, once uncommented.
        let uncommented = text
            .replace("# [alert_channel]", "[alert_channel]")
            .replace("# kind = ", "kind = ")
            .replace("# topic = ", "topic = ");
        let profile = Profile::from_toml(&uncommented, &path).unwrap();
        assert!(matches!(
            profile.alert_channel,
            Some(AlertChannelSetting::Ntfy(_))
        ));
    }
}
