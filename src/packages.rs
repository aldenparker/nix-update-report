// All of the package structs

use regex::Regex;
use time::{Date, macros::format_description};

// --- PKG
/// Version enum for better versioning lookup
#[derive(PartialEq, Eq, Hash, Clone)]
pub enum PkgVersion {
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
    pub fn new(version_str: &String) -> PkgVersion {
        // Try to parse
        let regex_str = Regex::new(
            r"^(?<version>\d+(?:\.\d+)*)(?<version_extra>[a-zA-Z0-9]+)?-?(?:unstable-(?<unstable_date>\d{4}-\d{2}-\d{2}))?$",
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

    pub fn to_string(&self) -> String {
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
#[derive(PartialEq, Eq, Hash, Clone)]
pub enum Package {
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
    pub fn new(full_name: &String, description: &Option<String>) -> Package {
        // Try to parse version from name
        let regex_str =
            Regex::new(r"^(?P<name>.*?)-(?P<version>(?:unstable-)?[0-9][0-9a-zA-Z.-]*)$").unwrap();

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
    pub fn get_name(&self) -> String {
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
pub enum PkgCompareData {
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
    pub fn new(old: &Package, new: &Package) -> Option<PkgCompareData> {
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
