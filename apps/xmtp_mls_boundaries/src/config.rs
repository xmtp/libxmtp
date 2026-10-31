//! `boundaries.toml`: the target crate of each module, the allowed edges
//! between target crates, the counted waivers and the crate-name rule.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The analysis that measured an edge.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// `use` items and `crate::`, `super::` and `self::` paths, read with `syn`.
    Path,
    /// Every resolved reference in a rust-analyzer SCIP index, method calls
    /// included.
    Call,
}

impl std::fmt::Display for Tier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Tier::Path => "path",
            Tier::Call => "call",
        })
    }
}

/// A known edge that is not allowed yet, with the number of references the
/// tier measures on it.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Waiver {
    pub from: String,
    pub to: String,
    pub tier: Tier,
    pub count: usize,
    /// Which PR removes the edge, and why it exists until then.
    pub reason: String,
}

/// New workspace packages must start with `prefix` unless they are exempt;
/// `existing` lists the packages the rule does not apply to.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Naming {
    pub prefix: String,
    pub exempt: BTreeSet<String>,
    pub existing: BTreeSet<String>,
}

/// A method call the call tier cannot check because neither indexed build
/// compiles it.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Uncovered {
    /// The file, relative to the workspace.
    pub file: String,
    pub method: String,
    /// What code it is in and why only the path tier reads it.
    pub reason: String,
}

impl Uncovered {
    pub fn key(&self) -> (String, String) {
        (self.file.clone(), self.method.clone())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// The source root of the checked crate, relative to the workspace.
    pub source: String,
    /// Target crate of each module path (`::`-separated, `""` for the crate
    /// root). The longest matching module prefix wins.
    pub modules: BTreeMap<String, String>,
    /// Target crate of single items (`module::Item` or `module::Type::method`)
    /// that sit in a module mapped elsewhere.
    #[serde(default)]
    pub items: BTreeMap<String, String>,
    /// For each target crate, the target crates it may depend on.
    pub allowed: BTreeMap<String, BTreeSet<String>>,
    #[serde(default)]
    pub waivers: Vec<Waiver>,
    #[serde(default)]
    pub uncovered: Vec<Uncovered>,
    pub naming: Naming,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Self> {
        let config: Config = toml::from_str(text)?;
        config.validate()?;
        Ok(config)
    }

    /// Every target crate named anywhere must be a key of `allowed`, waivers
    /// must be distinct, positive and not cover an allowed edge.
    fn validate(&self) -> Result<()> {
        let known = |name: &String, what: &str| {
            if self.allowed.contains_key(name) {
                Ok(())
            } else {
                bail!("{what} names unknown target crate {name:?}")
            }
        };
        for (module, target) in &self.modules {
            known(target, &format!("module {module:?}"))?;
        }
        for (item, target) in &self.items {
            known(target, &format!("item {item:?}"))?;
        }
        for (from, targets) in &self.allowed {
            for to in targets {
                known(to, &format!("allowed edge from {from:?}"))?;
            }
        }
        if !self.modules.contains_key("") {
            bail!("modules must map the crate root \"\"");
        }
        let mut seen = BTreeSet::new();
        for waiver in &self.waivers {
            known(&waiver.from, "waiver")?;
            known(&waiver.to, "waiver")?;
            let key = (&waiver.from, &waiver.to, waiver.tier);
            if !seen.insert(key) {
                bail!(
                    "waiver {} -> {} ({}) is listed twice",
                    waiver.from,
                    waiver.to,
                    waiver.tier
                );
            }
            if waiver.count == 0 {
                bail!(
                    "waiver {} -> {} ({}) has count 0; remove it",
                    waiver.from,
                    waiver.to,
                    waiver.tier
                );
            }
            if waiver.from == waiver.to || self.is_allowed(&waiver.from, &waiver.to) {
                bail!(
                    "waiver {} -> {} ({}) covers an allowed edge",
                    waiver.from,
                    waiver.to,
                    waiver.tier
                );
            }
        }
        Ok(())
    }

    pub fn is_allowed(&self, from: &str, to: &str) -> bool {
        from == to
            || self
                .allowed
                .get(from)
                .is_some_and(|targets| targets.contains(to))
    }

    pub fn waiver(&self, from: &str, to: &str, tier: Tier) -> Option<&Waiver> {
        self.waivers
            .iter()
            .find(|waiver| waiver.from == from && waiver.to == to && waiver.tier == tier)
    }

    /// Problems that make this file looser than `base`: an added or raised
    /// waiver, or a package added to the naming exceptions.
    pub fn loosened_from(&self, base: &Config) -> Vec<String> {
        let mut problems = vec![];
        for waiver in &self.waivers {
            match base.waiver(&waiver.from, &waiver.to, waiver.tier) {
                None => problems.push(format!(
                    "waiver {} -> {} ({}) is new; the base has no such waiver",
                    waiver.from, waiver.to, waiver.tier
                )),
                Some(old) if waiver.count > old.count => problems.push(format!(
                    "waiver {} -> {} ({}) rose from {} to {}",
                    waiver.from, waiver.to, waiver.tier, old.count, waiver.count
                )),
                Some(_) => {}
            }
        }
        for name in self.naming.existing.difference(&base.naming.existing) {
            problems.push(format!("package {name:?} was added to naming.existing"));
        }
        for name in self.naming.exempt.difference(&base.naming.exempt) {
            problems.push(format!("package {name:?} was added to naming.exempt"));
        }
        for (from, targets) in &self.allowed {
            let before = base.allowed.get(from);
            for to in targets {
                if before.is_none_or(|before| !before.contains(to)) {
                    problems.push(format!("allowed edge {from} -> {to} is new"));
                }
            }
        }
        if self.naming.prefix != base.naming.prefix {
            problems.push("naming.prefix changed".into());
        }
        let count = |calls: &[Uncovered], key: &(String, String)| {
            calls.iter().filter(|call| call.key() == *key).count()
        };
        let keys: BTreeSet<(String, String)> = self.uncovered.iter().map(Uncovered::key).collect();
        for key in keys {
            let (now, before) = (count(&self.uncovered, &key), count(&base.uncovered, &key));
            if now > before {
                problems.push(format!(
                    "uncovered lists {} .{}() {now} times; the base lists it {before}",
                    key.0, key.1
                ));
            }
        }
        problems
    }
}
