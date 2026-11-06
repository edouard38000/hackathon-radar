//! Writing a Profile: `hackathon-radar init` edits the person's Profile file
//! in place, so a re-run changes only the values it was given and keeps every
//! other key, comment and section the person wrote.

use std::path::Path;

use anyhow::Context;
use toml_edit::{Array, DocumentMut, Item, Table, Value};

use super::{Profile, ProfileError};

/// What a Profile written from scratch starts with.
const HEADER: &str = "\
# Your hackathon-radar Profile, written by `hackathon-radar init`. Run init
# again to change it, or edit it by hand: see examples/profile.toml for every
# setting. It holds no secrets; those live in your keychain.

";

/// A Profile file being edited. Keys are dotted paths of at most two parts,
/// such as `countries` or `home.city`.
#[derive(Debug, Clone)]
pub struct ProfileDocument {
    doc: DocumentMut,
    /// Whether it is new, so it gets the header comment.
    new: bool,
}

impl Default for ProfileDocument {
    fn default() -> Self {
        Self::new()
    }
}

impl ProfileDocument {
    /// An empty Profile, to be filled in.
    pub fn new() -> Self {
        Self {
            doc: DocumentMut::new(),
            new: true,
        }
    }

    /// The Profile in `text`, which only has to be TOML: an invalid Profile
    /// can be edited into a valid one. `path` only labels errors.
    pub fn parse(text: &str, path: &Path) -> Result<Self, ProfileError> {
        let doc = text
            .parse::<DocumentMut>()
            .map_err(|e| ProfileError::Syntax {
                path: path.to_owned(),
                message: e.to_string().trim_end().to_owned(),
            })?;
        Ok(Self { doc, new: false })
    }

    /// The Profile file at `path`, or `None` when there is none yet.
    pub fn load(path: &Path) -> Result<Option<Self>, ProfileError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text, path).map(Some),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(ProfileError::Read {
                path: path.to_owned(),
                source,
            }),
        }
    }

    /// Whether this Profile is being written from scratch.
    pub fn is_new(&self) -> bool {
        self.new
    }

    fn item(&self, key: &str) -> Option<&Item> {
        match key.split_once('.') {
            None => self.doc.get(key),
            Some((table, key)) => self.doc.get(table)?.as_table_like()?.get(key),
        }
    }

    fn value(&self, key: &str) -> Option<&Value> {
        self.item(key)?.as_value()
    }

    /// Whether the key is there at all.
    pub fn contains(&self, key: &str) -> bool {
        self.item(key).is_some_and(|i| !i.is_none())
    }

    pub fn get_str(&self, key: &str) -> Option<String> {
        self.value(key)?.as_str().map(str::to_owned)
    }

    /// A list of strings; entries that aren't strings are left out.
    pub fn get_strings(&self, key: &str) -> Option<Vec<String>> {
        Some(
            self.value(key)?
                .as_array()?
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect(),
        )
    }

    pub fn get_int(&self, key: &str) -> Option<i64> {
        self.value(key)?.as_integer()
    }

    /// A number, written as an integer or a float.
    pub fn get_number(&self, key: &str) -> Option<f64> {
        let value = self.value(key)?;
        value
            .as_float()
            .or_else(|| value.as_integer().map(|i| i as f64))
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.value(key)?.as_bool()
    }

    /// Sets the value, keeping the comments around the key. A value that is
    /// already there is left untouched, formatting included.
    fn set(&mut self, key: &str, mut value: Value, unchanged: bool) {
        if unchanged {
            return;
        }
        let (table, key) = match key.split_once('.') {
            None => (
                self.doc.as_table_mut() as &mut dyn toml_edit::TableLike,
                key,
            ),
            Some((name, key)) => (self.table_mut(name), key),
        };
        match table.get_mut(key) {
            // Replaced in place, so the key keeps the comments above it.
            Some(item) if !item.is_none() => {
                if let Some(old) = item.as_value() {
                    *value.decor_mut() = old.decor().clone();
                }
                *item = Item::Value(value);
            }
            _ => {
                table.insert(key, Item::Value(value));
            }
        }
    }

    /// The table `name`, created when it isn't there.
    fn table_mut(&mut self, name: &str) -> &mut dyn toml_edit::TableLike {
        let root = self.doc.as_table_mut();
        if !root.get(name).is_some_and(Item::is_table_like) {
            root.insert(name, Item::Table(Table::new()));
        }
        root.get_mut(name)
            .and_then(Item::as_table_like_mut)
            .expect("the table was just made")
    }

    pub fn set_str(&mut self, key: &str, value: &str) {
        let unchanged = self.get_str(key).as_deref() == Some(value);
        self.set(key, Value::from(value), unchanged);
    }

    pub fn set_strings(&mut self, key: &str, values: &[String]) {
        let unchanged = self.get_strings(key).as_deref() == Some(values);
        self.set(key, Value::Array(Array::from_iter(values)), unchanged);
    }

    pub fn set_int(&mut self, key: &str, value: i64) {
        let unchanged = self.get_int(key) == Some(value);
        self.set(key, Value::from(value), unchanged);
    }

    /// Sets a number, written as an integer when it is whole.
    pub fn set_number(&mut self, key: &str, value: f64) {
        let unchanged = self.get_number(key) == Some(value);
        let value = if value.fract() == 0.0 && value.abs() < 1e15 {
            Value::from(value as i64)
        } else {
            Value::from(value)
        };
        self.set(key, value, unchanged);
    }

    pub fn set_bool(&mut self, key: &str, value: bool) {
        let unchanged = self.get_bool(key) == Some(value);
        self.set(key, Value::from(value), unchanged);
    }

    /// Removes the key, or a whole table when `key` names one.
    pub fn remove(&mut self, key: &str) {
        match key.split_once('.') {
            None => {
                self.doc.as_table_mut().remove(key);
            }
            Some((table, key)) => {
                if let Some(table) = self
                    .doc
                    .as_table_mut()
                    .get_mut(table)
                    .and_then(Item::as_table_like_mut)
                {
                    table.remove(key);
                }
            }
        }
    }

    /// The Profile's text.
    pub fn to_toml(&self) -> String {
        let body = self.doc.to_string();
        if self.new {
            format!("{HEADER}{}", body.trim_start())
        } else {
            body
        }
    }

    /// The Profile this text makes, with every problem listed at once if it is
    /// invalid. `path` only labels errors.
    pub fn validate(&self, path: &Path) -> Result<Profile, ProfileError> {
        Profile::from_toml(&self.to_toml(), path)
    }

    /// Validates the Profile and writes it to `path`, replacing the file in
    /// one step so a failed write never leaves half a Profile behind.
    pub fn save(&self, path: &Path) -> anyhow::Result<Profile> {
        let profile = self.validate(path)?;
        let dir = path
            .parent()
            .filter(|d| !d.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        std::fs::create_dir_all(dir)
            .with_context(|| format!("could not create {}", dir.display()))?;
        let previous = std::fs::metadata(path).ok().map(|m| m.permissions());
        let file = tempfile::NamedTempFile::new_in(dir)
            .with_context(|| format!("could not write a file in {}", dir.display()))?;
        std::fs::write(file.path(), self.to_toml())
            .with_context(|| format!("could not write {}", path.display()))?;
        // The Profile holds no secrets, so it keeps its permissions, or gets
        // the usual ones when it is new (the temporary file starts as 0600).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = previous.unwrap_or_else(|| std::fs::Permissions::from_mode(0o644));
            std::fs::set_permissions(file.path(), permissions)
                .with_context(|| format!("could not set the permissions of {}", path.display()))?;
        }
        #[cfg(not(unix))]
        let _ = previous;
        file.persist(path)
            .with_context(|| format!("could not write {}", path.display()))?;
        Ok(profile)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATH: &str = "profile.toml";

    #[test]
    fn a_new_profile_gets_the_header_and_its_tables_in_order() {
        let mut doc = ProfileDocument::new();
        doc.set_strings("countries", &["Belgium".into(), "France".into()]);
        doc.set_str("home.city", "Brussels");
        doc.set_strings("home.airports", &["BRU".into()]);
        doc.set_number("travel.train_limit_hours", 4.0);
        doc.set_bool("travel.travel_funded_anywhere", true);
        let text = doc.to_toml();
        assert!(text.starts_with("# Your hackathon-radar Profile"), "{text}");
        assert!(
            text.contains(
                "countries = [\"Belgium\", \"France\"]\n\n[home]\ncity = \"Brussels\"\n\
                 airports = [\"BRU\"]\n\n[travel]\ntrain_limit_hours = 4\n\
                 travel_funded_anywhere = true\n"
            ),
            "{text}"
        );
        let profile = doc.validate(Path::new(PATH)).unwrap();
        assert!(profile.travel.travel_funded_anywhere);
    }

    #[test]
    fn editing_keeps_comments_other_keys_and_unchanged_values() {
        let text = "# My notes\ncountries = [\"Belgium\"]  # for now\n\n[home]\n# where I live\n\
                    city = \"Lyon\"\nairports = [ \"LYS\" ]\n\n[deadlines]\nreminder_days = 5\n";
        let mut doc = ProfileDocument::parse(text, Path::new(PATH)).unwrap();
        doc.set_strings("countries", &["Belgium".into(), "Schengen".into()]);
        doc.set_strings("home.airports", &["LYS".into()]);
        doc.set_str("home.city", "Lyon");
        doc.set_number("travel.train_limit_hours", 5.5);
        let edited = doc.to_toml();
        assert_eq!(
            edited,
            "# My notes\ncountries = [\"Belgium\", \"Schengen\"]  # for now\n\n[home]\n# where I live\n\
             city = \"Lyon\"\nairports = [ \"LYS\" ]\n\n[deadlines]\nreminder_days = 5\n\n\
             [travel]\ntrain_limit_hours = 5.5\n"
        );
        assert_eq!(doc.get_number("travel.train_limit_hours"), Some(5.5));
    }

    #[test]
    fn removes_keys_and_tables() {
        let text = "countries = [\"Belgium\"]\n[home]\ncity = \"Lyon\"\nairports = [\"LYS\"]\n\
                    [alert_channel]\nkind = \"ntfy\"\ntopic = \"radar\"\n";
        let mut doc = ProfileDocument::parse(text, Path::new(PATH)).unwrap();
        doc.remove("alert_channel.topic");
        assert!(!doc.contains("alert_channel.topic"));
        assert!(doc.contains("alert_channel.kind"));
        doc.remove("alert_channel");
        assert!(!doc.contains("alert_channel.kind"));
        assert!(
            doc.validate(Path::new(PATH))
                .unwrap()
                .alert_channel
                .is_none()
        );
    }

    #[test]
    fn saving_validates_and_lists_every_problem() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profile.toml");
        let mut doc = ProfileDocument::new();
        doc.set_strings("countries", &[]);
        doc.set_str("home.city", "Lyon");
        doc.set_strings("home.airports", &["LYSX".into()]);
        let error = doc.save(&path).unwrap_err().to_string();
        assert!(error.contains("home.airports"), "{error}");
        assert!(error.contains("countries is empty"), "{error}");
        assert!(!path.exists(), "nothing is written");

        doc.set_strings("countries", &["France".into()]);
        doc.set_strings("home.airports", &["LYS".into()]);
        doc.save(&path).unwrap();
        let saved = ProfileDocument::load(&path).unwrap().unwrap();
        assert_eq!(saved.get_str("home.city").as_deref(), Some("Lyon"));
        assert!(!saved.is_new());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o644);
        }
    }

    #[test]
    fn reads_inline_tables_too() {
        let text = "countries = [\"France\"]\nhome = { city = \"Lyon\", airports = [\"LYS\"] }\n";
        let mut doc = ProfileDocument::parse(text, Path::new(PATH)).unwrap();
        assert_eq!(doc.get_str("home.city").as_deref(), Some("Lyon"));
        doc.set_str("home.country", "France");
        let profile = doc.validate(Path::new(PATH)).unwrap();
        assert_eq!(profile.home.country.as_deref(), Some("France"));
    }
}
