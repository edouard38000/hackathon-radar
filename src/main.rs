use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;

use anyhow::Context;
use clap::{Parser, Subcommand};

use hackathon_radar::agent;
use hackathon_radar::alert::ntfy::{self, NtfyChannel};
use hackathon_radar::alert::photon::{self, PhotonChannel};
use hackathon_radar::alert::{AlertChannel, PrintChannel};
use hackathon_radar::clock::{Clock, SystemClock};
use hackathon_radar::http::HttpClient;
use hackathon_radar::process::SystemRunner;
use hackathon_radar::profile::{AlertChannelSetting, Profile};
use hackathon_radar::run::{self, RunDeps, RunOptions};
use hackathon_radar::sources::Source;
use hackathon_radar::sources::calendar::IcsCalendar;
use hackathon_radar::sources::devpost::Devpost;
use hackathon_radar::sources::hackathon_be::HackathonBe;
use hackathon_radar::sources::hackathonhub::{self, HackathonHub};
use hackathon_radar::sources::luma::{self, LumaCalendar};
use hackathon_radar::sources::polite::PoliteHttp;
use hackathon_radar::{paths, store::Store};

#[derive(Parser)]
#[command(name = "hackathon-radar", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Collect Hackathons from the Sources, store those that Fit, and compose the Alert.
    Run(RunArgs),
    /// Install, remove or check the recurring Run (launchd on macOS, systemd on Linux).
    Schedule(ScheduleArgs),
}

#[derive(clap::Args)]
struct RunArgs {
    /// Print the Alert instead of sending it.
    #[arg(long)]
    dry_run: bool,

    /// The Profile file [default: ~/.config/hackathon-radar/profile.toml]
    #[arg(long, value_name = "PATH")]
    profile: Option<PathBuf>,

    /// The database file [default: ~/.local/share/hackathon-radar/hackathon-radar.sqlite]
    #[arg(long, value_name = "PATH")]
    db: Option<PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Run(args) => run_command(args),
        Command::Schedule(args) => schedule_command(args),
    };
    match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run_command(args: RunArgs) -> anyhow::Result<ExitCode> {
    let profile_path = match args.profile {
        Some(path) => path,
        None => paths::default_profile_path()?,
    };
    let db_path = match args.db {
        Some(path) => path,
        None => paths::default_database_path()?,
    };
    let profile = Profile::load(&profile_path)?;
    let mut store = Store::open(&db_path)?;
    let sources = sources(&profile, &store)?;
    let mut channel = alert_channel(&profile, args.dry_run)?;
    let agent = agent::from_settings(&profile.agent);
    if let Some(warning) = agent.warning() {
        eprintln!("warning: {warning}");
    }

    let outcome = run::run(
        RunDeps {
            profile: &profile,
            sources: &sources,
            clock: &SystemClock,
            store: &mut store,
            channel: channel.as_mut(),
            agent: agent.as_ref(),
        },
        RunOptions {
            dry_run: args.dry_run,
        },
    )
    .context("the Run failed")?;

    for failure in &outcome.failed_sources {
        eprintln!(
            "warning: could not read the Source {}: {}",
            failure.source, failure.error
        );
    }
    if let Some(failure) = &outcome.agent_failure {
        eprintln!("warning: the Agent could not judge this Run: {failure}");
    }
    if let Some(failure) = &outcome.travel_failure {
        eprintln!(
            "warning: the Agent's travel estimates failed; {} cities wait for the next Run: {failure}",
            outcome.unestimated
        );
    }
    println!(
        "Collected {} Candidates; {} Hackathons fit and are stored in {}.",
        outcome.collected,
        outcome.fitting.len(),
        db_path.display()
    );
    if outcome.duplicates > 0 || outcome.waiting_on_duplicates > 0 {
        println!(
            "Merged {} Duplicate listings; {} Hackathons wait for the Agent to tell whether two listings are the same.",
            outcome.duplicates, outcome.waiting_on_duplicates
        );
    }
    println!(
        "The Agent judged {} Candidates; {} wait for the next Run.",
        outcome.judged, outcome.unjudged
    );
    if args.dry_run {
        match &outcome.alert {
            Some(alert) => println!(
                "\nDry run, so nothing was sent. The Alert would be:\n\n{}",
                alert.text
            ),
            None => println!("\nDry run: nothing to send."),
        }
    } else if outcome.alert.is_none() {
        println!("\nNothing new to send.");
    } else if let Some(AlertChannelSetting::Ntfy(_)) = &profile.alert_channel {
        println!("\nSent the Alert through ntfy.");
    } else if let Some(AlertChannelSetting::Photon(_)) = &profile.alert_channel {
        println!("\nSent the Alert by iMessage through Photon.");
    }

    // Every Source failing means the Run learnt nothing, which a scheduler should see.
    let all_failed = !sources.is_empty() && outcome.failed_sources.len() == sources.len();
    // So does an Agent that couldn't judge; its Candidates are kept for the next Run.
    Ok(if all_failed || outcome.agent_failure.is_some() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

/// The Sources a Run reads, all fetching politely through one client so each
/// site's robots.txt is read once per Run.
fn sources(profile: &Profile, store: &Store) -> anyhow::Result<Vec<Box<dyn Source>>> {
    let http = Rc::new(PoliteHttp::new(HttpClient::new()));
    let mut sources: Vec<Box<dyn Source>> = vec![
        Box::new(HackathonBe::new(http.clone())),
        Box::new(HackathonHub::new(
            http.clone(),
            store.listings_from(hackathonhub::NAME)?,
            profile.sources.hackathonhub_pages_per_run,
            SystemClock.today(),
        )),
    ];
    for url in luma::CALENDARS {
        sources.push(Box::new(LumaCalendar::new(http.clone(), *url)));
    }
    sources.push(Box::new(Devpost::new(http)));
    // The person's own calendars are fetched for them, like a calendar app would.
    let own = Rc::new(PoliteHttp::new(HttpClient::new()).ignoring_robots());
    for url in &profile.sources.ics {
        sources.push(Box::new(IcsCalendar::new(own.clone(), url.clone())));
    }
    Ok(sources)
}

/// The Alert channel the Profile names, or printing when it names none.
fn alert_channel(profile: &Profile, dry_run: bool) -> anyhow::Result<Box<dyn AlertChannel>> {
    Ok(match &profile.alert_channel {
        Some(AlertChannelSetting::Ntfy(setting)) => Box::new(NtfyChannel::new(
            setting.clone(),
            ntfy::token_from_env(),
            HttpClient::new(),
        )),
        Some(AlertChannelSetting::Photon(setting)) => {
            let credentials = photon::Credentials::from_env().map_err(|e| e.to_string());
            // Checked before the Run, so missing credentials don't waste the Agent's work.
            if let (false, Err(why)) = (dry_run, &credentials) {
                anyhow::bail!("{why}");
            }
            Box::new(PhotonChannel::new(
                setting.clone(),
                credentials,
                photon::Sidecar::locate(),
                SystemRunner,
            ))
        }
        None => Box::new(PrintChannel),
    })
}

#[derive(clap::Args)]
struct ScheduleArgs {
    #[command(subcommand)]
    action: ScheduleAction,
}

#[derive(Subcommand)]
enum ScheduleAction {
    /// Install the recurring Run from the Profile's schedule, or update it after a change.
    Install(ScheduleInstallArgs),
    /// Remove the recurring Run.
    Uninstall,
    /// Say whether the recurring Run is installed.
    Status,
}

#[derive(clap::Args)]
struct ScheduleInstallArgs {
    /// The Profile file each Run reads [default: ~/.config/hackathon-radar/profile.toml]
    #[arg(long, value_name = "PATH")]
    profile: Option<PathBuf>,

    /// The database file each Run uses [default: ~/.local/share/hackathon-radar/hackathon-radar.sqlite]
    #[arg(long, value_name = "PATH")]
    db: Option<PathBuf>,
}

fn schedule_command(args: ScheduleArgs) -> anyhow::Result<ExitCode> {
    use hackathon_radar::scheduler::{self, Platform, ScheduledRun, SystemCommandRunner};

    let mut runner = SystemCommandRunner;
    let platform = Platform::detect(&mut runner)?;
    let log = paths::default_log_path()?;
    match args.action {
        ScheduleAction::Install(args) => {
            // launchd and systemd start Runs from `/`, so every path must be absolute.
            let profile_path = std::path::absolute(match args.profile {
                Some(path) => path,
                None => paths::default_profile_path()?,
            })?;
            let db_path = std::path::absolute(match args.db {
                Some(path) => path,
                None => paths::default_database_path()?,
            })?;
            let profile = Profile::load(&profile_path)?;
            let binary =
                std::env::current_exe().context("could not find the path of this binary")?;
            let run_path = scheduler::RunPath::from_env(std::env::var_os("PATH"));
            let job = ScheduledRun {
                binary,
                profile: profile_path,
                db: db_path,
                log,
                schedule: profile.schedule,
                path: run_path.value.clone(),
            };
            let installed = scheduler::install(&platform, &job, &mut runner)?;
            println!(
                "{} the schedule: a Run {} (local time).",
                if installed.replaced {
                    "Updated"
                } else {
                    "Installed"
                },
                job.schedule
            );
            for file in &installed.files {
                println!("  {}", file.display());
            }
            println!("Each Run appends its output to {}.", job.log.display());
            if run_path.defaulted {
                println!(
                    "PATH is not set in this shell, so Runs use {}.",
                    run_path.value
                );
            } else {
                println!("Runs use this shell's PATH, so they find the same tools you do.");
            }
            println!(
                "If you install or move your Agent's CLI (claude, codex) later, run \
                 `hackathon-radar schedule install` again to refresh the PATH."
            );
            if matches!(platform, Platform::Systemd { .. }) {
                println!(
                    "systemd only runs user timers while you are logged in; to keep Runs going \
                     when you are logged out, run `loginctl enable-linger`."
                );
            }
            if scheduler::is_cargo_build_output(&job.binary) {
                eprintln!(
                    "warning: the schedule runs {}, a Cargo build output that `cargo clean` \
                     removes; install the binary (`cargo install --path .`) and run \
                     `hackathon-radar schedule install` from it.",
                    job.binary.display()
                );
            }
        }
        ScheduleAction::Uninstall => match scheduler::uninstall(&platform, &mut runner)? {
            scheduler::Uninstalled::Removed { files } => {
                println!("Removed the schedule:");
                for file in &files {
                    println!("  {}", file.display());
                }
            }
            scheduler::Uninstalled::NotInstalled => {
                println!("No schedule is installed, so there was nothing to remove.");
            }
        },
        ScheduleAction::Status => {
            let status = scheduler::status(&platform, &mut runner)?;
            println!("{}", status_message(status.installed, status.active));
            for file in &status.files {
                println!("  {}", file.display());
            }
            println!("Scheduled Runs append their output to {}.", log.display());
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn status_message(installed: bool, active: bool) -> &'static str {
    match (installed, active) {
        (true, true) => "The schedule is installed and active.",
        (true, false) => {
            "The schedule is installed but not active; run `hackathon-radar schedule install` to reload it."
        }
        (false, true) => {
            "The schedule is active but its files are missing; run `hackathon-radar schedule install` to repair it."
        }
        (false, false) => {
            "No schedule is installed; run `hackathon-radar schedule install` to add one."
        }
    }
}
