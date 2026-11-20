//! Parse the release workflow without installing actionlint. This checks the
//! distribution contract; building each target still needs its runner (GitHub
//! hosted for Linux, the maintainer's self-hosted Mac for macOS).

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
    // The macOS builds run on a self-hosted Mac: a pull request, from a fork
    // or not, must never be able to start this workflow.
    assert!(
        !workflow["on"]
            .as_mapping()
            .unwrap()
            .contains_key("pull_request")
    );
    assert!(
        !workflow["on"]
            .as_mapping()
            .unwrap()
            .contains_key("pull_request_target")
    );
    assert_eq!(workflow["permissions"]["contents"].as_str(), Some("read"));

    let jobs = workflow["jobs"].as_mapping().unwrap();
    // A runner is either one hosted label or a list of self-hosted labels;
    // compare both as label lists.
    let targets: Vec<(&str, Vec<&str>)> =
        workflow["jobs"]["build"]["strategy"]["matrix"]["include"]
            .as_sequence()
            .unwrap()
            .iter()
            .map(|entry| {
                let labels = match entry["os"].as_sequence() {
                    Some(labels) => labels.iter().map(|l| l.as_str().unwrap()).collect(),
                    None => vec![entry["os"].as_str().unwrap()],
                };
                (entry["target"].as_str().unwrap(), labels)
            })
            .collect();
    assert_eq!(targets.len(), 4);
    let self_hosted_mac = vec!["self-hosted", "macOS", "ARM64"];
    for target in [
        ("aarch64-apple-darwin", self_hosted_mac.clone()),
        ("x86_64-apple-darwin", self_hosted_mac.clone()),
        ("x86_64-unknown-linux-gnu", vec!["ubuntu-22.04"]),
        ("aarch64-unknown-linux-gnu", vec!["ubuntu-22.04-arm"]),
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
