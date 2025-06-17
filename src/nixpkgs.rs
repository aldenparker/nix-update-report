// Structs used for processing nix commit data

use std::collections::HashSet;

use regex::Regex;

/// Holds the data for a single nix commit
#[derive(PartialEq, Eq, Clone, Debug)]
enum NixpkgsCommit {
    /// Package with this name was added
    Add(String),
    /// Package with this name was removed
    Remove(String),
    /// Package with this name was updated
    Update(String, String),
    /// Could not parse commit message
    Unparsable(String),
}

impl NixpkgsCommit {
    fn new(commit_message: &String) -> NixpkgsCommit {
        let regex_str = Regex::new(
            r"^(?:\[.+\] )?(?<name>\S+): (?<action>drop|init|(?:[A-Za-z0-9-.]+ -> [A-Za-z0-9-.]+))",
        )
        .unwrap();

        let captures = regex_str.captures(commit_message.as_str());

        if let Some(caps) = captures {
            let name: String = caps.name("name").map(|m| m.as_str().into()).unwrap();
            let action: String = caps.name("action").map(|m| m.as_str().into()).unwrap();

            match action.as_str() {
                "init" => {
                    return NixpkgsCommit::Add(name);
                }
                "drop" => return NixpkgsCommit::Remove(name),
                _ => return NixpkgsCommit::Update(name, action),
            }
        }

        NixpkgsCommit::Unparsable(commit_message.clone())
    }
}

/// A struct used to generate a report about a nixpkgs diff
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Nixpkgs(Vec<NixpkgsCommit>);

impl Nixpkgs {
    pub fn new(commits: &Vec<String>) -> Nixpkgs {
        Nixpkgs(commits.iter().map(|val| NixpkgsCommit::new(val)).collect())
    }

    pub fn generate_report(&self, base_hash: &String, head_hash: &String) -> String {
        // Turn commits into hash sets TODO: find out why multiple appear
        let added: HashSet<String> = self
            .0
            .iter()
            .filter_map(|val| match val {
                NixpkgsCommit::Add(name) => Some(format!(" - {}\n", name)),
                _ => None,
            })
            .collect();

        let mut updated: Vec<String> = self
            .0
            .iter()
            .filter_map(|val| match val {
                NixpkgsCommit::Update(name, version) => Some(format!(" - {}: {}\n", name, version)),
                _ => None,
            })
            .collect::<HashSet<String>>()
            .iter()
            .map(|val| val.clone())
            .collect();
        updated.sort();

        let removed: HashSet<String> = self
            .0
            .iter()
            .filter_map(|val| match val {
                NixpkgsCommit::Remove(name) => Some(format!(" - {}\n", name)),
                _ => None,
            })
            .collect();

        let mut report = format!(
            "## nix-update-report - nixpkgs\n\
            Hash: `{} -> {}`\n\
            Report generated using [`nix-update-report`](https://github.com/aldenparker/nix-update-report.git).\n\
            \n\
            ### Stats\n\
            Pkgs Added: {}\n\
            Pkg Updates: {}\n\
            Pkgs Removed: {}\n\
            \n\
            ",
            base_hash,
            head_hash,
            added.len(),
            updated.len(),
            removed.len()
        );

        let pkg_changes: String = format!(
            "### Added\n\
            {}\n\
            ### Updated\n\
            {}\n\
            ### Removed\n\
            {}\n\
            ",
            added.iter().fold("".into(), |mut acc: String, val| {
                acc.push_str(&val);
                acc
            }),
            updated.iter().fold("".into(), |mut acc: String, val| {
                acc.push_str(&val);
                acc
            }),
            removed.iter().fold("".into(), |mut acc: String, val| {
                acc.push_str(&val);
                acc
            })
        );

        report.push_str(&pkg_changes);
        report
    }
}
