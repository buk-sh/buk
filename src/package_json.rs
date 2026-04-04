use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

// Represents a package.json file
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

// Represents a lock file entry for a resolved package
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

    // Here, we specify the default scripts that will be included in a new package.json when it is initialized.
    fn default_scripts() -> HashMap<String, String> {
        let mut scripts = HashMap::new();
        scripts.insert("test".to_string(), "runt test".to_string());
        scripts.insert("start".to_string(), "runt index.ts".to_string());
        scripts
    }

    // Here, we load the package.json file from disk, read its contents, and deserialize it into a PackageJson struct.
    pub fn load() -> Result<Self> {
        let path = Path::new("package.json");
        if !path.exists() {
            return Err(anyhow!("No package.json found. Run 'runt init' to create one."));
        }
        let content = fs::read_to_string(path)?;
        let package: PackageJson = serde_json::from_str(&content)?;
        Ok(package)
    }

    // Here, we save the package.json file to disk. 
    // This should be called whenever we make changes to the package.json in memory and want to persist those changes to disk.
    pub fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write("package.json", content)?;
        Ok(())
    }

    // Here, we check if the packacge.json file exists in the current directory. 
    // This is used by various commands to determine if they are being run in a valid project directory.
    pub fn exists() -> bool {
        Path::new("package.json").exists()
    }

    // This function allows us to add a dependency to the package.json. 
    // We specify the name and version of the dependency, as well as whether it is a dev dependency or a regular dependency.
    pub fn add_dependency(&mut self, name: &str, version: &str, dev: bool) {
        let deps = if dev {
            self.dev_dependencies.get_or_insert_with(HashMap::new)
        } else {
            self.dependencies.get_or_insert_with(HashMap::new)
        };
        deps.insert(name.to_string(), version.to_string());
    }

    // This function allows us to remove a dependency from the package.json.
    // We name a specific dependency by its name, and specify whether it is a dev dependency or a regular dependency.
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

    // This function allows us to get all dependencies, including regular and dev dependencies, 
    // We do this by creating a new HashMap and extending it with both the regular dependencies and dev dependencies.
    // So only a single hash map is returned with all dependencies.
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

// Lock file entry for a resolved package
// This is used in a runt.lock file to store the exact 
// versions and integrity of installed packages for reproducable installs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockEntry {
    pub name: String,
    pub version: String,
    pub resolved: String,
    pub integrity: String,
    pub dependencies: Option<HashMap<String, String>>,
}

// Run.lock file for reproducible installs
// This is done so we can reproduce the exact same dependency tree
// accross different machines and installs, even if the package.json has
// any version changes. 
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LockFile {
    pub lockfile_version: String,
    pub packages: HashMap<String, LockEntry>,
}

impl LockFile {
    // Creates a new empty lock file with the current lockfile version.
    // We use "1" as the lockfile version for now, but this can be updated in the future,
    // if we add new features or breaking changes. 
    // The lockfile version helps us manage compatibility and migrations down the line.
    pub fn new() -> Self {
        Self {
            lockfile_version: "1".to_string(),          // Current lockfile version,
                                                        // As stated above, this can be updated in the future if we add new features or breaking changes.
                                                        // The lockfile version helps us manage compatibility and migrations down the line.
            packages: HashMap::new(),                   // Map of package name to lock entry, representing the resolved dependencies and their integrity.
        }
    }

    pub fn load() -> Result<Self> {
        // The lock file is named "runt.lock" to avoid confusion with npm's package-lock.json.
        // It serves a similar purpose but is specific to runt's package management.
        // It is possible that we may rename the lock file file to buk.lock.
        let path = Path::new("runt.lock");
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = fs::read_to_string(path)?;
        let lock: LockFile = serde_json::from_str(&content)?;
        Ok(lock)
    }

    // Save the lock file to disk. This should *only* be called by the package manager
    // after it has resolved all dependencies and is ready to write the final lock file.
    pub fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write("runt.lock", content)?;               // We pass in the same "runt.lock" filename here to ensure we are writing to the correct lock file. 
                                                        // This keeps our lock file management consistent and avoids confusion with npm's package-lock.json.
        Ok(())                                          // We return Ok(()) to indicate that the save operation was successful. Any errors during serialization or file writing will be propagated as Err variants.
    }

    // This function allows us to actually add packages, to the lockfile.
    // We insert the package information into the internal packages map, which will then be saved to disk when save() is called.
    // The package information includes the name, version, resolved URL, integrity hash, and any dependencies.
    pub fn add_package(&mut self, name: &str, version: &str, resolved: &str, integrity: &str, deps: Option<HashMap<String, String>>) {
        self.packages.insert(name.to_string(), LockEntry {
            name: name.to_string(),                 // Package Name, e.g. "lodash"
            version: version.to_string(),           // Package Version, e.g. "4.17.21"
            resolved: resolved.to_string(),         // Package Resolved, e.g. "https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz"
            integrity: integrity.to_string(),       // Package Integrity, e.g. "sha512-abc123..."
            dependencies: deps,                     // Dependencies
        });
    }

    // This function allows us to get the packages.
    // We can use this to check if a package is already in the lock file, and to get its resolved information and integrity hash.
    // This is useful during the install process to determine if we can reuse an existing package or if we need to fetch a new version.
    pub fn get_package(&self, name: &str) -> Option<&LockEntry> {
        self.packages.get(name)                     // We return an Option<&LockEntry> here, which will be Some(&LockEntry) if 
                                                    // the package exists in the lock file, or None if it does not.
    }
}

// Package metadata from npm registry
// This is used when fetching package information from the npm registry, to get the available versions and dist-tags.
#[derive(Debug, Clone, Deserialize)]
pub struct PackageMetadata {
    pub name: String,                               // Package Name, e.g. "lodash"
    #[serde(rename = "dist-tags")]                  // Dist Tags, e.g. { "latest": "4.17.21", "beta": "5.0.0-beta" }
    pub dist_tags: HashMap<String, String>,         // We rename the dist-tags field from the npm registry to dist_tags in our struct for easier access and to follow Rust naming conventions.
    pub versions: HashMap<String, PackageVersion>,  // Versions, e.g. { "4.17.21": { name: "lodash", version: "4.17.21", dist: { tarball: "https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz", shasum: "abc123..." }, dependencies: { ... } }, ... }
}

// Package version information from npm registry
// This is used to represent a specific version of a package, including its dist information and dependencies
#[derive(Debug, Clone, Deserialize)]
pub struct PackageVersion {
    pub name: String,
    pub version: String,
    pub dist: PackageDist,
    pub dependencies: Option<HashMap<String, String>>,
    #[serde(rename = "devDependencies")]
    pub dev_dependencies: Option<HashMap<String, String>>,
}

// Package distribution information from npm registry
// This is used to represent the distribution information for a specific package version, including the tarball URL, shasum, integrity hash, and size. 
//  This information is crucial for downloading and verifying
#[derive(Debug, Clone, Deserialize)]
pub struct PackageDist {
    pub tarball: String,
    pub shasum: String,
    pub integrity: Option<String>,
    pub size: Option<u64>,
}
