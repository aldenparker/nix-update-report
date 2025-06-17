// All the structs used to organize package data when using the flake command

#[path = "packages.rs"]
mod packages;

use packages::{Package, PkgCompareData};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

// --- TYPE ALIASES
type PkgMap = HashMap<String, Package>;

// --- FLAKE
/// A hash map that holds all package maps by arch from a flake
#[derive(PartialEq, Eq)]
pub struct Flake(HashMap<String, PkgMap>);

impl Flake {
    pub fn new(flake_json: &Value) -> Flake {
        let mut new_fp: HashMap<String, PkgMap> = HashMap::new();
        for (arch, pkgs) in flake_json["packages"]
            .as_object()
            .expect("Malformed json, only flake json with a packages output are acceptable")
            .iter()
        {
            if pkgs.to_string() != "{}" {
                let mut new_ps: PkgMap = PkgMap::new();
                for (_, pkg_value) in pkgs
                    .as_object()
                    .expect("Malformed json, only flake json with a packages output are acceptable")
                    .iter()
                {
                    let new_pkg: Package = Package::new(
                    &pkg_value["name"]
                        .as_str()
                        .expect(
                            "Malformed json, only flake json with a packages output are acceptable",
                        )
                        .into(),
                    &pkg_value["description"].as_str().map_or(None, |val| {
                            if val == "" {
                                return None;
                            }

                            Some(val.into())
                        })
                    );

                    new_ps.insert(new_pkg.get_name(), new_pkg);
                }

                new_fp.insert(arch.clone(), new_ps);
            }
        }

        Flake(new_fp)
    }
}

// --- FLAKE PKGS COMPARE
/// FlakePkgs comparison data for a single architecture
#[derive(PartialEq, Eq)]
struct FlakeSingleArchCompareData {
    /// All packages that were added to the flake
    added: Vec<Package>,
    /// All packages that were updated in the flake (Package is the new package and PkgCompareData holds the update info)
    updated: Vec<(Package, PkgCompareData)>,
    /// All packages that were removed from the flake
    removed: Vec<Package>,
    /// The total packages in this arch
    total_pkgs: usize,
}

/// FlakePkgs comparison data for all packages in the flake
#[derive(PartialEq, Eq)]
pub struct FlakeCompareData {
    /// All the package compare data by arch
    pkg_data: HashMap<String, FlakeSingleArchCompareData>,
    /// Archs removed from this flake
    removed_archs: Vec<String>,
    /// Archs added to this flake
    added_archs: Vec<String>,
    /// The total number of archs this flake supports
    total_archs: usize,
}

impl FlakeCompareData {
    pub fn new(old: &Flake, new: &Flake) -> FlakeCompareData {
        let mut compare_data = FlakeCompareData {
            pkg_data: HashMap::new(),
            removed_archs: vec![],
            added_archs: vec![],
            total_archs: new.0.len(), // Only count the archs in new
        };

        // First go through archs
        let comparable_archs: HashSet<String> = (old
            .0
            .keys()
            .map(|val| val.clone())
            .collect::<HashSet<String>>())
        .intersection(
            &new.0
                .keys()
                .map(|val| val.clone())
                .collect::<HashSet<String>>(),
        )
        .map(|val| val.clone())
        .collect();

        compare_data.added_archs = new
            .0
            .keys()
            .filter(|&val| !comparable_archs.contains(val))
            .map(|val| val.clone())
            .collect();

        compare_data.removed_archs = old
            .0
            .keys()
            .filter(|&val| !comparable_archs.contains(val))
            .map(|val| val.clone())
            .collect();

        // Create pkg compare values
        for arch in comparable_archs.iter() {
            let old_pkgs = old.0.get(arch).unwrap();
            let new_pkgs = new.0.get(arch).unwrap();

            let mut single_comp = FlakeSingleArchCompareData {
                added: vec![],
                updated: vec![],
                removed: vec![],
                total_pkgs: new_pkgs.len(), // only includes new packages since those are what is left
            };

            // Find updated and removed packages
            for (name, old_pkg) in old_pkgs {
                if let Some(new_pkg) = new_pkgs.get(name) {
                    match PkgCompareData::new(old_pkg, new_pkg).unwrap() {
                        PkgCompareData::Unchanged => (),
                        val => single_comp.updated.push((new_pkg.clone(), val)),
                    }
                } else {
                    single_comp.removed.push(old_pkg.clone());
                }
            }

            // Find new packages
            for (name, pkg) in new_pkgs {
                if !old_pkgs.contains_key(name) {
                    single_comp.added.push(pkg.clone());
                }
            }

            compare_data.pkg_data.insert(arch.clone(), single_comp);
        }

        compare_data
    }

    /// Grab the total number of packages in the new flake
    fn total_pkgs(&self) -> usize {
        self.pkg_data.values().map(|val| val.total_pkgs).sum()
    }

    /// Generate comparison report in markdown
    pub fn generate_report(&self, title: &Option<String>) -> String {
        let by_arch_stats = self
            .pkg_data
            .iter()
            .map(|(arch, data)| {
                format!(
                    "##### {}\n\
                    Added: {}\n\
                    Updated: {}\n\
                    Removed: {}\n\
                    Total: {}\n\
                    \n\
                    ",
                    arch,
                    data.added.len(),
                    data.updated.len(),
                    data.removed.len(),
                    data.total_pkgs
                )
            })
            .reduce(|mut acc, e| {
                acc.push_str(e.as_str());
                acc
            })
            .unwrap_or("".into());

        let mut report = format!(
            "## nix-update-report{}\n\
            Report generated using [`nix-update-report`](https://github.com/aldenparker/nix-update-report.git).\n\
            \n\
            ### Stats\n\
            #### By Arch\n\
            {}\
            #### Totals\n\
            Added Pkgs: {}\n\
            Updated Pkgs: {}\n\
            Removed Pkgs: {}\n\
            Pkgs: {}\n\
            Added Archs: {}\n\
            Removed Archs: {}\n\
            Archs: {}\n\
            \n\
            ",
            title.clone().map_or("".into(), |val| format!(" - {}", val)),
            by_arch_stats,
            self.pkg_data
                .iter()
                .map(|(_, data)| data.added.len())
                .sum::<usize>(),
            self.pkg_data
                .iter()
                .map(|(_, data)| data.updated.len())
                .sum::<usize>(),
            self.pkg_data
                .iter()
                .map(|(_, data)| data.removed.len())
                .sum::<usize>(),
            self.total_pkgs(),
            self.added_archs.len(),
            self.removed_archs.len(),
            self.total_archs
        );

        // Generate lists
        let pkgs_by_arch = (&self.pkg_data)
            .iter()
            .map(|(arch, pkgs)| {
                // Grab correct strings for each category
                let added = (&pkgs.added)
                    .iter()
                    .map(|pkg| match pkg {
                        Package::Unparsable(name) => format!(" - {}: unparsable\n", name),
                        Package::Parsed {
                            name,
                            version,
                            description: _,
                        } => format!(" - {}: {}\n", name, version.to_string()),
                    })
                    .reduce(|mut acc, e| {
                        acc.push_str(e.as_str());
                        acc
                    })
                    .unwrap_or("None\n".into());

                let updated = (&pkgs.updated)
                    .iter()
                    .map(|(_, compare_data)| match compare_data {
                        PkgCompareData::Changed {
                            change_string,
                            version_change: _,
                            description_change: _,
                        } => format!("{}\n", change_string),
                        _ => unreachable!(),
                    })
                    .reduce(|mut acc, e| {
                        acc.push_str(e.as_str());
                        acc
                    })
                    .unwrap_or("None\n".into());

                let removed = (&pkgs.removed)
                    .iter()
                    .map(|pkg| match pkg {
                        Package::Unparsable(name) => format!(" - {}: unparsable\n", name),
                        Package::Parsed {
                            name,
                            version,
                            description: _,
                        } => format!(" - {}: {}\n", name, version.to_string()),
                    })
                    .reduce(|mut acc, e| {
                        acc.push_str(e.as_str());
                        acc
                    })
                    .unwrap_or("None\n".into());

                // Create arch section
                format!(
                    "#### {}\n\
                    ##### Added\n\
                    {}\n\
                    ##### Updated\n\
                    {}\n\
                    ##### Removed\n\
                    {}\n\
                    ",
                    arch, added, updated, removed
                )
            })
            .reduce(|mut acc, e| {
                acc.push_str(e.as_str());
                acc
            })
            .unwrap_or("".into());

        report.push_str("### Pkg Changes\n");
        report.push_str(pkgs_by_arch.as_str());
        report
    }
}
