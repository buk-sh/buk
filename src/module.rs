// Module loading system placeholder
// Will be expanded for ES modules, dynamic imports, etc.

use anyhow::Result;
use std::path::Path;

pub struct ModuleLoader;

impl ModuleLoader {
    pub fn new() -> Self {
        Self
    }

    pub fn load(&self, path: &Path) -> Result<String> {
        Ok(std::fs::read_to_string(path)?)
    }
}
