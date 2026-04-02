use anyhow::{anyhow, Result};
use reqwest;
use serde::Deserialize;
use std::fs;
use std::path::Path;

const NPM_REGISTRY: &str = "https://registry.npmjs.org";

#[derive(Debug, Deserialize)]
struct NpmPackage {
    name: String,
    #[serde(rename = "dist-tags")]
    dist_tags: serde_json::Value,
    versions: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct NpmPackageVersion {
    name: String,
    version: String,
    dist: NpmDist,
    dependencies: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct NpmDist {
    tarball: String,
    shasum: String,
}

pub struct PackageManager;

impl PackageManager {
    pub fn new() -> Self {
        Self
    }

    pub async fn install(&self, package: &str) -> Result<()> {
        let package_name = package;
        let version = "latest";

        // Parse package@version format
        let (name, ver) = if package.contains('@') {
            let parts: Vec<&str> = package.split('@').collect();
            (parts[0], parts.get(1).copied().unwrap_or("latest"))
        } else {
            (package_name, version)
        };

        println!("📦 Installing {}@{}...", name, ver);

        // Fetch package info from npm registry
        let client = reqwest::Client::new();
        let url = format!("{}/{}", NPM_REGISTRY, name);
        
        let response = client.get(&url).send().await?;
        
        if !response.status().is_success() {
            return Err(anyhow!("Package not found: {}", name));
        }

        let npm_pkg: NpmPackage = response.json().await?;
        
        // Resolve version
        let resolved_version = if ver == "latest" {
            npm_pkg.dist_tags["latest"]
                .as_str()
                .ok_or_else(|| anyhow!("No latest version found"))?
        } else {
            ver
        };

        println!("  Resolved to version {}", resolved_version);

        // Get version info
        let version_data = npm_pkg.versions
            .get(resolved_version)
            .ok_or_else(|| anyhow!("Version {} not found", resolved_version))?;

        let tarball_url = version_data
            .get("dist")
            .and_then(|d| d.get("tarball"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| anyhow!("No tarball URL found"))?;

        // Download tarball
        println!("  Downloading...");
        let tarball_bytes = client.get(tarball_url).send().await?.bytes().await?;

        // Create node_modules directory
        let node_modules = Path::new("node_modules");
        if !node_modules.exists() {
            fs::create_dir_all(node_modules)?;
        }

        let package_dir = node_modules.join(name);
        if !package_dir.exists() {
            fs::create_dir_all(&package_dir)?;
        }

        // Extract tarball (simplified - just save for now)
        let package_json = serde_json::to_string_pretty(version_data)?;
        fs::write(package_dir.join("package.json"), package_json)?;

        // Extract the tarball
        println!("  Extracting...");
        extract_tarball(&tarball_bytes, &package_dir)?;

        println!("✅ Installed {}@{}", name, resolved_version);

        Ok(())
    }

    pub fn resolve_package(&self, name: &str) -> Result<String> {
        // Check node_modules
        let node_modules = Path::new("node_modules");
        let package_dir = node_modules.join(name);
        
        if package_dir.exists() {
            let package_json = package_dir.join("package.json");
            if package_json.exists() {
                let content = fs::read_to_string(package_json)?;
                let pkg: serde_json::Value = serde_json::from_str(&content)?;
                
                // Find main entry point
                let main = pkg.get("main")
                    .and_then(|m| m.as_str())
                    .unwrap_or("index.js");
                
                let entry = package_dir.join(main);
                if entry.exists() {
                    return Ok(entry.to_string_lossy().to_string());
                }
                
                // Try index.js
                let index = package_dir.join("index.js");
                if index.exists() {
                    return Ok(index.to_string_lossy().to_string());
                }
            }
        }

        Err(anyhow!("Package not found: {}", name))
    }
}

fn extract_tarball(data: &[u8], dest: &Path) -> Result<()> {
    // Simplified extraction - just save package.json for now
    // Full implementation would use tar crate
    use std::io::Write;
    
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(data));
    archive.unpack(dest)?;
    
    Ok(())
}

impl Default for PackageManager {
    fn default() -> Self {
        Self::new()
    }
}
