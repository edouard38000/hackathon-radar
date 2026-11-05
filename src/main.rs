use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Context;
use clap::{Parser, Subcommand};

use hackathon_radar::alert::ntfy::{self, NtfyChannel};
use hackathon_radar::alert::{Alert, AlertChannel};
use hackathon_radar::clock::SystemClock;
use hackathon_radar::http::HttpClient;
use hackathon_radar::profile::{AlertChannelSetting, Profile};
use hackathon_radar::run::{self, RunDeps, RunOptions};
use hackathon_radar::sources::Source;
use hackathon_radar::sources::hackathon_be::HackathonBe;
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
    let sources: Vec<Box<dyn Source>> = vec![Box::new(HackathonBe::new(HttpClient::new()))];
    let mut channel = alert_channel(&profile);

    let outcome = run::run(
        RunDeps {
            profile: &profile,
            sources: &sources,
            clock: &SystemClock,
            store: &mut store,
            channel: channel.as_mut(),
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
    println!(
        "Collected {} Candidates; {} Hackathons fit and are stored in {}.",
        outcome.collected,
        outcome.fitting.len(),
        db_path.display()
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
    }

    // Every Source failing means the Run learnt nothing, which a scheduler should see.
    let all_failed = !sources.is_empty() && outcome.failed_sources.len() == sources.len();
    Ok(if all_failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

/// The Alert channel the Profile names, or printing when it names none.
fn alert_channel(profile: &Profile) -> Box<dyn AlertChannel> {
    match &profile.alert_channel {
        Some(AlertChannelSetting::Ntfy(setting)) => Box::new(NtfyChannel::new(
            setting.clone(),
            ntfy::token_from_env(),
            HttpClient::new(),
        )),
        None => Box::new(PrintChannel),
    }
}

/// Stands in for an Alert channel when the Profile has none: it prints and sends nothing.
/// What it prints counts as announced, since the person running it has read it.
struct PrintChannel;

impl AlertChannel for PrintChannel {
    fn send(&mut self, alert: &Alert) -> anyhow::Result<()> {
        println!(
            "\nThe Profile has no Alert channel, so the Alert is printed here and not sent:\n\n{}",
            alert.text
        );
        Ok(())
    }
}
