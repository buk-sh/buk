use anyhow::Result;
use regex::Regex;

pub struct TypeScriptTranspiler;

impl TypeScriptTranspiler {
    pub fn new() -> Self {
        Self
    }

    pub fn transpile(&self, code: &str) -> Result<String> {
        let mut result = code.to_string();

        // Remove type annotations (simple regex-based approach)
        // Remove : type annotations from variable declarations
        let re = Regex::new(r":\s*\w+\s*(=|;|,|\))").unwrap();
        result = re.replace_all(&result, "${1}").to_string();

        // Remove function return types
        let re = Regex::new(r"\)\s*:\s*\w+\s*\{").unwrap();
        result = re.replace_all(&result, ") {").to_string();

        // Remove interface declarations
        let re = Regex::new(r"interface\s+\w+\s*\{[^}]*\}").unwrap();
        result = re.replace_all(&result, "").to_string();

        // Remove type aliases
        let re = Regex::new(r"type\s+\w+\s*=\s*[^;]+;").unwrap();
        result = re.replace_all(&result, "").to_string();

        // Remove generic type parameters from function calls
        let re = Regex::new(r"<(\w+)>(\()").unwrap();
        result = re.replace_all(&result, "${2}").to_string();

        Ok(result)
    }

    pub fn is_typescript(path: &str) -> bool {
        path.ends_with(".ts") || path.ends_with(".tsx")
    }
}

impl Default for TypeScriptTranspiler {
    fn default() -> Self {
        Self::new()
    }
}
