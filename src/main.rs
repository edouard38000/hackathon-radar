use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::rc::Rc;

use anyhow::Context;
use clap::{Parser, Subcommand};

use hackathon_radar::agent;
use hackathon_radar::alert::photon::Sidecar;
use hackathon_radar::clock::{Clock, SystemClock};
use hackathon_radar::export;
use hackathon_radar::fares::FareLookup;
use hackathon_radar::http::HttpClient;
use hackathon_radar::init::{self, InitEnv, InitFlags, StdTerminal};
use hackathon_radar::process::SystemRunner;
use hackathon_radar::profile::{AlertChannelSetting, Profile};
use hackathon_radar::report::{self, Report, Scope};
use hackathon_radar::run::{self, RunDeps, RunOptions};
use hackathon_radar::secrets::Secrets;
use hackathon_radar::sources::Source;
use hackathon_radar::sources::hackathonhub;
use hackathon_radar::sources::packs;
use hackathon_radar::sources::polite::PoliteHttp;
use hackathon_radar::wiring;
use hackathon_radar::{paths, store::Store};

#[derive(Parser)]
#[command(name = "hackathon-radar", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Set up hackathon-radar, or change your Profile: Home, Countries, budget, Agent, keys and Alert channel.
    Init(Box<InitArgs>),
    /// Collect Hackathons from the Sources, store those that Fit, and compose the Alert.
    Run(RunArgs),
    /// Send one short test Alert through the Profile's Alert channel.
    TestAlert(TestAlertArgs),
    /// Install, remove or check the recurring Run (launchd on macOS, systemd on Linux).
    Schedule(ScheduleArgs),
    /// Write a Markdown (or HTML) report of the Hackathons that Fit, sorted by date.
    Report(ReportArgs),
    /// Export the deadlines as a calendar file, or the Hackathons as a CSV for Google Sheets.
    Export(ExportArgs),
    /// Import the personal tracker: a CSV export of its Sheet's Hackathons tab.
    ImportSheet(ImportSheetArgs),
    /// Check everything a Run depends on, and say how to fix what's wrong. Exits 1 if a check fails.
    Doctor(DoctorArgs),
}

#[derive(clap::Args)]
struct DoctorArgs {
    /// Print the results as JSON (for the setup skill) instead of text.
    #[arg(long)]
    json: bool,

    /// Skip the checks that use the network: the ntfy server and the Sources.
    #[arg(long, conflicts_with = "send_test_alert")]
    offline: bool,

    /// Also send one test Alert through the Profile's Alert channel.
    #[arg(long)]
    send_test_alert: bool,

    /// The Profile file [default: ~/.config/hackathon-radar/profile.toml]
    #[arg(long, value_name = "PATH")]
    profile: Option<PathBuf>,

    /// The database file [default: ~/.local/share/hackathon-radar/hackathon-radar.sqlite]
    #[arg(long, value_name = "PATH")]
    db: Option<PathBuf>,
}

#[derive(clap::Args)]
struct InitArgs {
    /// The Profile file to write or edit [default: ~/.config/hackathon-radar/profile.toml]
    #[arg(long, value_name = "PATH")]
    profile: Option<PathBuf>,

    /// The database file scheduled Runs use [default: ~/.local/share/hackathon-radar/hackathon-radar.sqlite]
    #[arg(long, value_name = "PATH")]
    db: Option<PathBuf>,

    #[command(flatten)]
    flags: InitFlags,
}

#[derive(clap::Args)]
struct TestAlertArgs {
    /// The Profile file [default: ~/.config/hackathon-radar/profile.toml]
    #[arg(long, value_name = "PATH")]
    profile: Option<PathBuf>,
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
        Command::Init(args) => init_command(*args),
        Command::Run(args) => run_command(args),
        Command::TestAlert(args) => test_alert_command(args),
        Command::Schedule(args) => schedule_command(args),
        Command::Report(args) => report_command(args),
        Command::Export(args) => export_command(args),
        Command::ImportSheet(args) => import_sheet_command(args),
        Command::Doctor(args) => doctor_command(args),
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
    // Secrets come from the keychain (or its owner-only file), so scheduled
    // Runs have them without the person's shell environment.
    let secrets = Secrets::system()?;
    let mut channel = wiring::alert_channel(
        &profile,
        &secrets,
        args.dry_run,
        HttpClient::new(),
        SystemRunner,
        Sidecar::locate(),
    )?;
    let agent = agent::from_settings(&profile.agent);
    if let Some(warning) = agent.warning() {
        eprintln!("warning: {warning}");
    }
    // Fares on the person's own SerpApi key, if set; without it, the Agent
    // estimates Unverified fares. The key is never printed.
    let serpapi = wiring::fare_lookup(
        &secrets,
        PoliteHttp::new(HttpClient::new()).ignoring_robots(),
    )?;

    let outcome = run::run(
        RunDeps {
            profile: &profile,
            sources: &sources,
            clock: &SystemClock,
            store: &mut store,
            channel: channel.as_mut(),
            agent: agent.as_ref(),
            fares: serpapi.as_ref().map(|s| s as &dyn FareLookup),
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
    if let Some(failure) = &outcome.recheck_failure {
        eprintln!(
            "warning: the Agent could not recheck deadlines and status; they are rechecked on the next Run: {failure}"
        );
    }
    if let Some(failure) = &outcome.alert_failure {
        eprintln!(
            "warning: the Alert channel could not send the Alert; it is tried again on the next Run: {failure}"
        );
    }
    if let Some(failure) = &outcome.fare_failure {
        eprintln!("warning: Fare lookups failed; they are tried again on the next Run: {failure}");
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
    if profile.travel.flight_budget.is_some() {
        println!(
            "{} {} Fares; {} flights are over the Flight budget and {} wait for a Fare.",
            if serpapi.is_some() {
                "Looked up"
            } else {
                "Without a SerpApi key, the Agent estimated"
            },
            outcome.fares_looked_up,
            outcome.over_budget,
            outcome.waiting_on_fares
        );
    }
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
    } else if !outcome.sent {
        // The Alert channel failed; reported on stderr above.
    } else if let Some(AlertChannelSetting::Ntfy(_)) = &profile.alert_channel {
        println!("\nSent the Alert through ntfy.");
    } else if let Some(AlertChannelSetting::Photon(_)) = &profile.alert_channel {
        println!("\nSent the Alert by iMessage through Photon.");
    }
    if outcome.needs_attention && outcome.sent {
        println!("The Alert told you hackathon-radar needs attention.");
    }

    // A failed Run (see hackathon_radar::run::failure) exits non-zero with one
    // line saying why, for the schedule's log; nothing it collected is lost.
    Ok(match &outcome.failure {
        Some(failure) => {
            eprintln!("error: the Run failed: {failure}");
            ExitCode::FAILURE
        }
        None => ExitCode::SUCCESS,
    })
}

/// The Sources a Run reads: the global ones, the Source packs (from Home and
/// the Profile) and the person's own calendars, all fetching politely through
/// one client so each site's robots.txt is read once per Run.
fn sources(profile: &Profile, store: &Store) -> anyhow::Result<Vec<Box<dyn Source>>> {
    let plan = packs::plan(profile);
    Ok(packs::build(
        &plan,
        packs::Fetching {
            http: Rc::new(PoliteHttp::new(HttpClient::new())),
            // The person's own calendars are fetched for them, like a calendar app would.
            own_calendars: Rc::new(PoliteHttp::new(HttpClient::new()).ignoring_robots()),
            today: SystemClock.today(),
            hackathonhub_known: store.listings_from(hackathonhub::NAME)?,
            hackathonhub_pages_per_run: profile.sources.hackathonhub_pages_per_run,
        },
    ))
}

fn init_command(args: InitArgs) -> anyhow::Result<ExitCode> {
    let profile_path = match args.profile {
        Some(path) => path,
        None => paths::default_profile_path()?,
    };
    let secrets = Secrets::system()?;
    let path_var = std::env::var_os("PATH");
    let find_program = |name: &str| init::find_on_path(name, path_var.as_deref());
    let db = args.db;
    let mut install_schedule = |profile: &Path| schedule_install(Some(profile.into()), db.clone());
    let mut terminal = StdTerminal::new();
    let outcome = init::init(
        &profile_path,
        &args.flags,
        InitEnv {
            terminal: &mut terminal,
            secrets: &secrets,
            find_program: &find_program,
            runner: &SystemRunner,
            http: &HttpClient::new(),
            sidecar: Sidecar::locate(),
            install_schedule: &mut install_schedule,
        },
    )?;
    Ok(if outcome.succeeded() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn test_alert_command(args: TestAlertArgs) -> anyhow::Result<ExitCode> {
    let profile_path = match args.profile {
        Some(path) => path,
        None => paths::default_profile_path()?,
    };
    let profile = Profile::load(&profile_path)?;
    let secrets = Secrets::system()?;
    let name = wiring::channel_name(&profile);
    let mut channel = wiring::alert_channel(
        &profile,
        &secrets,
        false,
        HttpClient::new(),
        SystemRunner,
        Sidecar::locate(),
    )?;
    wiring::send_test_alert(channel.as_mut())
        .with_context(|| format!("the test Alert through {name} was not sent"))?;
    println!("Sent a test Alert through {name}. Check your phone.");
    Ok(ExitCode::SUCCESS)
}

fn doctor_command(args: DoctorArgs) -> anyhow::Result<ExitCode> {
    use hackathon_radar::doctor::{self, DoctorEnv, DoctorOptions};
    use hackathon_radar::scheduler::{Platform, SystemCommandRunner};
    use std::time::Duration;

    let profile = match args.profile {
        Some(path) => path,
        None => paths::default_profile_path()?,
    };
    let db = match args.db {
        Some(path) => path,
        None => paths::default_database_path()?,
    };
    let options = DoctorOptions {
        profile,
        db: db.clone(),
        log: paths::default_log_path()?,
        offline: args.offline,
        send_test_alert: args.send_test_alert,
    };
    let secrets = Secrets::system()?;
    let env = |name: &str| std::env::var(name).ok();
    let mut scheduler = SystemCommandRunner;
    let platform = Platform::detect(&mut scheduler).map_err(|e| format!("{e:#}"));
    // Quick checks: a server or Source that takes longer is reported as unreachable.
    let quick = || HttpClient::with_timeout(Duration::from_secs(15));
    let http = quick();
    let sources_http = PoliteHttp::new(quick());
    let calendars_http = PoliteHttp::new(quick()).ignoring_robots();
    let http_post = HttpClient::new();
    // A database that doesn't exist yet has no Runs, and isn't created.
    let last_run = || {
        if db.exists() {
            Store::open(&db)?.last_run()
        } else {
            Ok(None)
        }
    };
    let report = doctor::doctor(
        &options,
        DoctorEnv {
            secrets: &secrets,
            env: &env,
            find_program: &doctor::which,
            runner: &SystemRunner,
            scheduler: &mut scheduler,
            platform,
            binary: std::env::current_exe().unwrap_or_default(),
            http: &http,
            sources_http: &sources_http,
            calendars_http: &calendars_http,
            http_post: &http_post,
            sidecar: Sidecar::locate(),
            now: chrono::Utc::now(),
            last_run: &last_run,
        },
    );
    print!(
        "{}",
        if args.json {
            report.json()
        } else {
            report.human()
        }
    );
    Ok(ExitCode::from(report.exit_code()))
}

#[derive(clap::Args)]
struct ImportSheetArgs {
    /// The Hackathons tab exported as CSV (File > Download > Comma-separated values).
    file: PathBuf,

    /// Show what would be imported, and write nothing.
    #[arg(long)]
    dry_run: bool,

    /// The Profile file [default: ~/.config/hackathon-radar/profile.toml]
    #[arg(long, value_name = "PATH")]
    profile: Option<PathBuf>,

    /// The database file [default: ~/.local/share/hackathon-radar/hackathon-radar.sqlite]
    #[arg(long, value_name = "PATH")]
    db: Option<PathBuf>,
}

fn import_sheet_command(args: ImportSheetArgs) -> anyhow::Result<ExitCode> {
    use hackathon_radar::sheet_import;

    let profile_path = match args.profile {
        Some(path) => path,
        None => paths::default_profile_path()?,
    };
    let db_path = match args.db {
        Some(path) => path,
        None => paths::default_database_path()?,
    };
    let profile = Profile::load(&profile_path)?;
    let csv = std::fs::read_to_string(&args.file)
        .with_context(|| format!("reading {}", args.file.display()))?;
    // A dry run doesn't create the database either.
    let mut store = if args.dry_run && !db_path.exists() {
        Store::open_in_memory()?
    } else {
        Store::open(&db_path)?
    };
    let report = sheet_import::import(
        &mut store,
        &profile,
        &csv,
        SystemClock.today(),
        args.dry_run,
    )?;

    println!("Read {} rows from {}.", report.rows, args.file.display());
    println!(
        "{} {} new Hackathons; {} rows merged into Hackathons already stored; {} rows skipped.",
        if args.dry_run {
            "Would import"
        } else {
            "Imported"
        },
        report.imported,
        report.merged,
        report.skipped.len()
    );
    let mut reasons: Vec<(String, usize)> = Vec::new();
    for skipped in &report.skipped {
        let kind = match &skipped.why {
            sheet_import::SkipReason::Unreadable(_) => "unreadable",
            sheet_import::SkipReason::AlreadyImported => "already imported",
            sheet_import::SkipReason::SameAsRow(_) => "repeated rows",
        };
        match reasons.iter_mut().find(|(k, _)| k == kind) {
            Some((_, count)) => *count += 1,
            None => reasons.push((kind.to_owned(), 1)),
        }
    }
    if !reasons.is_empty() {
        let reasons: Vec<String> = reasons.iter().map(|(k, n)| format!("{n} {k}")).collect();
        println!("Skipped: {}.", reasons.join(", "));
    }
    for skipped in &report.skipped {
        if skipped.why == sheet_import::SkipReason::AlreadyImported {
            continue;
        }
        match &skipped.name {
            Some(name) => println!("  row {} ({name}): {}", skipped.row, skipped.why),
            None => println!("  row {}: {}", skipped.row, skipped.why),
        }
    }
    if report.announced > 0 {
        println!(
            "{} rows were already notified in the Sheet, so they are Announced and won't be announced again.",
            report.announced
        );
    }
    for warning in &report.warnings {
        eprintln!("warning: {warning}");
    }
    if args.dry_run {
        println!("Dry run: nothing was written to {}.", db_path.display());
    } else if report.imported + report.merged == 0 {
        println!("Nothing new to store in {}.", db_path.display());
    } else {
        println!("Stored in {}.", db_path.display());
    }
    Ok(ExitCode::SUCCESS)
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
    use hackathon_radar::scheduler::{self, Platform, SystemCommandRunner};

    let mut runner = SystemCommandRunner;
    let platform = Platform::detect(&mut runner)?;
    let log = paths::default_log_path()?;
    match args.action {
        ScheduleAction::Install(args) => schedule_install(args.profile, args.db)?,
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

#[derive(clap::Args)]
struct ReportArgs {
    /// Write HTML instead of Markdown.
    #[arg(long)]
    html: bool,

    #[command(flatten)]
    output: OutputArgs,
}

#[derive(clap::Args)]
struct ExportArgs {
    #[command(subcommand)]
    what: ExportWhat,
}

#[derive(Subcommand)]
enum ExportWhat {
    /// An .ics calendar with an all-day "Apply" event on each open deadline.
    Ics(OutputArgs),
    /// A CSV laid out like the personal tracker's Sheet, to import into Google Sheets.
    Sheet(OutputArgs),
}

/// What reports and exports share: what to include, where to read, where to write.
#[derive(clap::Args)]
struct OutputArgs {
    /// Include Hackathons that don't Fit or are closed.
    #[arg(long)]
    all: bool,

    /// Write to this file instead of stdout.
    #[arg(long, value_name = "FILE")]
    out: Option<PathBuf>,

    /// The Profile file [default: ~/.config/hackathon-radar/profile.toml]
    #[arg(long, value_name = "PATH")]
    profile: Option<PathBuf>,

    /// The database file [default: ~/.local/share/hackathon-radar/hackathon-radar.sqlite]
    #[arg(long, value_name = "PATH")]
    db: Option<PathBuf>,
}

impl OutputArgs {
    fn scope(&self) -> Scope {
        if self.all { Scope::All } else { Scope::Fitting }
    }

    /// The Profile and the store. A database that doesn't exist yet reads as
    /// empty, and isn't created.
    fn open(&self) -> anyhow::Result<(Profile, Store)> {
        let profile_path = match &self.profile {
            Some(path) => path.clone(),
            None => paths::default_profile_path()?,
        };
        let db_path = match &self.db {
            Some(path) => path.clone(),
            None => paths::default_database_path()?,
        };
        let profile = Profile::load(&profile_path)?;
        let store = if db_path.exists() {
            Store::open(&db_path)?
        } else {
            Store::open_in_memory()?
        };
        Ok((profile, store))
    }

    /// Writes `text` to the file, or to stdout, and says what was written on stderr.
    fn write(&self, text: &str, summary: &str) -> anyhow::Result<()> {
        match &self.out {
            Some(path) => {
                std::fs::write(path, text)
                    .with_context(|| format!("writing {}", path.display()))?;
                eprintln!("{summary} Written to {}.", path.display());
            }
            None => {
                use std::io::Write;
                std::io::stdout().write_all(text.as_bytes())?;
                eprintln!("{summary}");
            }
        }
        Ok(())
    }
}

fn report_command(args: ReportArgs) -> anyhow::Result<ExitCode> {
    let (profile, store) = args.output.open()?;
    let report = Report::build(&store, &profile, SystemClock.today(), args.output.scope())?;
    let text = if args.html {
        report.html()
    } else {
        report.markdown()
    };
    args.output.write(&text, &report.summary_line())?;
    Ok(ExitCode::SUCCESS)
}

fn export_command(args: ExportArgs) -> anyhow::Result<ExitCode> {
    let clock = SystemClock;
    let today = clock.today();
    match args.what {
        ExportWhat::Ics(output) => {
            let (profile, store) = output.open()?;
            let listed = report::collect(&store, &profile, today, output.scope())?;
            let calendar = export::ics::calendar(&listed, &profile, clock.now());
            output.write(&calendar.text, &calendar.summary())?;
        }
        ExportWhat::Sheet(output) => {
            let (profile, store) = output.open()?;
            let listed = report::collect(&store, &profile, today, output.scope())?;
            let sheet = export::sheet::sheet(&listed, &profile, today)?;
            output.write(&sheet.text, &sheet.summary())?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// `schedule install`, which `init` also runs: installs the recurring Run, or
/// updates it.
fn schedule_install(profile: Option<PathBuf>, db: Option<PathBuf>) -> anyhow::Result<()> {
    use hackathon_radar::scheduler::{self, Platform, ScheduledRun, SystemCommandRunner};

    let mut runner = SystemCommandRunner;
    let platform = Platform::detect(&mut runner)?;
    let log = paths::default_log_path()?;
    // launchd and systemd start Runs from `/`, so every path must be absolute.
    let profile_path = std::path::absolute(match profile {
        Some(path) => path,
        None => paths::default_profile_path()?,
    })?;
    let db_path = std::path::absolute(match db {
        Some(path) => path,
        None => paths::default_database_path()?,
    })?;
    let profile = Profile::load(&profile_path)?;
    let binary = std::env::current_exe().context("could not find the path of this binary")?;
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
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_command_line_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn doctor_cannot_send_a_test_alert_offline() {
        let parse = |args: &[&str]| {
            Cli::try_parse_from(["hackathon-radar", "doctor"].iter().chain(args)).map(|_| ())
        };
        assert!(parse(&["--json", "--offline"]).is_ok());
        assert!(parse(&["--send-test-alert"]).is_ok());
        assert!(parse(&["--offline", "--send-test-alert"]).is_err());
    }
}
