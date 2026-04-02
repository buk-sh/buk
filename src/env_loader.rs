use anyhow::Result;
use std::env;
use std::path::Path;

pub struct EnvLoader;

impl EnvLoader {
    pub fn load() -> Result<()> {
        let env_paths = [".env.local", ".env"];
        
        for path in &env_paths {
            if Path::new(path).exists() {
                dotenvy::from_path(path)?;
            }
        }
        
        Ok(())
    }
    
    pub fn load_from(path: &str) -> Result<()> {
        if Path::new(path).exists() {
            dotenvy::from_path(path)?;
        }
        Ok(())
    }
}
