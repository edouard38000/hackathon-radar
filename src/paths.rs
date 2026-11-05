//! Where the Profile and the database live by default, following the XDG base
//! directories on both macOS and Linux.

use std::ffi::OsString;
use std::path::PathBuf;

const APP_DIR: &str = "hackathon-radar";

/// `$XDG_CONFIG_HOME/hackathon-radar/profile.toml`, else `~/.config/hackathon-radar/profile.toml`.
pub fn default_profile_path() -> anyhow::Result<PathBuf> {
    profile_path_from(&|key| std::env::var_os(key))
}

/// `$XDG_DATA_HOME/hackathon-radar/hackathon-radar.sqlite`, else under `~/.local/share`.
pub fn default_database_path() -> anyhow::Result<PathBuf> {
    database_path_from(&|key| std::env::var_os(key))
}

/// `$XDG_STATE_HOME/hackathon-radar/run.log`, else under `~/.local/state`: where scheduled Runs log.
pub fn default_log_path() -> anyhow::Result<PathBuf> {
    log_path_from(&|key| std::env::var_os(key))
}

/// `~/Library/LaunchAgents`, where launchd looks for a person's own agents on macOS.
pub fn launch_agents_dir() -> anyhow::Result<PathBuf> {
    launch_agents_dir_from(&|key| std::env::var_os(key))
}

/// `$XDG_CONFIG_HOME/systemd/user`, else `~/.config/systemd/user`: systemd user units on Linux.
pub fn systemd_user_dir() -> anyhow::Result<PathBuf> {
    systemd_user_dir_from(&|key| std::env::var_os(key))
}

type Env<'a> = &'a dyn Fn(&str) -> Option<OsString>;

fn log_path_from(env: Env<'_>) -> anyhow::Result<PathBuf> {
    Ok(base_dir(env, "XDG_STATE_HOME", ".local/state")?
        .join(APP_DIR)
        .join("run.log"))
}

fn launch_agents_dir_from(env: Env<'_>) -> anyhow::Result<PathBuf> {
    let home = env("HOME")
        .filter(|h| !h.is_empty())
        .ok_or_else(|| anyhow::anyhow!("HOME is not set"))?;
    Ok(PathBuf::from(home).join("Library/LaunchAgents"))
}

fn systemd_user_dir_from(env: Env<'_>) -> anyhow::Result<PathBuf> {
    Ok(base_dir(env, "XDG_CONFIG_HOME", ".config")?.join("systemd/user"))
}

fn profile_path_from(env: Env<'_>) -> anyhow::Result<PathBuf> {
    Ok(base_dir(env, "XDG_CONFIG_HOME", ".config")?
        .join(APP_DIR)
        .join("profile.toml"))
}

fn database_path_from(env: Env<'_>) -> anyhow::Result<PathBuf> {
    Ok(base_dir(env, "XDG_DATA_HOME", ".local/share")?
        .join(APP_DIR)
        .join("hackathon-radar.sqlite"))
}

fn base_dir(env: Env<'_>, xdg_var: &str, home_fallback: &str) -> anyhow::Result<PathBuf> {
    // The XDG spec says relative values are invalid and must be ignored.
    if let Some(dir) = env(xdg_var).map(PathBuf::from).filter(|d| d.is_absolute()) {
        return Ok(dir);
    }
    let home = env("HOME")
        .filter(|h| !h.is_empty())
        .ok_or_else(|| anyhow::anyhow!("HOME is not set; pass the path explicitly"))?;
    Ok(PathBuf::from(home).join(home_fallback))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.into())
    }

    #[test]
    fn defaults_under_home() {
        let env = env(&[("HOME", "/home/ada")]);
        assert_eq!(
            profile_path_from(&env).unwrap(),
            PathBuf::from("/home/ada/.config/hackathon-radar/profile.toml")
        );
        assert_eq!(
            database_path_from(&env).unwrap(),
            PathBuf::from("/home/ada/.local/share/hackathon-radar/hackathon-radar.sqlite")
        );
    }

    #[test]
    fn honours_absolute_xdg_dirs_only() {
        let env = env(&[
            ("HOME", "/home/ada"),
            ("XDG_CONFIG_HOME", "/cfg"),
            ("XDG_DATA_HOME", "relative"),
        ]);
        assert_eq!(
            profile_path_from(&env).unwrap(),
            PathBuf::from("/cfg/hackathon-radar/profile.toml")
        );
        assert_eq!(
            database_path_from(&env).unwrap(),
            PathBuf::from("/home/ada/.local/share/hackathon-radar/hackathon-radar.sqlite")
        );
    }

    #[test]
    fn no_home_is_an_error() {
        assert!(profile_path_from(&env(&[])).is_err());
        assert!(launch_agents_dir_from(&env(&[])).is_err());
    }

    #[test]
    fn schedule_locations_under_home() {
        let env = env(&[("HOME", "/home/ada")]);
        assert_eq!(
            log_path_from(&env).unwrap(),
            PathBuf::from("/home/ada/.local/state/hackathon-radar/run.log")
        );
        assert_eq!(
            launch_agents_dir_from(&env).unwrap(),
            PathBuf::from("/home/ada/Library/LaunchAgents")
        );
        assert_eq!(
            systemd_user_dir_from(&env).unwrap(),
            PathBuf::from("/home/ada/.config/systemd/user")
        );
    }

    #[test]
    fn schedule_locations_honour_xdg_dirs() {
        let env = env(&[
            ("HOME", "/home/ada"),
            ("XDG_STATE_HOME", "/state"),
            ("XDG_CONFIG_HOME", "/cfg"),
        ]);
        assert_eq!(
            log_path_from(&env).unwrap(),
            PathBuf::from("/state/hackathon-radar/run.log")
        );
        assert_eq!(
            systemd_user_dir_from(&env).unwrap(),
            PathBuf::from("/cfg/systemd/user")
        );
    }
}
