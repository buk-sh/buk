use anyhow::{anyhow, Result};
use reqwest;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use tar::Archive;
use flate2::read::GzDecoder;

use crate::package_json::{LockFile, PackageJson, PackageMetadata};

const NPM_REGISTRY: &str = "https://registry.npmjs.org";

pub struct PackageManager {
    client: reqwest::Client,
    lockfile: LockFile,
    installed: HashSet<String>,
}

impl PackageManager {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            lockfile: LockFile::load().unwrap_or_default(),
            installed: HashSet::new(),
        }
    }

    // Install packages (one or more)
    pub async fn install(&mut self, packages: &[String], dev: bool) -> Result<()> {
        let mut package_json = PackageJson::load().unwrap_or_else(|_| {
            println!("📦 No package.json found, creating one...");
            let pkg = PackageJson::new("my-app", "1.0.0");
            pkg.save().expect("Failed to create package.json");
            pkg
        });

        if packages.is_empty() {
            // Install all dependencies from package.json
            self.install_all(&package_json).await?;
        } else {
            // Install specific packages
            for pkg in packages {
                self.install_single(&mut package_json, pkg, dev).await?;
            }
        }

        // Save lockfile
        self.lockfile.save()?;

        Ok(())
    }

    // Install a single package
    async fn install_single(&mut self, package_json: &mut PackageJson, package_spec: &str, dev: bool) -> Result<()> {
        let (name, version_req) = self.parse_package_spec(package_spec);
        
        println!("📦 Installing {}@{}...", name, version_req);

        // Fetch metadata
        let metadata = self.fetch_metadata(&name).await?;
        
        // Resolve version
        let version = self.resolve_version(&metadata, &version_req)?;
        
        if self.installed.contains(&format!("{}@{}", name, version)) {
            println!("  ✓ {}@{} already installed", name, version);
            return Ok(());
        }

        let version_info = metadata.versions.get(&version)
            .ok_or_else(|| anyhow!("Version {} not found for {}", version, name))?;

        // Download and extract
        self.download_and_extract(&name, &version, &version_info.dist.tarball).await?;

        // Update package.json
        package_json.add_dependency(&name, &format!("^{}", version), dev);
        package_json.save()?;

        // Update lockfile
        self.lockfile.add_package(
            &name,
            &version,
            &version_info.dist.tarball,
            &version_info.dist.shasum,
            version_info.dependencies.clone(),
        );

        self.installed.insert(format!("{}@{}", name, version));

        println!("✅ Installed {}@{}", name, version);

        // Install transitive dependencies
        if let Some(deps) = &version_info.dependencies {
            for (dep_name, dep_version) in deps {
                let dep_spec = format!("{}@{}", dep_name, dep_version.trim_start_matches('^').trim_start_matches('~'));
                Box::pin(self.install_single(package_json, &dep_spec, false)).await?;
            }
        }

        Ok(())
    }

    // Install all dependencies from package.json
    async fn install_all(&mut self, package_json: &PackageJson) -> Result<()> {
        let deps = package_json.get_all_dependencies();
        
        if deps.is_empty() {
            println!("No dependencies to install");
            return Ok(());
        }

        println!("📦 Installing {} dependencies...", deps.len());

        for (name, version) in deps {
            let spec = format!("{}@{}", name, version);
            let mut pkg = package_json.clone();
            self.install_single(&mut pkg, &spec, false).await?;
        }

        Ok(())
    }

    // Remove a package
    pub async fn remove(&mut self, package_name: &str) -> Result<()> {
        let mut package_json = PackageJson::load()?;
        
        // Check if it's a dependency or dev dependency
        let is_dev = package_json.dev_dependencies.as_ref()
            .map(|d| d.contains_key(package_name))
            .unwrap_or(false);

        package_json.remove_dependency(package_name, is_dev);
        package_json.save()?;

        // Remove from node_modules
        let package_dir = Path::new("node_modules").join(package_name);
        if package_dir.exists() {
            fs::remove_dir_all(&package_dir)?;
        }

        // Remove from lockfile
        self.lockfile.packages.remove(package_name);
        self.lockfile.save()?;

        println!("✅ Removed {}", package_name);
        Ok(())
    }

    // Run a package.json script
    pub fn run_script(&self, script_name: &str) -> Result<()> {
        let package_json = PackageJson::load()?;
        
        let script = package_json.get_script(script_name)
            .ok_or_else(|| anyhow!("Script '{}' not found in package.json", script_name))?;

        println!("$ {}", script);
        
        // Parse and run the script
        let parts: Vec<&str> = script.split_whitespace().collect();
        if parts.is_empty() {
            return Err(anyhow!("Empty script"));
        }

        let cmd = parts[0];
        let args = &parts[1..];

        let status = std::process::Command::new(cmd)
            .args(args)
            .status()?;

        if !status.success() {
            return Err(anyhow!("Script failed with exit code: {:?}", status.code()));
        }

        Ok(())
    }

    // List installed packages
    pub fn list(&self) -> Result<()> {
        let package_json = PackageJson::load()?;
        
        println!("📦 Dependencies:");
        if let Some(ref deps) = package_json.dependencies {
            for (name, version) in deps {
                println!("  {}@{} {}", name, version, self.check_installed(name, version));
            }
        }

        println!("\n📦 Dev Dependencies:");
        if let Some(ref dev_deps) = package_json.dev_dependencies {
            for (name, version) in dev_deps {
                println!("  {}@{} {}", name, version, self.check_installed(name, version));
            }
        }

        Ok(())
    }

    fn check_installed(&self, name: &str, version: &str) -> &'static str {
        let package_dir = Path::new("node_modules").join(name);
        if package_dir.exists() {
            "✓"
        } else {
            "✗"
        }
    }

    // Initialize a new project
    pub fn init(&self, name: Option<&str>) -> Result<()> {
        if PackageJson::exists() {
            return Err(anyhow!("package.json already exists"));
        }

        let project_name = name.unwrap_or("my-app");
        let package_json = PackageJson::new(project_name, "1.0.0");
        package_json.save()?;

        // Create basic files
        fs::write("index.ts", r#"console.log("Hello from Runt!");"#)?;
        fs::write(".gitignore", "node_modules\nrunt.lock\n")?;

        println!("✅ Initialized project '{}'", project_name);
        println!("  package.json created");
        println!("  index.ts created");
        println!("  .gitignore created");
        println!("\nNext steps:");
        println!("  runt i <package>    Install a package");
        println!("  runt run start      Run the start script");

        Ok(())
    }

    // Fetch package metadata from npm
    async fn fetch_metadata(&self, name: &str) -> Result<PackageMetadata> {
        let url = format!("{}/{}", NPM_REGISTRY, name);
        
        let response = self.client.get(&url).send().await?;
        
        if !response.status().is_success() {
            return Err(anyhow!("Package not found: {}", name));
        }

        let metadata: PackageMetadata = response.json().await?;
        Ok(metadata)
    }

    // Resolve version from version requirement
    fn resolve_version(&self, metadata: &PackageMetadata, version_req: &str) -> Result<String> {
        if version_req == "latest" || version_req == "*" {
            metadata.dist_tags.get("latest")
                .cloned()
                .ok_or_else(|| anyhow!("No latest version found"))
        } else {
            // Try exact version first
            if metadata.versions.contains_key(version_req) {
                Ok(version_req.to_string())
            } else {
                // Try to find a matching version (simplified semver matching)
                let clean_req = version_req.trim_start_matches('^').trim_start_matches('~');
                
                // Find highest matching version
                let mut candidates: Vec<&String> = metadata.versions.keys()
                    .filter(|v| v.starts_with(clean_req.split('.').next().unwrap_or("")))
                    .collect();
                
                candidates.sort();
                
                candidates.last()
                    .map(|v| v.to_string())
                    .ok_or_else(|| anyhow!("No version matching '{}' found", version_req))
            }
        }
    }

    // Parse package specification (name@version or just name)
    fn parse_package_spec(&self, spec: &str) -> (String, String) {
        if spec.contains('@') && !spec.starts_with('@') {
            let parts: Vec<&str> = spec.splitn(2, '@').collect();
            (parts[0].to_string(), parts[1].to_string())
        } else if spec.starts_with('@') {
            // Scoped package @scope/name@version
            let at_parts: Vec<&str> = spec.rsplitn(2, '@').collect();
            if at_parts.len() == 2 && at_parts[1].contains('/') {
                // at_parts[0] = version, at_parts[1] = @scope/name
                (at_parts[1].to_string(), at_parts[0].to_string())
            } else {
                (spec.to_string(), "latest".to_string())
            }
        } else {
            (spec.to_string(), "latest".to_string())
        }
    }

    // Download and extract package tarball
    async fn download_and_extract(&self, name: &str, version: &str, tarball_url: &str) -> Result<()> {
        // Download
        println!("  Downloading...");
        let response = self.client.get(tarball_url).send().await?;
        let bytes = response.bytes().await?;

        // Create node_modules directory
        let node_modules = Path::new("node_modules");
        if !node_modules.exists() {
            fs::create_dir_all(node_modules)?;
        }

        // Scoped packages need nested directories
        let package_dir = if name.starts_with('@') {
            let parts: Vec<&str> = name.splitn(2, '/').collect();
            node_modules.join(parts[0]).join(parts[1])
        } else {
            node_modules.join(name)
        };

        if !package_dir.parent().unwrap().exists() {
            fs::create_dir_all(package_dir.parent().unwrap())?;
        }

        // Extract tarball
        println!("  Extracting...");
        let tar = GzDecoder::new(&bytes[..]);
        let mut archive = Archive::new(tar);
        
        // npm packages have a "package/" prefix in the tarball
        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?;
            let path_str = path.to_string_lossy();
            
            // Remove "package/" prefix
            if let Some(stripped) = path_str.strip_prefix("package/") {
                let dest = package_dir.join(stripped);
                if let Some(parent) = dest.parent() {
                    if !parent.exists() {
                        fs::create_dir_all(parent)?;
                    }
                }
                entry.unpack(dest)?;
            }
        }

        Ok(())
    }

    // Resolve package path from node_modules
    pub fn resolve_package_in_dir(name: &str, from_dir: &Path) -> Result<String> {
        let mut current = from_dir.to_path_buf();
        
        loop {
            let node_modules = current.join("node_modules");
            
            if node_modules.exists() {
                let package_dir = if name.starts_with('@') {
                    let parts: Vec<&str> = name.splitn(2, '/').collect();
                    if parts.len() == 2 {
                        node_modules.join(parts[0]).join(parts[1])
                    } else {
                        return Err(anyhow!("Invalid scoped package name: {}", name));
                    }
                } else {
                    node_modules.join(name)
                };
                
                if package_dir.exists() {
                    let package_json = package_dir.join("package.json");
                    if package_json.exists() {
                        let content = fs::read_to_string(&package_json)?;
                        let pkg: serde_json::Value = serde_json::from_str(&content)?;
                        
                        let main = pkg.get("main")
                            .and_then(|m| m.as_str())
                            .unwrap_or("index.js");
                        
                        let entry = package_dir.join(main);
                        if entry.exists() {
                            return Ok(entry.to_string_lossy().to_string());
                        }
                    }
                    
                    let index = package_dir.join("index.js");
                    if index.exists() {
                        return Ok(index.to_string_lossy().to_string());
                    }
                }
            }
            
            if !current.pop() {
                break;
            }
        }
        
        Err(anyhow!("Package not found: {}", name))
    }

    // Resolve package path from node_modules (uses cwd)
    pub fn resolve_package(&self, name: &str) -> Result<String> {
        let cwd = std::env::current_dir()?;
        Self::resolve_package_in_dir(name, &cwd)
    }
}

impl Default for PackageManager {
    fn default() -> Self {
        Self::new()
    }
}
