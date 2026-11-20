//! Keeps the README's generated blocks in step with the binary, so CI
//! catches a stale block without mdsh installed.
//!
//! mdsh fills each block that follows a `<!-- `$ cargo run --quiet -- <args>` -->`
//! line with that command's output. This test runs the built binary with the
//! same arguments and fails when the block differs; run `mdsh` to update it.

use std::process::Command;

const BINARY: &str = env!("CARGO_BIN_EXE_hackathon-radar");
const README: &str = include_str!("../README.md");
const MARKER_START: &str = "<!-- `$ cargo run --quiet -- ";
const MARKER_END: &str = "` -->";

/// Each generated block: the binary's arguments and the block's text.
fn generated_blocks(readme: &str) -> Vec<(Vec<String>, String)> {
    let lines: Vec<&str> = readme.lines().collect();
    let mut blocks = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some(args) = line
            .strip_prefix(MARKER_START)
            .and_then(|rest| rest.strip_suffix(MARKER_END))
        else {
            continue;
        };
        assert_eq!(
            lines.get(i + 1).copied(),
            Some("```"),
            "the mdsh block for `{args}` must start right after its marker"
        );
        let body: Vec<&str> = lines[i + 2..]
            .iter()
            .take_while(|l| **l != "```")
            .copied()
            .collect();
        assert!(
            i + 2 + body.len() < lines.len(),
            "the mdsh block for `{args}` is never closed"
        );
        let args = args.split_whitespace().map(str::to_owned).collect();
        blocks.push((args, body.join("\n")));
    }
    blocks
}

#[test]
fn the_readme_has_the_help_block() {
    let blocks = generated_blocks(README);
    assert!(
        blocks.iter().any(|(args, _)| args == &["--help"]),
        "the README should carry the generated `--help` block"
    );
}

#[test]
fn the_readme_generated_blocks_match_the_binary() {
    for (args, block) in generated_blocks(README) {
        let output = Command::new(BINARY)
            .args(&args)
            .output()
            .expect("the binary runs");
        assert!(output.status.success(), "`{args:?}` failed");
        let actual = String::from_utf8(output.stdout).expect("UTF-8 output");
        assert_eq!(
            block,
            actual.trim_end(),
            "README.md's block for `hackathon-radar {}` is out of date; run `mdsh` to update it",
            args.join(" ")
        );
    }
}

#[test]
fn a_block_is_read_up_to_its_closing_fence() {
    let readme = "x\n<!-- `$ cargo run --quiet -- doctor --help` -->\n```\na\n\nb\n```\nafter\n";
    assert_eq!(
        generated_blocks(readme),
        vec![(
            vec!["doctor".to_owned(), "--help".to_owned()],
            "a\n\nb".to_owned()
        )]
    );
}
