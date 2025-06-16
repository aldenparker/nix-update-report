// All the structs used to organize package data when using the flake command

use regex::Regex;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use time::{Date, macros::format_description};

// --- TYPE ALIASES
type PkgMap = HashMap<String, Package>;

// --- SINGLE PACKAGE STRUCTS
/// Version enum for better versioning lookup
#[derive(PartialEq, Eq, Clone, Hash, Debug)]
enum PkgVersion {
    /// Includes numbered version (0.0.0 etc), extra version data (rc5 etc), and the unstable date if applicable
    Parsed {
        numbers: Vec<u16>,
        extra: Option<String>,
        unstable_date: Option<Date>,
    },
    /// Includes the original string passed (used when version_str can't be parsed)
    Unparsable(String),
}

impl PkgVersion {
    fn new(version_str: &String) -> PkgVersion {
        // Try to parse
        let regex_str = Regex::new(
            r"(?<version>\d+(?:\.\d+)*)(?<version_extra>[a-zA-Z0-9]+)?-?(?:unstable-(?<unstable_date>\d{4}-\d{2}-\d{2}))?",
        )
        .unwrap();

        let captures = regex_str.captures(version_str.as_str());

        if let Some(caps) = captures {
            return PkgVersion::Parsed {
                numbers: caps
                    .name("version")
                    .unwrap()
                    .as_str()
                    .split(".")
                    .map(|val| val.parse::<u16>().unwrap())
                    .collect(),
                extra: caps.name("version_extra").map(|m| m.as_str().into()),
                unstable_date: caps.name("unstable_date").map(|m| {
                    let format = format_description!("[year]-[month]-[day]");
                    Date::parse(m.as_str(), &format).unwrap()
                }),
            };
        }

        PkgVersion::Unparsable(version_str.clone())
    }

    fn to_string(&self) -> String {
        match self {
            PkgVersion::Unparsable(string) => string.clone(),
            PkgVersion::Parsed {
                numbers,
                extra,
                unstable_date,
            } => format!(
                "{}{}{}",
                numbers
                    .iter()
                    .map(|val| val.to_string())
                    .collect::<Vec<String>>()
                    .join("."),
                extra.clone().unwrap_or("".into()),
                unstable_date
                    .clone()
                    .map_or("".into(), |val| val.to_string())
            ),
        }
    }
}

/// Individual package data, parsed into data oriented forms
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
enum Package {
    /// Includes name, version, and ?description
    Parsed {
        name: String,
        version: PkgVersion,
        description: Option<String>,
    },
    /// Includes only the package name (used when full_name can't be parsed)
    Unparsable(String),
}

impl Package {
    fn new(full_name: &String, description: &Option<String>) -> Package {
        // Try to parse version from name
        let regex_str =
            Regex::new(r"(?P<name>.*?)-(?P<version>(?:unstable-)?[0-9][0-9a-zA-Z.-]*)").unwrap();

        let captures = regex_str.captures(full_name.as_str());

        if let Some(caps) = captures {
            return Package::Parsed {
                name: caps.name("name").map(|m| m.as_str().into()).unwrap(),
                version: PkgVersion::new(&caps.name("version").map(|m| m.as_str().into()).unwrap()),
                description: description.clone(),
            };
        }

        Package::Unparsable(full_name.clone())
    }

    /// Gets the name of the package
    fn get_name(&self) -> String {
        match self {
            Package::Parsed {
                name,
                version: _,
                description: _,
            } => name.clone(),
            Package::Unparsable(name) => name.clone(),
        }
    }
}

// --- PKG COMPARE
/// Holds data produced when two Package objects are compared
#[derive(PartialEq, Eq, Clone, Debug)]
enum PkgCompareData {
    /// The package changed
    Changed {
        /// Holds a string that shows the change (ex. package: 1.0.0 -> 2.0.0)
        change_string: String,
        /// Did the version change
        version_change: Option<bool>,
        /// Did the description change
        description_change: Option<bool>,
    },
    /// The package did not change
    Unchanged,
}

impl PkgCompareData {
    /// Compares two packages. If the package names are not the same, returns none.
    fn new(old: &Package, new: &Package) -> Option<PkgCompareData> {
        match (old, new) {
            (Package::Unparsable(name), Package::Unparsable(new_name)) => {
                if name != new_name {
                    return None;
                }

                return Some(PkgCompareData::Unchanged);
            }
            (
                Package::Parsed {
                    name,
                    version,
                    description: _,
                },
                Package::Unparsable(new_name),
            ) => {
                if name != new_name {
                    return None;
                }

                return Some(PkgCompareData::Changed {
                    change_string: format!("{}: {} -> unparsable", name, version.to_string()),
                    version_change: None,
                    description_change: None,
                });
            }
            (
                Package::Unparsable(name),
                Package::Parsed {
                    name: new_name,
                    version,
                    description: _,
                },
            ) => {
                if name != new_name {
                    return None;
                }

                return Some(PkgCompareData::Changed {
                    change_string: format!("{}: unparsable -> {}", name, version.to_string()),
                    version_change: None,
                    description_change: None,
                });
            }
            (
                Package::Parsed {
                    name,
                    version,
                    description,
                },
                Package::Parsed {
                    name: new_name,
                    version: new_version,
                    description: new_description,
                },
            ) => {
                if name != new_name {
                    return None;
                }

                if version != new_version || description != new_description {
                    return Some(PkgCompareData::Changed {
                        change_string: format!(
                            "{}: {} -> {}{}",
                            name,
                            version.to_string(),
                            new_version.to_string(),
                            match description != new_description {
                                true => ", description changed",
                                false => "",
                            }
                        ),
                        version_change: Some(version != new_version),
                        description_change: Some(description != new_description),
                    });
                }

                return Some(PkgCompareData::Unchanged);
            }
        }
    }
}

// --- ALL PACKAGES FROM FLAKE
/// A hash map that holds all package maps by arch from a flake
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FlakePkgs(HashMap<String, PkgMap>);

impl FlakePkgs {
    pub fn new(flake_json: &Value) -> FlakePkgs {
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

        FlakePkgs(new_fp)
    }
}

// --- FLAKE PKGS COMPARE
/// FlakePkgs comparison data for a single architecture
#[derive(PartialEq, Eq, Debug)]
struct FlakePkgsSingleArchCompareData {
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
#[derive(PartialEq, Eq, Debug)]
pub struct FlakePkgsCompareData {
    /// All the package compare data by arch
    pkg_data: HashMap<String, FlakePkgsSingleArchCompareData>,
    /// Archs removed from this flake
    removed_archs: Vec<String>,
    /// Archs added to this flake
    added_archs: Vec<String>,
    /// The total number of archs this flake supports
    total_archs: usize,
}

impl FlakePkgsCompareData {
    pub fn new(old: &FlakePkgs, new: &FlakePkgs) -> FlakePkgsCompareData {
        let mut compare_data = FlakePkgsCompareData {
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

            let mut single_comp = FlakePkgsSingleArchCompareData {
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
                    .unwrap_or("None".into());

                let updated = (&pkgs.updated)
                    .iter()
                    .map(|(_, compare_data)| match compare_data {
                        PkgCompareData::Changed {
                            change_string,
                            version_change: _,
                            description_change: _,
                        } => change_string.clone(),
                        _ => unreachable!(),
                    })
                    .reduce(|mut acc, e| {
                        acc.push_str(e.as_str());
                        acc
                    })
                    .unwrap_or("None".into());

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
                    .unwrap_or("None".into());

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
