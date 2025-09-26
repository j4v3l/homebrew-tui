use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::process::Command;
use std::str;

#[derive(Clone, Deserialize, Debug, Default)]
pub struct InstalledInfo {
    #[serde(default)]
    pub version: String,
    // other fields omitted
}

#[derive(Clone, Deserialize, Debug, Default)]
pub struct FormulaInfo {
    pub name: String,
    pub full_name: Option<String>,
    pub desc: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub installed: Vec<InstalledInfo>,
    #[serde(default)]
    pub versions: Option<JsonValue>,
    #[serde(default)]
    pub caveats: Option<String>,
}

#[derive(Clone)]
pub struct Brew {}

impl Brew {
    pub fn new() -> Self {
        Self {}
    }

    pub fn list_installed(&self) -> Result<Vec<FormulaInfo>> {
        // Preferred: call `brew list --formula` to get names (more portable).
        let out = Command::new("brew")
            .arg("list")
            .arg("--formula")
            .output()
            .context("failed to run brew list --formula")?;

        // If the command succeeded and returned names, parse them line-by-line.
        if out.status.success() {
            let s = str::from_utf8(&out.stdout)?;
            let formulas: Vec<FormulaInfo> = s
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|name| {
                    let mut fi = FormulaInfo::default();
                    fi.name = name.to_string();
                    fi
                })
                .collect();
            if !formulas.is_empty() {
                return Ok(formulas);
            }
            // If empty, fall through to JSON attempt below
        }

        // Fallback: try JSON output (older/newer brews may support this on 'info' but not 'list')
        let out = Command::new("brew")
            .arg("list")
            .arg("--formula")
            .arg("--json=v2")
            .output()
            .context("failed to run brew list --json")?;
        if !out.status.success() {
            return Err(anyhow::anyhow!(
                "brew list failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        let s = str::from_utf8(&out.stdout)?;
        // Brew JSON v2 has {"formulae": [...]}
        #[derive(Deserialize)]
        struct List {
            formulae: Vec<FormulaInfo>,
        }
        let list: List = serde_json::from_str(s)?;
        Ok(list.formulae)
    }

    pub fn info(&mut self, name: &str) -> Result<FormulaInfo> {
        let out = Command::new("brew")
            .arg("info")
            .arg("--json=v2")
            .arg(name)
            .output()?;
        if !out.status.success() {
            return Err(anyhow::anyhow!(
                "brew info failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        let s = str::from_utf8(&out.stdout)?;
        #[derive(Deserialize)]
        struct Info {
            formulae: Vec<FormulaInfo>,
        }
        let info: Info = serde_json::from_str(s)?;
        info.formulae
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("no info"))
    }

    pub fn search(&self, query: &str) -> Result<Vec<String>> {
        let out = Command::new("brew").arg("search").arg(query).output()?;
        if !out.status.success() {
            return Err(anyhow::anyhow!(
                "brew search failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        let s = str::from_utf8(&out.stdout)?;
        Ok(s.lines().map(|l| l.to_string()).collect())
    }

    pub fn all_available(&self) -> Result<Vec<String>> {
        // Homebrew `brew search` requires an argument; use a regex that matches everything
        // and restrict to formulae for a stable list.
        let out = Command::new("brew")
            .arg("search")
            .arg("/.*/")
            .arg("--formula")
            .output()?;
        if !out.status.success() {
            return Err(anyhow::anyhow!(
                "brew search (all) failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        let s = str::from_utf8(&out.stdout)?;
        let mut v: Vec<String> = s
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        // dedupe and sort for stable display
        v.sort();
        v.dedup();
        Ok(v)
    }

    pub fn outdated(&self) -> Result<Vec<String>> {
        // `brew outdated --formula` lists installed formulae that are outdated
        let out = Command::new("brew")
            .arg("outdated")
            .arg("--formula")
            .output()?;
        if !out.status.success() {
            return Err(anyhow::anyhow!(
                "brew outdated failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        let s = str::from_utf8(&out.stdout)?;
        let v: Vec<String> = s
            .lines()
            .filter_map(|l| {
                let t = l.trim();
                if t.is_empty() {
                    return None;
                }
                // Take the first token before whitespace (e.g., "awscli (2.30.7) < 2.31.2" -> "awscli")
                let name = t.split_whitespace().next().unwrap_or(t).to_string();
                Some(name)
            })
            .collect();
        Ok(v)
    }

    pub fn install(&mut self, name: &str) -> Result<()> {
        let status = Command::new("brew").arg("install").arg(name).status()?;
        if status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("install failed"))
        }
    }

    pub fn upgrade(&mut self, name: &str) -> Result<()> {
        let status = Command::new("brew").arg("upgrade").arg(name).status()?;
        if status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("upgrade failed"))
        }
    }

    pub fn uninstall(&mut self, name: &str) -> Result<()> {
        let status = Command::new("brew").arg("uninstall").arg(name).status()?;
        if status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("uninstall failed"))
        }
    }
}
