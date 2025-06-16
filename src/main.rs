use clap::Parser;
use regex::Regex;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::Write,
    process::Command,
};

/// Small application to compare nixpkgs commits.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    /// The previous commit hash
    previous: String,
    /// The next commit hash
    next: String,
    /// Search for packages on all systems, not just the system you are on (may take a bit longer)
    #[arg(short, long)]
    all: bool,
}

// Individual package data
#[derive(PartialEq, Eq, Clone, Debug)]
enum Package {
    Filled {
        name: String,
        version: String,
        description: String,
    },
    Empty(String), // Some packages don't come with metadata for some reason
}

// A set of stats about the compare data that can be made
#[derive(PartialEq, Eq, Debug)]
struct CompareStats {
    // Package change totals by categories
    total_added_by_arch: HashMap<String, usize>,
    total_updated_by_arch: HashMap<String, usize>,
    total_removed_by_arch: HashMap<String, usize>,
    total_same_by_arch: HashMap<String, usize>,
    total_added: usize,
    total_updated: usize,
    total_removed: usize,
    total_same: usize,

    // Overall totals
    total_pkgs_by_arch: HashMap<String, usize>,
    total_pkgs: usize,
    total_unique_pkgs: usize,
    total_removed_archs: usize,
    total_added_archs: usize,
    total_archs: usize,
}

// The compare data for all packages in a single arch
#[derive(PartialEq, Eq, Debug)]
struct SingleArchPackagesCompareData {
    added: Vec<Package>,
    updated: Vec<(Package, Package)>,
    removed: Vec<Package>,
    same: Vec<Package>,
}

// The compare data for all packages recieved from nixpkgs
#[derive(PartialEq, Eq, Debug)]
struct PackagesCompareData {
    pkg_data: HashMap<String, SingleArchPackagesCompareData>,
    removed_archs: Vec<String>,
    added_archs: Vec<String>,
}

impl PackagesCompareData {
    fn new(prev: &Packages, next: &Packages) -> PackagesCompareData {
        let mut removed_archs: Vec<String> = vec![];
        let mut added_archs: Vec<String> = vec![];
        let mut pkg_data: HashMap<String, SingleArchPackagesCompareData> = HashMap::new();

        // Compare all archs that can be compared and track removed ones
        for (arch, pkg_map) in &prev.package_map {
            if !next.package_map.contains_key(arch) {
                removed_archs.push(arch.clone());
                continue;
            }

            let next_pkg_map = next.package_map.get(arch).unwrap(); // So I am not grabbing this all the time

            let added_packages: Vec<Package> = next_pkg_map
                .into_iter()
                .filter_map(|(key, val)| {
                    if !pkg_map.contains_key(key) {
                        return Some(val.clone());
                    }

                    return None;
                })
                .collect();

            let mut removed_packages: Vec<Package> = vec![];
            let mut updated_packages: Vec<(Package, Package)> = vec![];
            let mut same_packages: Vec<Package> = vec![];
            for (key, val) in pkg_map.into_iter() {
                match next_pkg_map.contains_key(key) {
                    false => removed_packages.push(val.clone()),
                    true => {
                        let next_pkg = next_pkg_map.get(key).unwrap();

                        if val != next_pkg {
                            updated_packages.push((val.clone(), next_pkg.clone()))
                        } else {
                            same_packages.push(val.clone());
                        }
                    }
                }
            }

            pkg_data.insert(
                arch.clone(),
                SingleArchPackagesCompareData {
                    added: added_packages,
                    updated: updated_packages,
                    removed: removed_packages,
                    same: same_packages,
                },
            );
        }

        // Check for added archs and log their packages as all added
        for (arch, next_pkg_map) in &next.package_map {
            if prev.package_map.contains_key(arch) {
                continue;
            }

            // New arch
            added_archs.push(arch.clone());

            // Add all lists to show it exists
            pkg_data.insert(
                arch.clone(),
                SingleArchPackagesCompareData {
                    added: next_pkg_map
                        .into_iter()
                        .map(|(_, val)| val.clone())
                        .collect(),
                    updated: vec![],
                    removed: vec![],
                    same: vec![],
                },
            );
        }

        PackagesCompareData {
            pkg_data,
            removed_archs,
            added_archs,
        }
    }

    fn generate_stats(&self) -> CompareStats {
        let total_added_by_arch: HashMap<String, usize> = (&self.pkg_data)
            .into_iter()
            .map(|(arch, pkgs)| (arch.clone(), pkgs.added.len()))
            .collect();
        let total_updated_by_arch: HashMap<String, usize> = (&self.pkg_data)
            .into_iter()
            .map(|(arch, pkgs)| (arch.clone(), pkgs.updated.len()))
            .collect();
        let total_removed_by_arch: HashMap<String, usize> = (&self.pkg_data)
            .into_iter()
            .map(|(arch, pkgs)| (arch.clone(), pkgs.removed.len()))
            .collect();
        let total_same_by_arch: HashMap<String, usize> = (&self.pkg_data)
            .into_iter()
            .map(|(arch, pkgs)| (arch.clone(), pkgs.same.len()))
            .collect();

        let total_added = total_added_by_arch.values().sum();
        let total_updated = total_updated_by_arch.values().sum();
        let total_removed = total_removed_by_arch.values().sum();
        let total_same = total_same_by_arch.values().sum();

        let total_pkgs_by_arch: HashMap<String, usize> = self
            .pkg_data
            .keys()
            .into_iter()
            .map(|arch| {
                (
                    arch.clone(),
                    total_added_by_arch.get(arch).unwrap()
                        + total_updated_by_arch.get(arch).unwrap()
                        + total_removed_by_arch.get(arch).unwrap()
                        + total_same_by_arch.get(arch).unwrap(),
                )
            })
            .collect();
        let total_pkgs = total_pkgs_by_arch.values().sum();

        let total_unique_pkgs: usize = (&self.pkg_data)
            .into_iter()
            .map(|(_, pkgs)| {
                // Get all packages in one
                let mut all_packages: Vec<Package> = vec![];
                all_packages.append(&mut pkgs.added.clone());
                all_packages.append(
                    &mut (&pkgs.updated)
                        .into_iter()
                        .map(|(pkg, _)| pkg.clone())
                        .collect(),
                );
                all_packages.append(&mut pkgs.removed.clone());
                all_packages.append(&mut pkgs.same.clone());

                // Create hashset
                all_packages
                    .into_iter()
                    .map(|val| match val {
                        Package::Empty(name) => name,
                        Package::Filled {
                            name,
                            version: _,
                            description: _,
                        } => name,
                    })
                    .collect::<HashSet<String>>()
            })
            .reduce(|acc, new| acc.union(&new).map(|val| val.clone()).collect())
            .unwrap()
            .len();

        let total_removed_archs = self.removed_archs.len();
        let total_added_archs = self.added_archs.len();
        let total_archs = self.pkg_data.keys().len();

        CompareStats {
            total_added_by_arch,
            total_updated_by_arch,
            total_removed_by_arch,
            total_same_by_arch,
            total_added,
            total_updated,
            total_removed,
            total_same,
            total_pkgs_by_arch,
            total_pkgs,
            total_unique_pkgs,
            total_removed_archs,
            total_added_archs,
            total_archs,
        }
    }
}

// The structures holding the package data (gotten from json)
#[derive(PartialEq, Eq, Clone, Debug)]
struct Packages {
    package_map: HashMap<String, HashMap<String, Package>>,
    all_systems: bool,
}

impl Packages {
    fn new(hash: &String, all: bool) -> Packages {
        // Download hash data
        let out = Command::new("sh")
            .arg("-c")
            .arg(if all {
                format!(
                    "nix flake show 'github:nixos/nixpkgs/{}' --legacy --json --quiet --all-systems",
                    hash
                )
            } else {
                format!(
                    "nix flake show 'github:nixos/nixpkgs/{}' --legacy --json --quiet",
                    hash
                )
            })
            .output()
            .expect("Failed to execute nix flake show for previous hash");

        if !out.status.success() {
            eprintln!("Hash Download Error:");
            eprintln!("{}", String::from_utf8_lossy(&out.stderr));
            std::process::exit(1);
        }

        // Proccess into packages type
        let full_json: Value =
            serde_json::from_str(String::from_utf8_lossy(&out.stdout).to_string().as_str())
                .expect(format!("Unable to parse hash's json data : {}", hash).as_str());

        let mut package_map: HashMap<String, HashMap<String, Package>> = HashMap::new();
        for (key, val) in full_json["legacyPackages"]
            .as_object()
            .expect(
                format!("Malformed json from hash : {}", hash)
                    .to_string()
                    .as_str(),
            )
            .into_iter()
        {
            if val.to_string() != "{}" {
                let name_parse_regex =
                    Regex::new(r"(?P<name>.*?)-(?P<version>(?:unstable-)?[0-9][0-9a-zA-Z.-]*)")
                        .unwrap();

                package_map.insert(
                    key.clone(),
                    val.as_object()
                        .unwrap()
                        .into_iter()
                        .map(|(key, value)| {
                            if !value["name"].is_null() {
                                let mut name: String = value["name"].as_str().unwrap().into();
                                let mut version: String = "unknown".into();
                                let captures = name_parse_regex
                                    .captures(value["name"].as_str().unwrap().into());

                                if let Some(caps) = captures {
                                    name = caps["name"].into();
                                    version = caps["version"].into();
                                }

                                return (
                                    key.clone(),
                                    Package::Filled {
                                        name,
                                        version,
                                        description: value["description"].as_str().unwrap().into(),
                                    },
                                );
                            } else {
                                return (key.clone(), Package::Empty(key.clone()));
                            }
                        })
                        .collect(),
                );
            }
        }

        if package_map.keys().len() == 0 {
            eprintln!("Packages had no valid lists");
            std::process::exit(1);
        };

        Packages {
            package_map,
            all_systems: all,
        }
    }

    // Compares this package map to a differnt one
    fn compare(&self, next_packages: &Packages) -> PackagesCompareData {
        PackagesCompareData::new(self, next_packages)
    }
}

fn genrate_report(
    compare_data: &PackagesCompareData,
    prev_hash: &String,
    next_hash: &String,
    all: bool,
) -> String {
    // Generate stats
    let stats = compare_data.generate_stats();

    // Generate prelude
    let mut report = match all {
        true => {
            let by_arch_stats = stats
                .total_added_by_arch
                .keys()
                .map(|arch| {
                    let added = stats.total_added_by_arch.get(arch).unwrap();
                    let updated = stats.total_updated_by_arch.get(arch).unwrap();
                    let removed = stats.total_removed_by_arch.get(arch).unwrap();
                    let same = stats.total_same_by_arch.get(arch).unwrap();
                    let total = stats.total_pkgs_by_arch.get(arch).unwrap();

                    format!(
                        "#### {}\n\
                        Added: {}\n\
                        Updated: {}\n\
                        Removed: {}\n\
                        Same: {}\n\
                        Total: {}\n\
                        \n\
                        ",
                        arch, added, updated, removed, same, total
                    )
                })
                .reduce(|mut acc, e| {
                    acc.push_str(e.as_str());
                    acc
                })
                .unwrap();

            format!(
                "# nix-update-report for {} -> {}\n\
                Report generated using [`nix-update-reporter`]().\n\
                \n\
                NOTE: nix-update-report can only get top level packages. Packages like `nushellPlugins.formats` will have their top level, in this case `nushellPlugins`, shown as version `unknown`.\n\
                \n\
                ## Stats\n\
                ### By Arch\n\
                {}\
                ### Totals\n\
                Total Added: {}\n\
                Total Updated: {}\n\
                Total Removed: {}\n\
                Total Same: {}\n\
                Total Pkgs: {}\n\
                Total Unique Pkgs: {}\n\
                Total Removed Archs: {}\n\
                Total Added Archs: {}\n\
                Total Archs: {}\n\
                \n\
                ",
                prev_hash,
                next_hash,
                by_arch_stats,
                stats.total_added,
                stats.total_updated,
                stats.total_removed,
                stats.total_same,
                stats.total_pkgs,
                stats.total_unique_pkgs,
                stats.total_removed_archs,
                stats.total_added_archs,
                stats.total_archs
            )
        }
        false => format!(
            "# nix-update-report for {} -> {}\n\
            Report generated using [`nix-update-reporter`]().\n\
            \n\
            ## Stats\n\
            Total Added: {}\n\
            Total Updated: {}\n\
            Total Removed: {}\n\
            Total Same: {}\n\
            Total Pkgs: {}\n\
            \n\
            ",
            prev_hash,
            next_hash,
            stats.total_added,
            stats.total_updated,
            stats.total_removed,
            stats.total_same,
            stats.total_pkgs
        ),
    };

    // Generate lists
    if all {
        let pkgs_by_arch = (&compare_data.pkg_data)
            .into_iter()
            .map(|(arch, pkgs)| {
                // Grab correct strings for each category
                let added = (&pkgs.added)
                    .into_iter()
                    .map(|pkg| match pkg {
                        Package::Empty(name) => format!(" - {}: unknown\n", name),
                        Package::Filled {
                            name,
                            version,
                            description: _,
                        } => format!(" - {}: {}\n", name, version),
                    })
                    .reduce(|mut acc, e| {
                        acc.push_str(e.as_str());
                        acc
                    })
                    .unwrap();

                let updated = (&pkgs.updated)
                    .into_iter()
                    .map(|pkg| match (&pkg.0, &pkg.1) {
                        (Package::Empty(_), Package::Empty(_)) => unreachable!(),
                        (
                            Package::Empty(_),
                            Package::Filled {
                                name,
                                version,
                                description: _,
                            },
                        ) => format!(" - {}: unknown -> {}\n", name, version),
                        (
                            Package::Filled {
                                name,
                                version,
                                description: _,
                            },
                            Package::Empty(_),
                        ) => format!(" - {}: {} -> unknown\n", name, version),
                        (
                            Package::Filled {
                                name,
                                version: v_old,
                                description: _,
                            },
                            Package::Filled {
                                name: _,
                                version: v_new,
                                description: _,
                            },
                        ) => format!(" - {}: {} -> {}\n", name, v_old, v_new),
                    })
                    .reduce(|mut acc, e| {
                        acc.push_str(e.as_str());
                        acc
                    })
                    .unwrap();

                let removed = (&pkgs.removed)
                    .into_iter()
                    .map(|pkg| match pkg {
                        Package::Empty(name) => format!(" - {}: unknown\n", name),
                        Package::Filled {
                            name,
                            version,
                            description: _,
                        } => format!(" - {}: {}\n", name, version),
                    })
                    .reduce(|mut acc, e| {
                        acc.push_str(e.as_str());
                        acc
                    })
                    .unwrap();

                // Create arch section
                format!(
                    "### {}\n\
                    #### Added\n\
                    {}\n\
                    #### Updated\n\
                    {}\n\
                    #### Removed\n\
                    {}\n\
                    ",
                    arch, added, updated, removed
                )
            })
            .reduce(|mut acc, e| {
                acc.push_str(e.as_str());
                acc
            })
            .unwrap();

        report.push_str("## Pkg Changes\n");
        report.push_str(pkgs_by_arch.as_str());
        return report;
    } else {
        let pkgs = (&compare_data.pkg_data)
            .into_iter()
            .map(|(_, pkgs)| {
                // Grab correct strings for each category
                let added = (&pkgs.added)
                    .into_iter()
                    .map(|pkg| match pkg {
                        Package::Empty(name) => format!(" - {}: unknown\n", name),
                        Package::Filled {
                            name,
                            version,
                            description: _,
                        } => format!(" - {}: {}\n", name, version),
                    })
                    .reduce(|mut acc, e| {
                        acc.push_str(e.as_str());
                        acc
                    })
                    .unwrap();

                let updated = (&pkgs.updated)
                    .into_iter()
                    .map(|pkg| match (&pkg.0, &pkg.1) {
                        (Package::Empty(_), Package::Empty(_)) => unreachable!(),
                        (
                            Package::Empty(_),
                            Package::Filled {
                                name,
                                version,
                                description: _,
                            },
                        ) => format!(" - {}: unknown -> {}\n", name, version),
                        (
                            Package::Filled {
                                name,
                                version,
                                description: _,
                            },
                            Package::Empty(_),
                        ) => format!(" - {}: {} -> unknown\n", name, version),
                        (
                            Package::Filled {
                                name,
                                version: v_old,
                                description: _,
                            },
                            Package::Filled {
                                name: _,
                                version: v_new,
                                description: _,
                            },
                        ) => format!(" - {}: {} -> {}\n", name, v_old, v_new),
                    })
                    .reduce(|mut acc, e| {
                        acc.push_str(e.as_str());
                        acc
                    })
                    .unwrap();

                let removed = (&pkgs.removed)
                    .into_iter()
                    .map(|pkg| match pkg {
                        Package::Empty(name) => format!(" - {}: unknown\n", name),
                        Package::Filled {
                            name,
                            version,
                            description: _,
                        } => format!(" - {}: {}\n", name, version),
                    })
                    .reduce(|mut acc, e| {
                        acc.push_str(e.as_str());
                        acc
                    })
                    .unwrap();

                // Create arch section
                format!(
                    "## Pkg Changes\n\
                    ### Added\n\
                    {}\n\
                    ### Updated\n\
                    {}\n\
                    ### Removed\n\
                    {}\n\
                    ",
                    added, updated, removed
                )
            })
            .next()
            .unwrap();

        report.push_str(pkgs.as_str());
        return report;
    }
}

fn main() {
    // Parse args
    let args = Cli::parse();

    // Grab commit data
    println!("Downloading and parsing packages based on hashes...");
    let prev_packages = Packages::new(&args.previous, args.all);
    let next_packages = Packages::new(&args.next, args.all);

    // Grab compare data
    println!("Comparing commits...");
    let compare_data = prev_packages.compare(&next_packages);

    // Generate report and save to report.md
    println!("Writing report...");
    let mut output = File::create("report.md").unwrap();
    write!(
        output,
        "{}",
        genrate_report(&compare_data, &args.previous, &args.next, args.all)
    )
    .expect("Unable to write report.md");
}
