mod flakes;
mod nixpkgs;

use clap::{Parser, Subcommand};
use flakes::{Flake, FlakeCompareData};
use nixpkgs::Nixpkgs;
use serde_json::Value;
use std::{fs::File, io::Write, process::Command};

/// Small application to compare nixpkgs commits.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Compares two nixpkgs hashes and makes a report
    Nixpkgs {
        /// The base commit hash
        previous: String,
        /// The head commit hash
        next: String,
        /// Set a custom output path for the report
        #[arg(short, long, default_value = "report.md")]
        out: String,
    },

    /// Compares two versions of a flake (or different flakes) and makes a report based on it's packages (does not work on nixpkgs repo)
    Flake {
        /// The flake url pointing towards the previous revision, tag, etc.
        previous_url: String,
        /// The flake url pointing towards the next revision, tag, etc.
        next_url: String,
        /// Set a title for the report generated
        #[arg(short, long)]
        title: Option<String>,
        /// Set a custom output path for the report
        #[arg(short, long, default_value = "report.md")]
        out: String,
    },
}

fn get_flake(flake_url: &String) -> Flake {
    // Download hash data
    let out = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "nix flake show '{}' --legacy --json --quiet --all-systems",
            flake_url
        ))
        .output()
        .expect(format!("Failed to execute nix flake show for flake: {}", flake_url).as_str());

    if !out.status.success() {
        eprintln!("Flake Download Error:");
        eprintln!("{}", String::from_utf8_lossy(&out.stderr));
        std::process::exit(1);
    }

    // Proccess into packages type
    let full_json: Value =
        serde_json::from_str(String::from_utf8_lossy(&out.stdout).to_string().as_str())
            .expect(format!("Unable to parse flake's json data : {}", flake_url).as_str());

    Flake::new(&full_json)
}

fn get_nixpkgs(base_hash: &String, head_hash: &String) -> Nixpkgs {
    // Download hash data
    let out = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "gh api repos/NixOS/nixpkgs/compare/{}...{}",
            base_hash, head_hash
        ))
        .output()
        .expect(format!("Failed to execute gh api call for [{}...{}]. Please check the hashes and if you are authenticated for gn.", base_hash, head_hash).as_str());

    if !out.status.success() {
        eprintln!("Nix Commits Download Error:");
        eprintln!("{}", String::from_utf8_lossy(&out.stderr));
        std::process::exit(1);
    }

    // Proccess into json
    let full_json: Value =
        serde_json::from_str(String::from_utf8_lossy(&out.stdout).to_string().as_str()).expect(
            format!(
                "Unable to parse Github API's json data for [{}...{}]",
                base_hash, head_hash
            )
            .as_str(),
        );

    let commits: Vec<String> = full_json
        .get("commits")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|commit| {
            commit
                .get("commit")
                .unwrap()
                .get("message")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();

    Nixpkgs::new(&commits)
}

fn main() {
    // Parse args
    let args = Cli::parse();

    match &args.command {
        Some(Commands::Flake {
            previous_url,
            next_url,
            title,
            out,
        }) => {
            // Grab commit data
            println!("Downloading and parsing packages based on hashes...");
            let prev_packages = get_flake(previous_url);
            let next_packages = get_flake(next_url);

            // Grab compare data
            println!("Comparing flakes or flake versions...");
            let compare_data = FlakeCompareData::new(&prev_packages, &next_packages);

            // Generate report and save to report.md
            println!("Writing report...");
            let mut output = File::create(out).unwrap();
            write!(output, "{}", compare_data.generate_report(title))
                .expect(format!("Unable to write {}", out).as_str());
        }
        Some(Commands::Nixpkgs {
            previous,
            next,
            out,
        }) => {
            // Grab commit data
            println!("Downloading and parsing commits based on hashes...");
            let npkgs = get_nixpkgs(previous, next);

            println!("Writing report...");
            let mut output = File::create(out).unwrap();
            write!(output, "{}", npkgs.generate_report(previous, next))
                .expect(format!("Unable to write {}", out).as_str());
        }
        _ => (),
    }
}
