//! Parse the release workflow without installing actionlint. This checks the
//! distribution contract; building each target still needs its hosted runner.
//! Also guards every workflow against self-hosted runners: the repo is public,
//! and a fork's pull request could run its own code on a self-hosted machine.

use serde_norway::Value;

#[test]
fn release_workflow_parses_and_keeps_publication_scoped_to_tags() {
    let workflow: Value =
        serde_norway::from_str(include_str!("../.github/workflows/release.yml")).unwrap();
    assert_eq!(workflow["on"]["push"]["tags"][0].as_str(), Some("v*"));
    assert!(
        workflow["on"]
            .as_mapping()
            .unwrap()
            .contains_key("workflow_dispatch")
    );
    // A pull request, from a fork or not, must never start a release build.
    for trigger in ["pull_request", "pull_request_target"] {
        assert!(
            !workflow["on"].as_mapping().unwrap().contains_key(trigger),
            "release workflow must not run on {trigger}"
        );
    }
    assert_eq!(workflow["permissions"]["contents"].as_str(), Some("read"));

    let jobs = workflow["jobs"].as_mapping().unwrap();
    let targets: Vec<(&str, &str)> = workflow["jobs"]["build"]["strategy"]["matrix"]["include"]
        .as_sequence()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry["target"].as_str().unwrap(),
                entry["os"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(targets.len(), 4);
    for target in [
        ("aarch64-apple-darwin", "macos-latest"),
        ("x86_64-apple-darwin", "macos-latest"),
        ("x86_64-unknown-linux-gnu", "ubuntu-22.04"),
        ("aarch64-unknown-linux-gnu", "ubuntu-22.04-arm"),
    ] {
        assert!(
            targets.contains(&target),
            "missing target/runner {target:?}"
        );
    }

    for (name, job) in jobs {
        if name.as_str() == Some("release") {
            assert_eq!(job["permissions"]["contents"].as_str(), Some("write"));
            // Dispatching even from a tag must remain a build-only dry run.
            let condition = job["if"].as_str().unwrap();
            assert!(condition.contains("github.event_name == 'push'"));
            assert!(condition.contains("startsWith(github.ref, 'refs/tags/v')"));
        } else {
            assert!(job["permissions"].is_null()); // Inherits contents: read.
        }
        for step in job["steps"].as_sequence().unwrap() {
            if let Some(action) = step["uses"].as_str() {
                let (_, revision) = action.split_once('@').unwrap();
                assert_eq!(revision.len(), 40, "unpinned action {action}");
                assert!(revision.chars().all(|c| c.is_ascii_hexdigit()));
            }
            if let Some(script) = step["run"].as_str() {
                assert!(!script.contains("cargo publish"));
            }
        }
    }
}

#[test]
fn release_archive_extracts_into_one_versioned_directory() {
    let workflow: Value =
        serde_norway::from_str(include_str!("../.github/workflows/release.yml")).unwrap();
    let script = workflow["jobs"]["build"]["steps"]
        .as_sequence()
        .unwrap()
        .iter()
        .find(|step| step["name"].as_str() == Some("Package binary and Photon sidecar"))
        .unwrap()["run"]
        .as_str()
        .unwrap();

    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path();
    let target = "x86_64-unknown-linux-gnu";
    let version = "9.8.7";
    let package = format!("hackathon-radar-{version}-{target}");
    let binary = format!("target/{target}/release/hackathon-radar");
    let files = [
        "README.md",
        "LICENSE-MIT",
        "LICENSE-APACHE",
        "docs/setup-guide.md",
        "sidecar/photon/.npmrc",
        "sidecar/photon/package.json",
        "sidecar/photon/package-lock.json",
        "sidecar/photon/send.mjs",
        "sidecar/photon/lib.mjs",
    ];
    for file in files.into_iter().chain([
        binary.as_str(),
        "sidecar/photon/node_modules/not-for-release.js",
    ]) {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("fixture: {file}\n")).unwrap();
    }

    // Execute the workflow's packaging step, with a fake binary and sidecar.
    // No build, network service, Agent or keychain is involved.
    let output = std::process::Command::new("bash")
        .args(["-c", script])
        .current_dir(root)
        .env("VERSION", version)
        .env("TARGET", target)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let checksum = std::process::Command::new("shasum")
        .args(["-a", "256", "-c", &format!("{package}.tar.gz.sha256")])
        .current_dir(root.join("dist"))
        .output()
        .unwrap();
    assert!(
        checksum.status.success(),
        "{}",
        String::from_utf8_lossy(&checksum.stderr)
    );

    let extracted = root.join("extracted");
    std::fs::create_dir(&extracted).unwrap();
    let output = std::process::Command::new("tar")
        .arg("-xzf")
        .arg(root.join("dist").join(format!("{package}.tar.gz")))
        .arg("-C")
        .arg(&extracted)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let entries: Vec<_> = std::fs::read_dir(&extracted)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(entries, [std::ffi::OsString::from(&package)]);
    let payload = extracted.join(&package);
    assert!(payload.is_dir());
    for file in files {
        assert_eq!(
            std::fs::read(payload.join(file)).unwrap(),
            std::fs::read(root.join(file)).unwrap(),
            "archive payload differs: {file}"
        );
    }
    assert_eq!(
        std::fs::read(payload.join("hackathon-radar")).unwrap(),
        std::fs::read(root.join(binary)).unwrap()
    );
    assert!(!payload.join("sidecar/photon/node_modules").exists());
}

#[test]
fn no_workflow_uses_a_self_hosted_runner() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let is_yaml = matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("yml" | "yaml")
        );
        if !is_yaml {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let file = path.display();
        assert!(
            !text.to_ascii_lowercase().contains("self-hosted"),
            "{file} mentions a self-hosted runner; this public repo runs on GitHub-hosted runners only"
        );
        // A self-hosted runner can also be picked without the label, by its
        // other labels or a runner group, so every job's runner must be a
        // single GitHub-hosted image name.
        let workflow: Value = serde_norway::from_str(&text).unwrap();
        for (name, job) in workflow["jobs"].as_mapping().unwrap() {
            let name = name.as_str().unwrap();
            for runner in hosted_runner_labels(job) {
                assert!(
                    ["ubuntu-", "macos-", "windows-"]
                        .iter()
                        .any(|prefix| runner.starts_with(prefix)),
                    "{file}: job {name} runs on {runner:?}, not a GitHub-hosted runner"
                );
            }
        }
        checked += 1;
    }
    assert!(
        checked >= 2,
        "expected ci.yml and release.yml in {}",
        dir.display()
    );
}

/// The runner names a job can run on, resolving `${{ matrix.<key> }}` from
/// the job's matrix. Panics on any form other than a single name (a label
/// list or a runner group), which could select a self-hosted runner.
fn hosted_runner_labels(job: &Value) -> Vec<String> {
    let runs_on = job["runs-on"]
        .as_str()
        .unwrap_or_else(|| panic!("runs-on must be one runner name, got {:?}", job["runs-on"]));
    let Some(key) = runs_on
        .strip_prefix("${{ matrix.")
        .and_then(|rest| rest.strip_suffix(" }}"))
    else {
        assert!(!runs_on.contains("${{"), "unresolvable runs-on {runs_on}");
        return vec![runs_on.to_owned()];
    };
    let matrix = &job["strategy"]["matrix"];
    let mut values: Vec<&Value> = matrix[key].as_sequence().into_iter().flatten().collect();
    values.extend(
        matrix["include"]
            .as_sequence()
            .into_iter()
            .flatten()
            .map(|entry| &entry[key])
            .filter(|value| !value.is_null()),
    );
    assert!(!values.is_empty(), "matrix.{key} has no values");
    values
        .into_iter()
        .map(|value| {
            value
                .as_str()
                .unwrap_or_else(|| panic!("matrix.{key} must be one runner name, got {value:?}"))
                .to_owned()
        })
        .collect()
}
