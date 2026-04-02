use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Represents a package.json file
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PackageJson {
    pub name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub main: Option<String>,
    #[serde(rename = "type")]
    pub module_type: Option<String>,
    pub scripts: Option<HashMap<String, String>>,
    pub dependencies: Option<HashMap<String, String>>,
    #[serde(rename = "devDependencies")]
    pub dev_dependencies: Option<HashMap<String, String>>,
    #[serde(rename = "peerDependencies")]
    pub peer_dependencies: Option<HashMap<String, String>>,
    pub optional_dependencies: Option<HashMap<String, String>>,
    pub workspaces: Option<Vec<String>>,
    pub engines: Option<HashMap<String, String>>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl PackageJson {
    pub fn new(name: &str, version: &str) -> Self {
        Self {
            name: Some(name.to_string()),
            version: Some(version.to_string()),
            description: Some(format!("{} package", name)),
            main: Some("index.js".to_string()),
            module_type: Some("module".to_string()),
            scripts: Some(Self::default_scripts()),
            dependencies: Some(HashMap::new()),
            dev_dependencies: Some(HashMap::new()),
            peer_dependencies: None,
            optional_dependencies: None,
            workspaces: None,
            engines: None,
            extra: HashMap::new(),
        }
    }

    fn default_scripts() -> HashMap<String, String> {
        let mut scripts = HashMap::new();
        scripts.insert("test".to_string(), "runt test".to_string());
        scripts.insert("start".to_string(), "runt index.ts".to_string());
        scripts
    }

    pub fn load() -> Result<Self> {
        let path = Path::new("package.json");
        if !path.exists() {
            return Err(anyhow!("No package.json found. Run 'runt init' to create one."));
        }
        let content = fs::read_to_string(path)?;
        let package: PackageJson = serde_json::from_str(&content)?;
        Ok(package)
    }

    pub fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write("package.json", content)?;
        Ok(())
    }

    pub fn exists() -> bool {
        Path::new("package.json").exists()
    }

    pub fn add_dependency(&mut self, name: &str, version: &str, dev: bool) {
        let deps = if dev {
            self.dev_dependencies.get_or_insert_with(HashMap::new)
        } else {
            self.dependencies.get_or_insert_with(HashMap::new)
        };
        deps.insert(name.to_string(), version.to_string());
    }

    pub fn remove_dependency(&mut self, name: &str, dev: bool) {
        if dev {
            if let Some(ref mut deps) = self.dev_dependencies {
                deps.remove(name);
            }
        } else {
            if let Some(ref mut deps) = self.dependencies {
                deps.remove(name);
            }
        }
    }

    pub fn get_all_dependencies(&self) -> HashMap<String, String> {
        let mut all = HashMap::new();
        
        if let Some(ref deps) = self.dependencies {
            all.extend(deps.clone());
        }
        if let Some(ref dev_deps) = self.dev_dependencies {
            all.extend(dev_deps.clone());
        }
        
        all
    }

    pub fn get_script(&self, name: &str) -> Option<String> {
        self.scripts.as_ref()?.get(name).cloned()
    }
}

/// Lock file entry for a resolved package
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockEntry {
    pub name: String,
    pub version: String,
    pub resolved: String,
    pub integrity: String,
    pub dependencies: Option<HashMap<String, String>>,
}

/// Run.lock file for reproducible installs
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LockFile {
    pub lockfile_version: String,
    pub packages: HashMap<String, LockEntry>,
}

impl LockFile {
    pub fn new() -> Self {
        Self {
            lockfile_version: "1".to_string(),
            packages: HashMap::new(),
        }
    }

    pub fn load() -> Result<Self> {
        let path = Path::new("runt.lock");
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = fs::read_to_string(path)?;
        let lock: LockFile = serde_json::from_str(&content)?;
        Ok(lock)
    }

    pub fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write("runt.lock", content)?;
        Ok(())
    }

    pub fn add_package(&mut self, name: &str, version: &str, resolved: &str, integrity: &str, deps: Option<HashMap<String, String>>) {
        self.packages.insert(name.to_string(), LockEntry {
            name: name.to_string(),
            version: version.to_string(),
            resolved: resolved.to_string(),
            integrity: integrity.to_string(),
            dependencies: deps,
        });
    }

    pub fn get_package(&self, name: &str) -> Option<&LockEntry> {
        self.packages.get(name)
    }
}

/// Package metadata from npm registry
#[derive(Debug, Clone, Deserialize)]
pub struct PackageMetadata {
    pub name: String,
    #[serde(rename = "dist-tags")]
    pub dist_tags: HashMap<String, String>,
    pub versions: HashMap<String, PackageVersion>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PackageVersion {
    pub name: String,
    pub version: String,
    pub dist: PackageDist,
    pub dependencies: Option<HashMap<String, String>>,
    #[serde(rename = "devDependencies")]
    pub dev_dependencies: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PackageDist {
    pub tarball: String,
    pub shasum: String,
    pub integrity: Option<String>,
    pub size: Option<u64>,
}
