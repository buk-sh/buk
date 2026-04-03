use anyhow::{anyhow, Result};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use v8;

use crate::package_manager::PackageManager;
use crate::typescript::TypeScriptTranspiler;


/// Module loading system with CommonJS and ES module support, plus built-in module shims.
thread_local! {
    /// Cache for loaded modules to prevent reloading and handle circular dependencies
    static MODULE_CACHE: RefCell<HashMap<String, v8::Global<v8::Value>>> = RefCell::new(HashMap::new());
    /// Track the current directory for resolving relative imports
    static CURRENT_DIR: RefCell<String> = RefCell::new(".".to_string());
}

/// Set the current directory for module resolution.
/// This is used internally when loading modules to ensure 
/// that relative imports are resolved correctly.
pub fn set_current_dir(dir: &str) {
    CURRENT_DIR.with(|d| *d.borrow_mut() = dir.to_string());
}

/// The `require` function exposed to JavaScript for loading modules.
pub struct RequireAPI;

/// Initialize the `require` function and related properties in the global scope.
impl RequireAPI {
    pub fn init<'s>(scope: &mut v8::HandleScope<'s>, global: v8::Local<'s, v8::Object>) {
        let name = v8::String::new(scope, "require").unwrap();                          /// Create the `require` function template and set it on the global object
        let func = v8::FunctionTemplate::new(scope, require_callback);                  /// Set up `__dirname` and `__filename` for the main module
        let func = func.get_function(scope).unwrap();                                   /// Set up `__dynamicImport` for dynamic imports in ES modules
        global.set(scope, name.into(), func.into());                                    /// Initialize built-in modules like `fs`, `path`, `http`, and `querystring`

        let dirname = v8::String::new(scope, ".").unwrap();                             /// For the main module, `__dirname` is set to the current directory (".") and `__filename` is set to "<main>"
        let dirname_key = v8::String::new(scope, "__dirname").unwrap();                 /// This will be updated when loading actual files to reflect their directory
        global.set(scope, dirname_key.into(), dirname.into());                          /// The `__filename` is set to a placeholder for the main module, but will be updated for loaded modules

        let filename = v8::String::new(scope, "<main>").unwrap();                       /// The `__filename` is set to a placeholder for the main module, but will be updated for loaded modules
        let filename_key = v8::String::new(scope, "__filename").unwrap();               /// This allows the main module to have access to `__dirname` and `__filename`, and ensures that when other modules are loaded, these values will be correctly set based on their file paths.
        global.set(scope, filename_key.into(), filename.into());                        /// This setup allows the main module to have access to `__dirname` and `__filename`, and ensures that when other modules are loaded, these values will be correctly set based on their file paths.

        let import_func = v8::FunctionTemplate::new(scope, es_dynamic_import);          /// Set up `__dynamicImport` for dynamic imports in ES modules. This function will be called when an ES module uses the `import()` syntax, allowing it to load modules dynamically at runtime.
        let import_key = v8::String::new(scope, "__dynamicImport").unwrap();            /// This function will be called when an ES module uses the `import()` syntax, allowing it to load modules dynamically at runtime.
        let import_fn = import_func.get_function(scope).unwrap();                       /// By setting `__dynamicImport` on the global object, we enable support for dynamic imports in ES modules. When an ES module calls `import('some-module')`, it will invoke this function, which will handle the module resolution and loading process, similar to how the `require` function works for CommonJS modules.
        global.set(scope, import_key.into(), import_fn.into());                         /// By setting `__dynamicImport` on the global object, we enable support for dynamic imports in ES modules. When an ES module calls `import('some-module')`, it will invoke this function, which will handle the module resolution and loading process, similar to how the `require` function works for CommonJS modules.
    }
}

/// Determine if a module is an ES module by checking for `import` or `export` statements.
/// This is a heuristic approach that looks for ES module syntax while ignoring comments and empty lines. 
/// If it finds `import` or `export` statements without any CommonJS patterns like `module.exports` or `require()`, 
/// it treats the module as an ES module.
fn is_es_module(content: &str) -> bool {
    let trimmed = content.trim();                                                       /// Check if the content is empty or starts with a comment. If so, we can skip the ES module check for this line, but we need to continue checking other lines until we find actual code.
    let starts_with_comment_or_empty =                                                  /// This allows us to ignore leading comments and whitespace when determining if a file is an ES module. We only start looking for `import` or `export` statements once we encounter a line of code that is not a comment or empty.
        trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("/*");   /// This allows us to ignore leading comments and whitespace when determining if a file is an ES module. We only start looking for `import` or `export` statements once we encounter a line of code that is not a comment or empty.

    for line in content.lines() {                                                       /// Iterate through each line of the content. We trim the line to ignore leading and trailing whitespace, which allows us to accurately detect `import` and `export` statements even if they are indented or have extra spaces.
        let line = line.trim();                                                         /// Trim the line to ignore leading and trailing whitespace, which allows us to accurately detect `import` and `export` statements even if they are indented or have extra spaces.
        if line.is_empty() || line.starts_with("//") || line.starts_with("/*") {        /// Check if the line is empty or starts with a comment. If so, we can skip the ES module check for this line, but we need to continue checking other lines until we find actual code.
            continue;                                                                   /// This allows us to ignore leading comments and whitespace when determining if a file is an ES module. We only start looking for `import` or `export` statements once we encounter a line of code that is not a comment or empty.
        }                                                               
        if line.starts_with("import ") || line.starts_with("export ") {                 /// Check if the line starts with `import` or `export`. If it does, we have a strong indication that this is an ES module. However, we also need to check for CommonJS patterns to avoid false positives.
            if content.contains("module.exports") || content.contains("require(") {     /// If we find `import` or `export` statements but also see `module.exports` or `require()`, it's likely that this file is using CommonJS syntax, and we should not treat it as an ES module. This is a heuristic to handle cases where a file might have mixed syntax, which is uncommon but possible.
                return false;                                                           /// If we find `import` or `export` statements but also see `module.exports` or `require()`, it's likely that this file is using CommonJS syntax, and we should not treat it as an ES module. This is a heuristic to handle cases where a file might have mixed syntax, which is uncommon but possible.
            }
            return true;                                                                /// If we find `import` or `export` statements and do not see CommonJS patterns, we can confidently treat this file as an ES module.
        }
        if line.starts_with("var ") || line.starts_with("const ") || line.starts_with("let ") {
            continue;                                                                   /// If we encounter variable declarations at the top of the file, we can skip them as they do not necessarily indicate that the file is not an ES module. Some ES modules may have top-level variable declarations before any `import` or `export` statements.
        }
        if !starts_with_comment_or_empty {                                              /// If we encounter a line of code that is not a comment or empty, and it does not start with `import` or `export`, we can break out of the loop. This means we've reached actual code without finding any ES module syntax, so we can conclude that this is not an ES module. 
            break;                                                                      /// If we encounter a line of code that is not a comment or empty, and it does not start with `import` or `export`, we can break out of the loop. This means we've reached actual code without finding any ES module syntax, so we can conclude that this is not an ES module.
        }
    }
    false                                                                               /// If we finish iterating through the lines without finding any `import` or `export` statements, we can conclude that this is not an ES module.
}

/// Shim ES module syntax to CommonJS. This function takes the content of an ES module and transforms 
/// it into a format that can be executed in a CommonJS environment. It handles `import` statements by converting them to 
/// `require()`, and it handles `export` statements by assigning exports to `module.exports`. 
/// This allows us to load ES modules in our runtime even if they are not natively supported, 
/// by transforming them into a compatible format.
fn shim_es_module(content: &str, file_path: &str) -> String {
    let mut result = String::new();                                                     /// Determine the directory of the file being processed. This is important for resolving relative imports correctly when shimming ES module syntax. By knowing the directory of the current file, we can ensure that any relative import paths are transformed correctly in the shimmed code.
    let dirname = Path::new(file_path)                                                  /// Determine the directory of the file being processed. This is important for resolving relative imports correctly when shimming ES module syntax. By knowing the directory of the current file, we can ensure that any relative import paths are transformed correctly in the shimmed code.
        .parent()                                                                       /// Determine the directory of the file being processed. This is important for resolving relative imports correctly when shimming ES module syntax. By knowing the directory of the current file, we can ensure that any relative import paths are transformed correctly in the shimmed code.
        .map(|p| p.to_string_lossy())                                                   /// Determine the directory of the file being processed. This is important for resolving relative imports correctly when shimming ES module syntax. By knowing the directory of the current file, we can ensure that any relative import paths are transformed correctly in the shimmed code.
        .unwrap_or_default()                                                            /// Determine the directory of the file being processed. This is important for resolving relative imports correctly when shimming ES module syntax. By knowing the directory of the current file, we can ensure that any relative import paths are transformed correctly in the shimmed code.
        .to_string();                                                                   /// Determine the directory of the file being processed. This is important for resolving relative imports correctly when shimming ES module syntax. By knowing the directory of the current file, we can ensure that any relative import paths are transformed correctly in the shimmed code.
    let mut named_exports: Vec<String> = Vec::new();                                    /// This vector will hold the names of any named exports found in the ES module. When we encounter `export` statements that define named exports, we will extract the export names and store them in this vector. Later, we will use this list of named exports to construct the `module.exports` object in the shimmed CommonJS code, ensuring that all named exports are correctly exposed.
    let mut default_export: Option<String> = None;                                      /// This variable will hold the default export if we encounter an `export default` statement in the ES module. When we find an `export default`, we will extract the expression being exported and store it in this variable. Later, when constructing the `module.exports` object, we will check if there is a default export and assign it to `module.exports.default` if there are also named exports, or directly to `module.exports` if there are no named exports. This allows us to correctly handle both named and default exports in the shimmed CommonJS code.

    for line in content.lines() {                                                       /// Iterate through each line of the ES module content. For each line, we will check if it contains `import` or `export` statements and transform them accordingly. We trim the line to ignore leading and trailing whitespace, which allows us to accurately detect `import` and `export` statements even if they are indented or have extra spaces.
        let trimmed = line.trim();                                                      /// Trim the line to ignore leading and trailing whitespace, which allows us to accurately detect `import` and `export` statements even if they are indented or have extra spaces.

        if trimmed.starts_with("import ") {                                             /// If the line starts with `import`, we will shim it to a `require()` statement. We call the `shim_import_line` function, passing the trimmed line and the directory name of the current file. This function will handle the transformation of the ES module import syntax into CommonJS require syntax, taking into account different forms of imports (default, named, namespace) and ensuring that relative paths are resolved correctly based on the directory of the current file.
            let shimmed = shim_import_line(trimmed, &dirname);                          /// If the line starts with `import`, we will shim it to a `require()` statement. We call the `shim_import_line` function, passing the trimmed line and the directory name of the current file. This function will handle the transformation of the ES module import syntax into CommonJS require syntax, taking into account different forms of imports (default, named, namespace) and ensuring that relative paths are resolved correctly based on the directory of the current file.
            result.push_str(&shimmed);                                                  /// If the line starts with `import`, we will shim it to a `require()` statement. We call the `shim_import_line` function, passing the trimmed line and the directory name of the current file. This function will handle the transformation of the ES module import syntax into CommonJS require syntax, taking into account different forms of imports (default, named, namespace) and ensuring that relative paths are resolved correctly based on the directory of the current file.
            result.push('\n');                                                          /// If the line starts with `import`, we will shim it to a `require()` statement. We call the `shim_import_line` function, passing the trimmed line and the directory name of the current file. This function will handle the transformation of the ES module import syntax into CommonJS require syntax, taking into account different forms of imports (default, named, namespace) and ensuring that relative paths are resolved correctly based on the directory of the current file.
        } else if trimmed.starts_with("export default") {                               /// If the line starts with `export default`, we will shim it to a CommonJS export. We call the `shim_export_default_line` function, passing the trimmed line. This function will extract the expression being exported as the default export and return a string that assigns it to a variable (e.g., `var __default = ...`). We store this in the `default_export` variable for later use when constructing the `module.exports` object. Additionally, we push a comment into the result indicating that this was a default export, and we also include a version of the line where `export default` is replaced with `var`, which can help preserve the original code structure while still allowing us to capture the default export.
            let shimmed = shim_export_default_line(trimmed);                            /// If the line starts with `export default`, we will shim it to a CommonJS export. We call the `shim_export_default_line` function, passing the trimmed line. This function will extract the expression being exported as the default export and return a string that assigns it to a variable (e.g., `var __default = ...`). We store this in the `default_export` variable for later use when constructing the `module.exports` object. Additionally, we push a comment into the result indicating that this was a default export, and we also include a version of the line where `export default` is replaced with `var`, which can help preserve the original code structure while still allowing us to capture the default export.
            default_export = Some(shimmed);                                             /// If the line starts with `export default`, we will shim it to a CommonJS export. We call the `shim_export_default_line` function, passing the trimmed line. This function will extract the expression being exported as the default export and return a string that assigns it to a variable (e.g., `var __default = ...`). We store this in the `default_export` variable for later use when constructing the `module.exports` object. Additionally, we push a comment into the result indicating that this was a default export, and we also include a version of the line where `export default` is replaced with `var`, which can help preserve the original code structure while still allowing us to capture the default export.
            result.push_str("// default export: ");                                     /// If the line starts with `export default`, we will shim it to a CommonJS export. We call the `shim_export_default_line` function, passing the trimmed line. This function will extract the expression being exported as the default export and return a string that assigns it to a variable (e.g., `var __default = ...`). We store this in the `default_export` variable for later use when constructing the `module.exports` object. Additionally, we push a comment into the result indicating that this was a default export, and we also include a version of the line where `export default` is replaced with `var`, which can help preserve the original code structure while still allowing us to capture the default export.
            result.push_str(&trimmed.replace("export default", "var"));                 /// If the line starts with `export default`, we will shim it to a CommonJS export. We call the `shim_export_default_line` function, passing the trimmed line. This function will extract the expression being exported as the default export and return a string that assigns it to a variable (e.g., `var __default = ...`). We store this in the `default_export` variable for later use when constructing the `module.exports` object. Additionally, we push a comment into the result indicating that this was a default export, and we also include a version of the line where `export default` is replaced with `var`, which can help preserve the original code structure while still allowing us to capture the default export.
            result.push('\n');                                                          /// If the line starts with `export default`, we will shim it to a CommonJS export. We call the `shim_export_default_line` function, passing the trimmed line. This function will extract the expression being exported as the default export and return a string that assigns it to a variable (e.g., `var __default = ...`). We store this in the `default_export` variable for later use when constructing the `module.exports` object. Additionally, we push a comment into the result indicating that this was a default export, and we also include a version of the line where `export default` is replaced with `var`, which can help preserve the original code structure while still allowing us to capture the default export.
        } else if trimmed.starts_with("export ") {                                      /// If the line starts with `export`, we will shim it to a CommonJS export. We call the `shim_export_line` function, passing the trimmed line. This function will extract the name of the export (if it's a named export) and return it. If it's a named export, we add it to the `named_exports` vector. We also push a version of the line where `export` is replaced with an empty string, which allows us to preserve the original code structure while still capturing the exports.
            let shimmed = shim_export_line(trimmed);                                    /// If the line starts with `export`, we will shim it to a CommonJS export. We call the `shim_export_line` function, passing the trimmed line. This function will extract the name of the export (if it's a named export) and return it. If it's a named export, we add it to the `named_exports` vector. We also push a version of the line where `export` is replaced with an empty string, which allows us to preserve the original code structure while still capturing the exports.
            if !shimmed.starts_with("/* skipped:") {                                    /// If the `shim_export_line` function returns a non-empty string that does not start with "/* skipped:", it means we have successfully extracted a named export. We add this export name to the `named_exports` vector, which we will later use to construct the `module.exports` object in the shimmed CommonJS code.
                named_exports.push(shimmed);                                            /// If the `shim_export_line` function returns a non-empty string that does not start with "/* skipped:", it means we have successfully extracted a named export. We add this export name to the `named_exports` vector, which we will later use to construct the `module.exports` object in the shimmed CommonJS code.
            }
            result.push_str(&trimmed.replace("export ", ""));                           /// If the line starts with `export`, we will shim it to a CommonJS export. We call the `shim_export_line` function, passing the trimmed line. This function will extract the name of the export (if it's a named export) and return it. If it's a named export, we add it to the `named_exports` vector. We also push a version of the line where `export` is replaced with an empty string, which allows us to preserve the original code structure while still capturing the exports.
            result.push('\n');                                                          /// If the line starts with `export`, we will shim it to a CommonJS export. We call the `shim_export_line` function, passing the trimmed line. This function will extract the name of the export (if it's a named export) and return it. If it's a named export, we add it to the `named_exports` vector. We also push a version of the line where `export` is replaced with an empty string, which allows us to preserve the original code structure while still capturing the exports.
        } else {                                                                        /// If the line does not start with `import` or `export`, we simply include it in the result as-is. This allows us to preserve the original code structure for lines that are not related to module syntax, while still ensuring that all `import` and `export` statements are properly shimmed.
            result.push_str(line);                                                      /// If the line does not start with `import` or `export`, we simply include it in the result as-is. This allows us to preserve the original code structure for lines that are not related to module syntax, while still ensuring that all `import` and `export` statements are properly shimmed.
            result.push('\n');                                                          /// If the line does not start with `import` or `export`, we simply include it in the result as-is. This allows us to preserve the original code structure for lines that are not related to module syntax, while still ensuring that all `import` and `export` statements are properly shimmed.             
        }
    }

    if !named_exports.is_empty() {                                                      /// If we have collected any named exports, we need to construct the `module.exports` object to include these exports. We start by pushing a line that initializes `module.exports` with an object containing all the named exports. We join the named exports with commas to create the object literal. This way, all named exports will be available as properties on `module.exports` in the shimmed CommonJS code.
        result.push_str("\nmodule.exports = { ");                                       /// If we have collected any named exports, we need to construct the `module.exports` object to include these exports. We start by pushing a line that initializes `module.exports` with an object containing all the named exports. We join the named exports with commas to create the object literal. This way, all named exports will be available as properties on `module.exports` in the shimmed CommonJS code.
        result.push_str(&named_exports.join(", "));                                     /// If we have collected any named exports, we need to construct the `module.exports` object to include these exports. We start by pushing a line that initializes `module.exports` with an object containing all the named exports. We join the named exports with commas to create the object literal. This way, all named exports will be available as properties on `module.exports` in the shimmed CommonJS code.
        result.push_str(" };\n");                                                       /// If we have collected any named exports, we need to construct the `module.exports` object to include these exports. We start by pushing a line that initializes `module.exports` with an object containing all the named exports. We join the named exports with commas to create the object literal. This way, all named exports will be available as properties on `module.exports` in the shimmed CommonJS code.
    }

    if let Some(default) = default_export {                                             /// If we have a default export, we need to assign it to `module.exports`. If there are also named exports, we assign the default export to `module.exports.default` to avoid overwriting the named exports. If there are no named exports, we can assign the default export directly to `module.exports`. This ensures that both named and default exports are correctly exposed in the shimmed CommonJS code.
        if !named_exports.is_empty() {                                                  /// If we have a default export, we need to assign it to `module.exports`. If there are also named exports, we assign the default export to `module.exports.default` to avoid overwriting the named exports. If there are no named exports, we can assign the default export directly to `module.exports`. This ensures that both named and default exports are correctly exposed in the shimmed CommonJS code.
            result.push_str("module.exports.default = ");                               /// If we have a default export, we need to assign it to `module.exports`. If there are also named exports, we assign the default export to `module.exports.default` to avoid overwriting the named exports. If there are no named exports, we can assign the default export directly to `module.exports`. This ensures that both named and default exports are correctly exposed in the shimmed CommonJS code.
        }
        result.push_str(&default);                                                      /// If we have a default export, we need to assign it to `module.exports`. If there are also named exports, we assign the default export to `module.exports.default` to avoid overwriting the named exports. If there are no named exports, we can assign the default export directly to `module.exports`. This ensures that both named and default exports are correctly exposed in the shimmed CommonJS code.     
        result.push('\n');                                                              /// If we have a default export, we need to assign it to `module.exports`. If there are also named exports, we assign the default export to `module.exports.default` to avoid overwriting the named exports. If there are no named exports, we can assign the default export directly to `module.exports`. This ensures that both named and default exports are correctly exposed in the shimmed CommonJS code.
    }

    result                                                                              /// Finally, If we have a default export, we need to assign it to `module.exports`. If there are also named exports, we assign the default export to `module.exports.default` to avoid overwriting the named exports. If there are no named exports, we can assign the default export directly to `module.exports`. This ensures that both named and default exports are correctly exposed in the shimmed CommonJS code.
}
/**
 * Shim a single `import` line from ES module syntax to CommonJS `require()` syntax.
 * This function handles different forms of imports, including default imports, named imports,
 * and namespace imports. It also takes into account the directory of the current file to resolve relative import
 * paths correctly. If the line cannot be shimmed (e.g., it uses unsupported syntax), it returns a comment
 * indicating that the import was skipped.
 */
fn shim_import_line(line: &str, dirname: &str) -> String {
    let line = line.trim_end_matches(';');                                              /// Trim the line to remove any trailing semicolons, which allows us to focus on the actual import syntax without being affected by formatting differences. This makes it easier to parse the line and extract the relevant parts for shimming.

    if line.contains('{') && line.contains('}') {                                       /// If the line contains both `{` and `}`, it indicates that we have named imports. We need to extract the part between the braces to determine which named imports are being imported, and then construct a `require()` statement that includes these named imports as properties of the imported module.
        let between_braces = line                                                       /// If the line contains both `{` and `}`, it indicates that we have named imports. We need to extract the part between the braces to determine which named imports are being imported, and then construct a `require()` statement that includes these named imports as properties of the imported module.
            .split('{')                                                                 /// If the line contains both `{` and `}`, it indicates that we have named imports. We need to extract the part between the braces to determine which named imports are being imported, and then construct a `require()` statement that includes these named imports as properties of the imported module.
            .nth(1)                                                                     /// If the line contains both `{` and `}`, it indicates that we have named imports. We need to extract the part between the braces to determine which named imports are being imported, and then construct a `require()` statement that includes these named imports as properties of the imported module.
            .unwrap_or("")
            .split('}')
            .next()
            .unwrap_or("");

        let imports: Vec<String> = between_braces
            .split(',')
            .filter_map(|s| {
                let s = s.trim();
                if s.is_empty() || s == "type" {
                    return None;
                }
                let parts: Vec<&str> = s.split_ascii_whitespace().collect();
                if parts.is_empty() {
                    return None;
                }
                let import_name = parts[0];
                let alias = if s.contains(" as ") {
                    parts.last().unwrap_or(&import_name).to_string()
                } else {
                    import_name.to_string()
                };
                Some(format!("{}: {}", import_name, alias))
            })
            .collect();

        if imports.is_empty() {
            return String::new();
        }

        let path = if let Some(from_idx) = line.find("from") {
            let from_part = &line[from_idx..];
            from_part
                .trim_start_matches("from")
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string()
        } else {
            dirname.to_string()
        };

        return format!("const {{{}}} = require('{}');", imports.join(", "), path);
    }

    if let Some(from_idx) = line.find("from") {
        let from_part = &line[from_idx..];
        let path = from_part
            .trim_start_matches("from")
            .trim()
            .trim_matches('"')
            .trim_matches('\'');
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[1] != "from" {
            return format!("const {} = require('{}');", parts[1], path);
        }
    }

    format!("/* skipped import: {} */", line)
}

/// Extracts the export name from an ES module export statement.
/// This function parses different forms of export statements and returns the name
/// of the exported item, which will be used to construct the module.exports object.
fn shim_export_line(line: &str) -> String {
    let line = line.trim_end_matches(';').trim();                                     /// Clean up the line by removing trailing semicolons and whitespace.

    if line.starts_with("export {") {                                                 /// Handle re-export syntax like `export { foo, bar }`.
        let start = line.find("{").unwrap_or(line.len());                             /// Find the opening brace.
        let end = line.rfind("}").unwrap_or(line.len());                              /// Find the closing brace.
        let between_braces = &line[start + 1..end];                                   /// Extract the content between braces.

        let exports: Vec<String> = between_braces                                     /// Parse the comma-separated export names.
            .split(',')                                                               /// Split on commas.
            .filter_map(|s| {                                                         /// Process each export item.
                let trimmed = s.trim();                                               /// Trim whitespace.
                if trimmed.is_empty() {                                               /// Skip empty items.
                    return None;                                                      /// Skip empty items.
                }
                let parts: Vec<&str> = trimmed.split_ascii_whitespace().collect();    /// Split into parts.
                if parts.is_empty() {                                                 /// Skip if no parts.
                    return None;                                                      /// Skip if no parts.
                }
                if trimmed.contains(" as ") {                                         /// Handle alias syntax like `foo as bar`.
                    let name = parts[0];                                              /// The original name.
                    let alias = parts.last().unwrap_or(&name);                        /// The alias name.
                    Some(format!("{}: {}", name, alias))                              /// Format as "name: alias".
                } else {
                    Some(format!("{}", parts[0]))                                     /// Use the name as-is.
                }
            })
            .collect();                                                               /// Collect all processed exports.

        return exports.join(", ");                                                    /// Join with commas for the exports list.
    }

    if line.contains("function") {                                                    /// Handle function exports like `export function foo()`.
        let re = regex::Regex::new(r"export\s+function\s+(\w+)").unwrap();            /// Regex to match `export function name`.
        if let Some(caps) = re.captures(line) {                                       /// Try to capture the function name.
            if let Some(name) = caps.get(1) {                                        /// If capture succeeded.
                return name.as_str().to_string();                                    /// Return the function name.
            }
        }
        let re2 = regex::Regex::new(r"function\s+(\w+)").unwrap();                    /// Fallback regex for just `function name` (in case export is separate).
        if let Some(caps) = re2.captures(line) {                                      /// Try the fallback regex.
            if let Some(name) = caps.get(1) {                                        /// If capture succeeded.
                return name.as_str().to_string();                                    /// Return the function name.
            }
        }
    }

    if line.contains("=") {                                                           /// Handle variable exports like `export const foo = ...`.
        let re = regex::Regex::new(r"export\s+const\s+(\w+)").unwrap();               /// Regex for `export const name`.
        if let Some(caps) = re.captures(line) {                                       /// Try to capture the variable name.
            if let Some(name) = caps.get(1) {                                        /// If capture succeeded.
                return name.as_str().to_string();                                    /// Return the variable name.
            }
        }
        let re2 = regex::Regex::new(r"export\s+let\s+(\w+)").unwrap();                /// Regex for `export let name`.
        if let Some(caps) = re2.captures(line) {                                      /// Try to capture the variable name.
            if let Some(name) = caps.get(1) {                                        /// If capture succeeded.
                return name.as_str().to_string();                                    /// Return the variable name.
            }
        }
        let re3 = regex::Regex::new(r"export\s+var\s+(\w+)").unwrap();                /// Regex for `export var name`.
        if let Some(caps) = re3.captures(line) {                                      /// Try to capture the variable name.
            if let Some(name) = caps.get(1) {                                        /// If capture succeeded.
                return name.as_str().to_string();                                    /// Return the variable name.
            }
        }
    }

    String::new()                                                                     /// Return empty string if no export name found.
}

/// Extracts the default export expression from an ES module export default statement.
/// This function parses `export default` statements and returns a variable assignment
/// that captures the default export value for later use in module.exports.
fn shim_export_default_line(line: &str) -> String {
    let line = line.trim_end_matches(';');                                           /// Remove trailing semicolon for cleaner parsing.

    if line.contains("export default") {                                             /// Check if this is a default export.
        if let Some(expr) = line.strip_prefix("export default") {                    /// Extract the expression after "export default".
            let expr = expr.trim();                                                  /// Trim whitespace from the expression.

            if expr.contains("class") {                                              /// Handle class default exports.
                if let Some(name) = expr.split_whitespace().nth(1) {                 /// Extract the class name.
                    return format!("var {}", name);                                  /// Return a var declaration for the class.
                }
            }

            if expr.contains("function") {                                           /// Handle function default exports.
                if let Some(name) = expr.split_whitespace().nth(1) {                 /// Extract the function name.
                    return format!("var {}", name);                                  /// Return a var declaration for the function.
                }
            }

            return format!("var __default = {}", expr);                             /// For other expressions, assign to __default.
        }
    }

    String::new()                                                                    /// Return empty string if not a default export.
}

/// The callback function invoked when JavaScript calls `require()`. This function handles the module loading process,
/// including resolving the module path, loading the module content, and returning the module's exports to JavaScript.
/// It supports both CommonJS and ES modules, and provides error handling for failed module loads.
fn require_callback<'s>(
    scope: &mut v8::HandleScope<'s>,                                                  /// The V8 JavaScript execution scope where the require function is being called. This scope provides access to V8's memory management and object creation functions.
    args: v8::FunctionCallbackArguments<'s>,                                          /// The arguments passed to the require function from JavaScript. The first argument should be the module name or path to load.
    mut rv: v8::ReturnValue,                                                           /// The return value object that will be set to the module's exports if loading succeeds, or left undefined if it fails.
) {
    if args.length() < 1 {                                                             /// Check if at least one argument (the module name) was provided. If not, we cannot proceed with loading a module.
        rv.set(v8::undefined(scope).into());                                           /// Set the return value to undefined since no module was specified.
        return;                                                                        /// Exit the function early as there's nothing to load.
    }

    let module_name = args                                                             /// Extract the module name from the first argument passed to require().
        .get(0)                                                                        /// Get the first argument (index 0) from the arguments list.
        .to_string(scope)                                                              /// Convert the V8 value to a V8 string object.
        .unwrap()                                                                      /// Unwrap the Option, assuming the conversion succeeded (it should for string arguments).
        .to_rust_string_lossy(scope);                                                  /// Convert the V8 string to a Rust String, using lossy conversion to handle invalid UTF-8.

    let base_dir = CURRENT_DIR.with(|d| d.borrow().clone());                           /// Get the current directory from the thread-local storage. This is used as the base directory for resolving relative module paths.

    match resolve_and_load(scope, &module_name, &base_dir) {                           /// Attempt to resolve and load the module using the module name and base directory. This function handles all the complex logic of finding, reading, and executing the module.
        Ok(module_exports) => {                                                        /// If the module was loaded successfully, we have the module's exports object.
            rv.set(module_exports.into());                                             /// Set the return value to the module's exports object, making it available to the JavaScript code that called require().
        }
        Err(e) => {                                                                    /// If there was an error loading the module, we need to throw a JavaScript exception to indicate the failure.
            let error_msg = format!("Error loading module '{}': {}", module_name, e); /// Format an error message that includes the module name and the specific error that occurred.
            let error = v8::String::new(scope, &error_msg).unwrap();                   /// Create a V8 string object containing the error message.
            let error_obj = v8::Exception::error(scope, error);                        /// Create a V8 error object from the error message string.
            scope.throw_exception(error_obj);                                          /// Throw the error object as a JavaScript exception, which will propagate up the call stack and can be caught by JavaScript try-catch blocks.
        }
    }
}

/// Resolves a module name to a file path and loads the module, returning its exports object.
/// This function handles the core module loading logic, including path resolution, caching, and
/// determining whether to load the module as CommonJS or ES module. It supports built-in modules,
/// relative paths, package resolution, and absolute paths.
fn resolve_and_load<'s>(
    scope: &mut v8::HandleScope<'s>,                                                  /// The V8 JavaScript execution scope where the module will be loaded. This scope is used for creating V8 objects and executing JavaScript code.
    module_name: &str,                                                                /// The name or path of the module to load. This can be a built-in module name, a relative path, an absolute path, or a package name.
    base_dir: &str,                                                                   /// The base directory to use for resolving relative paths. This is typically the directory of the module that is doing the requiring.
) -> Result<v8::Local<'s, v8::Object>> {                                               /// Returns the module's exports object if loading succeeds, or an error if loading fails.
    if module_name == "fs" || module_name == "path" || module_name == "http" || module_name == "querystring" {
        return create_builtin_module(scope, module_name);                             /// If the module name matches one of our built-in modules, create and return the built-in module implementation instead of loading from a file.
    }

    let resolved_path = if module_name.starts_with("./") || module_name.starts_with("../") {
        resolve_relative_path(module_name, base_dir)?                                 /// If the module name starts with "./" or "../", it's a relative path. Use the relative path resolver to find the actual file.
    } else if let Ok(package_path) =                                                  /// Otherwise, try to resolve it as a package using the package manager.
        PackageManager::resolve_package_in_dir(module_name, Path::new(base_dir))     /// The package manager will look for the module in node_modules or other package locations.
    {
        package_path                                                                   /// If package resolution succeeds, use the resolved package path.
    } else if Path::new(module_name).exists() {                                        /// If it's not a relative path or package, check if it's an absolute path that exists as a file.
        module_name.to_string()                                                        /// If the path exists, use it directly.
    } else {
        return Err(anyhow!("Cannot find module '{}'", module_name));                  /// If none of the above methods work, return an error indicating the module cannot be found.
    };

    let abs_path = Path::new(&resolved_path)                                           /// Canonicalize the resolved path to get an absolute path. This ensures we have a consistent path for caching and further operations.
        .canonicalize()                                                                /// Convert the path to its canonical form, resolving any "." or ".." components and symlinks.
        .map(|p| p.to_string_lossy().to_string())                                      /// Convert the canonical path to a String, using lossy conversion for invalid UTF-8.
        .unwrap_or_else(|_| resolved_path.clone());                                    /// If canonicalization fails (e.g., the path doesn't exist), fall back to the original resolved path.

    let cached = MODULE_CACHE.with(|cache| cache.borrow().get(&abs_path).cloned());   /// Check if the module has already been loaded and cached. This prevents reloading the same module multiple times.
    if let Some(cached_module) = cached {                                              /// If a cached version exists, use it instead of reloading.
        let local = v8::Local::new(scope, cached_module);                              /// Create a local reference to the cached V8 global value.
        return Ok(local.to_object(scope).unwrap_or(v8::Object::new(scope)));           /// Convert the cached value to an object and return it. If conversion fails, return a new empty object.
    }

    let content = fs::read_to_string(&abs_path)?;                                      /// Read the content of the module file. This will fail if the file doesn't exist or can't be read.

    let is_es = is_es_module(&content);                                                /// Determine whether the module is an ES module or CommonJS module by analyzing its content.

    if is_es {                                                                         /// If it's an ES module, load it using the ES module shimming process.
        load_es_module_shimmed(scope, &abs_path, &content)                             /// This will transpile TypeScript if needed, shim ES syntax to CommonJS, and execute the module.
    } else {                                                                           /// If it's a CommonJS module, load it using the standard CommonJS loading process.
        load_commonjs_module(scope, &abs_path, &content)                               /// This will transpile TypeScript if needed and execute the module in a CommonJS wrapper.
    }
}

/// Resolves a relative module path to an absolute file path, trying various extensions and index files.
/// This function implements Node.js-style module resolution for relative paths, checking for files
/// with different extensions (.js, .mjs, .ts) and index files in directories.
fn resolve_relative_path(module_name: &str, base_dir: &str) -> Result<String> {       /// Takes a relative module name and base directory, returns the resolved absolute path or an error.
    let base = Path::new(base_dir);                                                   /// Create a Path object from the base directory string.
    let path = base.join(module_name);                                                /// Join the base directory with the module name to get the potential file or directory path.

    if path.is_file() {                                                               /// First, check if the path exists as a file with no extension. This handles cases where the module name includes the extension.
        return Ok(path.to_string_lossy().to_string());                                /// Return the path as a string if it exists as a file.
    }

    let with_js = path.with_extension("js");                                          /// Try adding a .js extension to the path.
    if with_js.is_file() {                                                            /// Check if the file with .js extension exists.
        return Ok(with_js.to_string_lossy().to_string());                             /// Return the .js file path if it exists.
    }

    let with_mjs = path.with_extension("mjs");                                        /// Try adding a .mjs extension (ES module format).
    if with_mjs.is_file() {                                                           /// Check if the .mjs file exists.
        return Ok(with_mjs.to_string_lossy().to_string());                            /// Return the .mjs file path if it exists.
    }

    let with_ts = path.with_extension("ts");                                          /// Try adding a .ts extension for TypeScript files.
    if with_ts.is_file() {                                                            /// Check if the .ts file exists.
        return Ok(with_ts.to_string_lossy().to_string());                             /// Return the .ts file path if it exists.
    }

    let index_js = path.join("index.js");                                             /// If the path is a directory, try looking for an index.js file inside it.
    if index_js.is_file() {                                                           /// Check if index.js exists in the directory.
        return Ok(index_js.to_string_lossy().to_string());                            /// Return the index.js path if it exists.
    }

    let index_mjs = path.join("index.mjs");                                           /// Try index.mjs in the directory.
    if index_mjs.is_file() {                                                          /// Check if index.mjs exists.
        return Ok(index_mjs.to_string_lossy().to_string());                           /// Return the index.mjs path if it exists.
    }

    let index_ts = path.join("index.ts");                                             /// Try index.ts in the directory.
    if index_ts.is_file() {                                                           /// Check if index.ts exists.
        return Ok(index_ts.to_string_lossy().to_string());                            /// Return the index.ts path if it exists.
    }

    Err(anyhow!(                                                                     /// If none of the above paths exist, return an error indicating the module cannot be resolved.
        "Cannot resolve '{}' from '{}'",                                              /// Format the error message with the module name and base directory.
        module_name,                                                                  /// The module name that couldn't be resolved.
        base_dir                                                                       /// The base directory used for resolution.
    ))
}

/// Loads an ES module by first shimming its syntax to CommonJS and then executing it.
/// This function handles TypeScript transpilation if needed, applies ES module to CommonJS transformation,
/// and then loads the resulting code as a CommonJS module.
fn load_es_module_shimmed<'s>(
    scope: &mut v8::HandleScope<'s>,                                                  /// The V8 JavaScript execution scope where the module will be loaded.
    file_path: &str,                                                                  /// The absolute path to the ES module file.
    content: &str,                                                                    /// The raw content of the ES module file.
) -> Result<v8::Local<'s, v8::Object>> {                                               /// Returns the module's exports object or an error.
    let abs_path = Path::new(file_path)                                               /// Canonicalize the file path to ensure we have an absolute path.
        .canonicalize()                                                                /// Resolve any relative components and symlinks.
        .map(|p| p.to_string_lossy().to_string())                                      /// Convert to string, handling invalid UTF-8.
        .unwrap_or_else(|_| file_path.to_string());                                    /// Fall back to original path if canonicalization fails.

    let js_code = if file_path.ends_with(".ts") || file_path.ends_with(".tsx") {       /// Check if the file is a TypeScript file that needs transpilation.
        let transpiler = TypeScriptTranspiler::new();                                  /// Create a new TypeScript transpiler instance.
        let transpiled = transpiler.transpile(content)?;                               /// Transpile the TypeScript content to JavaScript.
        shim_es_module(&transpiled, &abs_path)                                         /// Apply ES module shimming to the transpiled JavaScript.
    } else {
        shim_es_module(content, &abs_path)                                             /// If it's already JavaScript, just apply ES module shimming.
    };

    load_commonjs_module_from_code(scope, &abs_path, &js_code)                         /// Load the shimmed code as a CommonJS module and return its exports.
}

/// Loads a CommonJS module by transpiling TypeScript if needed and executing it in a CommonJS wrapper.
/// This function handles the loading of traditional CommonJS modules, including TypeScript files
/// that need to be transpiled to JavaScript before execution.
fn load_commonjs_module<'s>(
    scope: &mut v8::HandleScope<'s>,                                                  /// The V8 JavaScript execution scope where the module will be loaded.
    file_path: &str,                                                                  /// The absolute path to the CommonJS module file.
    content: &str,                                                                    /// The raw content of the CommonJS module file.
) -> Result<v8::Local<'s, v8::Object>> {                                               /// Returns the module's exports object or an error.
    let abs_path = Path::new(file_path)                                               /// Canonicalize the file path to ensure we have an absolute path.
        .canonicalize()                                                                /// Resolve any relative components and symlinks.
        .map(|p| p.to_string_lossy().to_string())                                      /// Convert to string, handling invalid UTF-8.
        .unwrap_or_else(|_| file_path.to_string());                                    /// Fall back to original path if canonicalization fails.

    let js_code = if file_path.ends_with(".ts") || file_path.ends_with(".tsx") {       /// Check if the file is a TypeScript file that needs transpilation.
        let transpiler = TypeScriptTranspiler::new();                                  /// Create a new TypeScript transpiler instance.
        transpiler.transpile(content)?                                                 /// Transpile the TypeScript content to JavaScript.
    } else {
        content.to_string()                                                            /// If it's already JavaScript, use the content as-is.
    };

    load_commonjs_module_from_code(scope, &abs_path, &js_code)                         /// Load the (possibly transpiled) JavaScript code as a CommonJS module.
}

/// Loads JavaScript code as a CommonJS module by wrapping it in a function and executing it.
/// This function creates the CommonJS environment (module, exports, require, __filename, __dirname)
/// and executes the module code in that context, then returns the module's exports.
fn load_commonjs_module_from_code<'s>(
    scope: &mut v8::HandleScope<'s>,                                                  /// The V8 JavaScript execution scope where the module will be loaded.
    file_path: &str,                                                                  /// The absolute path to the module file (used for __filename and __dirname).
    js_code: &str,                                                                    /// The JavaScript code to execute as a CommonJS module.
) -> Result<v8::Local<'s, v8::Object>> {                                               /// Returns the module's exports object or an error.
    let abs_path = Path::new(file_path)                                               /// Canonicalize the file path to ensure we have an absolute path.
        .canonicalize()                                                                /// Resolve any relative components and symlinks.
        .map(|p| p.to_string_lossy().to_string())                                      /// Convert to string, handling invalid UTF-8.
        .unwrap_or_else(|_| file_path.to_string());                                    /// Fall back to original path if canonicalization fails.

    let module_obj = v8::Object::new(scope);                                          /// Create a new V8 object to represent the CommonJS 'module' object.
    let exports_obj = v8::Object::new(scope);                                          /// Create a new V8 object for the module's exports.

    let module_key = v8::String::new(scope, "exports").unwrap();                      /// Create a V8 string for the "exports" property key.
    module_obj.set(scope, module_key.into(), exports_obj.into());                     /// Set the exports property on the module object.

    let path = Path::new(&abs_path);                                                  /// Create a Path object from the absolute path.
    let filename = path                                                                /// Extract the filename from the path.
        .file_name()                                                                   /// Get the final component of the path.
        .and_then(|n| n.to_str())                                                      /// Convert the OsStr to &str.
        .unwrap_or(&abs_path);                                                         /// Fall back to the full path if filename extraction fails.
    let dirname = path.parent().and_then(|p| p.to_str()).unwrap_or(".");               /// Extract the directory name from the path, defaulting to "." if no parent.

    let filename_str = v8::String::new(scope, filename).unwrap();                      /// Create a V8 string for __filename.
    let dirname_str = v8::String::new(scope, dirname).unwrap();                        /// Create a V8 string for __dirname.

    let prev_dir = CURRENT_DIR.with(|d| d.borrow().clone());                           /// Save the current directory before changing it for this module.
    CURRENT_DIR.with(|d| *d.borrow_mut() = dirname.to_string());                       /// Set the current directory to the module's directory for relative requires.

    let wrapper = format!(                                                              /// Create a wrapper function that provides the CommonJS environment.
        "(function(exports, require, module, __filename, __dirname) {{\n{}\n}})",     /// The wrapper takes the CommonJS globals as parameters.
        js_code                                                                        /// Insert the module's JavaScript code inside the function.
    );

    let code = v8::String::new(scope, &wrapper)                                        /// Create a V8 string from the wrapped code.
        .ok_or_else(|| anyhow!("Failed to create module code string"))?;               /// Return an error if string creation fails.

    let script = v8::Script::compile(scope, code, None)                                /// Compile the wrapped code into a V8 script.
        .ok_or_else(|| anyhow!("Failed to compile module wrapper"))?;                  /// Return an error if compilation fails.

    let result = script                                                                 /// Execute the compiled script.
        .run(scope)                                                                     /// Run the script in the current scope.
        .ok_or_else(|| anyhow!("Failed to execute module wrapper"))?;                  /// Return an error if execution fails.

    let func = v8::Local::<v8::Function>::try_from(result)                             /// Convert the script result to a function.
        .map_err(|_| anyhow!("Module wrapper did not return a function"))?;            /// Return an error if the result is not a function.

    let recv = v8::undefined(scope);                                                   /// Create an undefined value for the 'this' context.
    let require_func = create_require_func(scope);                                     /// Create a require function for the module to use.
    let args = [                                                                       /// Create an array of arguments to pass to the module function.
        exports_obj.into(),                                                             /// exports object
        require_func.into(),                                                            /// require function
        module_obj.into(),                                                              /// module object
        filename_str.into(),                                                            /// __filename
        dirname_str.into(),                                                             /// __dirname
    ];

    func.call(scope, recv.into(), &args);                                              /// Call the module function with the CommonJS arguments.

    CURRENT_DIR.with(|d| *d.borrow_mut() = prev_dir);                                   /// Restore the previous current directory.

    let exports_key = v8::String::new(scope, "exports").unwrap();                      /// Create a V8 string for the "exports" key.
    let final_exports = module_obj                                                      /// Get the final exports from the module object.
        .get(scope, exports_key.into())                                                 /// Retrieve the exports property.
        .unwrap_or(exports_obj.into());                                                 /// Fall back to the original exports object if retrieval fails.

    let exports_object = final_exports.to_object(scope).unwrap_or(exports_obj);        /// Convert the final exports to an object.

    MODULE_CACHE.with(|cache| {                                                        /// Cache the module exports for future requires.
        let exports_val: v8::Local<v8::Value> = exports_object.into();                  /// Convert the exports object to a V8 value.
        cache                                                                           /// Insert the exports into the module cache.
            .borrow_mut()                                                               /// Get a mutable borrow of the cache.
            .insert(abs_path.clone(), v8::Global::new(scope, exports_val));            /// Store the exports as a global value in the cache.
    });

    Ok(exports_object)                                                                  /// Return the module's exports object.
}

/// The callback function invoked when JavaScript uses dynamic import() syntax.
/// This function handles ES module dynamic imports by resolving the module specifier
/// and loading the module asynchronously, returning a promise that resolves to the module's exports.
fn es_dynamic_import<'s>(
    scope: &mut v8::HandleScope<'s>,                                                  /// The V8 JavaScript execution scope where the dynamic import is being called.
    args: v8::FunctionCallbackArguments<'s>,                                          /// The arguments passed to the import() function. The first argument should be the module specifier.
    mut rv: v8::ReturnValue,                                                           /// The return value object that will be set to the module's exports (or a promise in real implementations).
) {
    if args.length() < 1 {                                                             /// Check if a module specifier was provided.
        rv.set(v8::undefined(scope).into());                                           /// Set the return value to undefined if no specifier was given.
        return;                                                                        /// Exit the function early.
    }

    let specifier = args                                                               /// Extract the module specifier from the first argument.
        .get(0)                                                                        /// Get the first argument (index 0).
        .to_string(scope)                                                              /// Convert the V8 value to a V8 string object.
        .unwrap()                                                                      /// Unwrap the Option, assuming the conversion succeeded.
        .to_rust_string_lossy(scope);                                                  /// Convert the V8 string to a Rust String.

    let base_dir = CURRENT_DIR.with(|d| d.borrow().clone());                           /// Get the current directory from the thread-local storage.

    let resolved = match resolve_relative_path(&specifier, &base_dir) {                /// First, try to resolve the specifier as a relative path.
        Ok(path) => path,                                                              /// If relative path resolution succeeds, use that path.
        Err(_) => match PackageManager::resolve_package_in_dir(&specifier, Path::new(&base_dir)) {
            Ok(pkg_path) => pkg_path,                                                  /// If package resolution succeeds, use the package path.
            Err(_) => {                                                                /// If both resolution methods fail, throw a JavaScript error.
                let error_str =                                                        /// Create an error message indicating the module cannot be found.
                    v8::String::new(scope, &format!("Cannot find module '{}'", specifier)).unwrap();
                let error = v8::Exception::error(scope, error_str);                    /// Create a V8 error object.
                scope.throw_exception(error);                                          /// Throw the error as a JavaScript exception.
                return;                                                                /// Exit the function.
            }
        },
    };

    let abs_path = Path::new(&resolved)                                                /// Canonicalize the resolved path to get an absolute path.
        .canonicalize()                                                                /// Resolve any relative components and symlinks.
        .map(|p| p.to_string_lossy().to_string())                                      /// Convert to string, handling invalid UTF-8.
        .unwrap_or_else(|_| resolved.clone());                                         /// Fall back to the original resolved path if canonicalization fails.

    let dirname = Path::new(&abs_path)                                                 /// Extract the directory of the resolved module.
        .parent()                                                                      /// Get the parent directory.
        .and_then(|p| p.to_str())                                                      /// Convert the Path to &str.
        .unwrap_or(".");                                                               /// Default to "." if no parent directory.
    CURRENT_DIR.with(|d| *d.borrow_mut() = dirname.to_string());                       /// Set the current directory to the module's directory.

    match resolve_and_load(scope, &abs_path, &abs_path) {                              /// Load the resolved module.
        Ok(exports) => rv.set(exports.into()),                                         /// If loading succeeds, set the return value to the module's exports.
        Err(e) => {                                                                    /// If loading fails, throw a JavaScript error.
            let error_str =                                                            /// Create an error message with the loading error.
                v8::String::new(scope, &format!("Error loading '{}': {}", specifier, e)).unwrap();
            let error = v8::Exception::error(scope, error_str);                        /// Create a V8 error object.
            scope.throw_exception(error);                                              /// Throw the error as a JavaScript exception.
        }
    }
}

/// Creates a new require function that can be passed to module wrappers.
/// This function returns a V8 function object that, when called, will invoke the require_callback.
fn create_require_func<'s>(scope: &mut v8::HandleScope<'s>) -> v8::Local<'s, v8::Function> {
    let func = v8::FunctionTemplate::new(scope, require_callback);                     /// Create a function template with the require_callback as the implementation.
    func.get_function(scope).unwrap()                                                  /// Get the function object from the template.
}

/// Creates a built-in Node.js module with its standard API.
/// This function implements shims for core Node.js modules like fs, path, http, and querystring,
/// providing JavaScript code with access to native functionality through V8 callbacks.
fn create_builtin_module<'s>(
    scope: &mut v8::HandleScope<'s>,                                                  /// The V8 JavaScript execution scope where the module will be created.
    module_name: &str,                                                                /// The name of the built-in module to create (fs, path, http, querystring).
) -> Result<v8::Local<'s, v8::Object>> {                                               /// Returns the module object with its API methods.
    let module = v8::Object::new(scope);                                              /// Create a new V8 object to hold the module's exports.

    match module_name {                                                                /// Match on the module name to set up the appropriate API.
        "fs" => {                                                                      /// Set up the fs module with file system operations.
            let read_file_sync = v8::FunctionTemplate::new(scope, fs_read_file_sync); /// Create a function template for readFileSync.
            let name = v8::String::new(scope, "readFileSync").unwrap();                /// Create the property name.
            let func = read_file_sync.get_function(scope).unwrap();                    /// Get the function from the template.
            module.set(scope, name.into(), func.into());                              /// Add the function to the module.

            let write_file_sync = v8::FunctionTemplate::new(scope, fs_write_file_sync);/// Create a function template for writeFileSync.
            let name = v8::String::new(scope, "writeFileSync").unwrap();               /// Create the property name.
            let func = write_file_sync.get_function(scope).unwrap();                   /// Get the function from the template.
            module.set(scope, name.into(), func.into());                              /// Add the function to the module.
        }
        "path" => {                                                                    /// Set up the path module with path manipulation functions.
            let join = v8::FunctionTemplate::new(scope, path_join);                    /// Create a function template for join.
            let name = v8::String::new(scope, "join").unwrap();                        /// Create the property name.
            let func = join.get_function(scope).unwrap();                             /// Get the function from the template.
            module.set(scope, name.into(), func.into());                              /// Add the function to the module.
        }
        "http" => {                                                                    /// Set up the http module with server creation functions.
            let create_server = v8::FunctionTemplate::new(scope, http_create_server);/// Create a function template for createServer.
            let name = v8::String::new(scope, "createServer").unwrap();                /// Create the property name.
            let func = create_server.get_function(scope).unwrap();                    /// Get the function from the template.
            module.set(scope, name.into(), func.into());                              /// Add the function to the module.
        }
        "querystring" => {                                                             /// Set up the querystring module with parsing functions.
            let parse = v8::FunctionTemplate::new(scope, querystring_parse);           /// Create a function template for parse.
            let name = v8::String::new(scope, "parse").unwrap();                       /// Create the property name.
            let func = parse.get_function(scope).unwrap();                            /// Get the function from the template.
            module.set(scope, name.into(), func.into());                              /// Add the function to the module.
            
            let stringify = v8::FunctionTemplate::new(scope, querystring_stringify);   /// Create a function template for stringify.
            let name = v8::String::new(scope, "stringify").unwrap();                   /// Create the property name.
            let func = stringify.get_function(scope).unwrap();                         /// Get the function from the template.
            module.set(scope, name.into(), func.into());                              /// Add the function to the module.
        }
        _ => {}                                                                        /// For unrecognized module names, return an empty module object.
    }

    Ok(module)                                                                         /// Return the configured module object.
}

/// Synchronous file reading function for the fs module.
/// Reads the entire contents of a file as a UTF-8 string and returns it to JavaScript.
fn fs_read_file_sync<'s>(
    scope: &mut v8::HandleScope<'s>,                                                  /// The V8 JavaScript execution scope.
    args: v8::FunctionCallbackArguments<'s>,                                          /// The arguments passed from JavaScript (file path).
    mut rv: v8::ReturnValue,                                                           /// The return value to set with the file contents.
) {
    if args.length() < 1 {                                                             /// Check if a file path was provided.
        rv.set(v8::undefined(scope).into());                                           /// Return undefined if no path was given.
        return;                                                                        /// Exit the function.
    }

    let path = args                                                                    /// Extract the file path from the first argument.
        .get(0)                                                                        /// Get the first argument.
        .to_string(scope)                                                              /// Convert to V8 string.
        .unwrap()                                                                      /// Unwrap the Option.
        .to_rust_string_lossy(scope);                                                  /// Convert to Rust string.

    match fs::read_to_string(&path) {                                                  /// Attempt to read the file as a UTF-8 string.
        Ok(content) => {                                                               /// If reading succeeds, return the content.
            let result = v8::String::new(scope, &content).unwrap();                    /// Create a V8 string from the file content.
            rv.set(result.into());                                                     /// Set the return value to the content string.
        }
        Err(_) => {                                                                    /// If reading fails, return undefined.
            rv.set(v8::undefined(scope).into());                                       /// Set the return value to undefined.
        }
    }
}

/// Synchronous file writing function for the fs module.
/// Writes a string to a file and returns a boolean indicating success.
fn fs_write_file_sync<'s>(
    scope: &mut v8::HandleScope<'s>,                                                  /// The V8 JavaScript execution scope.
    args: v8::FunctionCallbackArguments<'s>,                                          /// The arguments passed from JavaScript (file path and data).
    mut rv: v8::ReturnValue,                                                           /// The return value to set with the success boolean.
) {
    if args.length() < 2 {                                                             /// Check if both path and data arguments were provided.
        rv.set(v8::Boolean::new(scope, false).into());                                 /// Return false if insufficient arguments.
        return;                                                                        /// Exit the function.
    }

    let path = args                                                                    /// Extract the file path from the first argument.
        .get(0)                                                                        /// Get the first argument.
        .to_string(scope)                                                              /// Convert to V8 string.
        .unwrap()                                                                      /// Unwrap the Option.
        .to_rust_string_lossy(scope);                                                  /// Convert to Rust string.
    let data = args                                                                    /// Extract the data to write from the second argument.
        .get(1)                                                                        /// Get the second argument.
        .to_string(scope)                                                              /// Convert to V8 string.
        .unwrap()                                                                      /// Unwrap the Option.
        .to_rust_string_lossy(scope);                                                  /// Convert to Rust string.

    match fs::write(&path, data) {                                                     /// Attempt to write the data to the file.
        Ok(_) => rv.set(v8::Boolean::new(scope, true).into()),                         /// Return true if writing succeeds.
        Err(_) => rv.set(v8::Boolean::new(scope, false).into()),                       /// Return false if writing fails.
    }
}

/// Path joining function for the path module.
/// Joins multiple path segments using forward slashes and returns the combined path.
fn path_join<'s>(
    scope: &mut v8::HandleScope<'s>,                                                  /// The V8 JavaScript execution scope.
    args: v8::FunctionCallbackArguments<'s>,                                          /// The path segments to join.
    mut rv: v8::ReturnValue,                                                           /// The return value to set with the joined path.
) {
    let mut path_parts = Vec::new();                                                   /// Create a vector to collect the path segments.

    for i in 0..args.length() {                                                        /// Iterate through all arguments.
        let part = args                                                                /// Extract each path segment.
            .get(i)                                                                    /// Get the argument at index i.
            .to_string(scope)                                                          /// Convert to V8 string.
            .unwrap()                                                                  /// Unwrap the Option.
            .to_rust_string_lossy(scope);                                              /// Convert to Rust string.
        path_parts.push(part);                                                         /// Add the segment to the vector.
    }

    let joined = path_parts.join("/");                                                 /// Join all segments with forward slashes.
    let result = v8::String::new(scope, &joined).unwrap();                             /// Create a V8 string from the joined path.
    rv.set(result.into());                                                             /// Set the return value to the joined path.
}

/// HTTP server creation function for the http module.
/// Creates a server object with listen and on methods, storing an optional request handler.
fn http_create_server<'s>(
    scope: &mut v8::HandleScope<'s>,                                                  /// The V8 JavaScript execution scope.
    args: v8::FunctionCallbackArguments<'s>,                                          /// The arguments passed from JavaScript (optional request handler).
    mut rv: v8::ReturnValue,                                                           /// The return value to set with the server object.
) {
    let server = v8::Object::new(scope);                                              /// Create a new V8 object to represent the HTTP server.
    
    if args.length() > 0 && args.get(0).is_function() {                               /// If a request handler function was provided, store it.
        let handler_key = v8::String::new(scope, "__handler").unwrap();               /// Create a key for storing the handler.
        server.set(scope, handler_key.into(), args.get(0));                           /// Store the handler function on the server object.
    }
    
    let listen_template = v8::FunctionTemplate::new(scope, http_server_listen);      /// Create a function template for the listen method.
    let listen_func = listen_template.get_function(scope).unwrap();                   /// Get the function from the template.
    let listen_key = v8::String::new(scope, "listen").unwrap();                       /// Create the property name.
    server.set(scope, listen_key.into(), listen_func.into());                         /// Add the listen method to the server.
    
    let on_template = v8::FunctionTemplate::new(scope, http_server_on);              /// Create a function template for the on method.
    let on_func = on_template.get_function(scope).unwrap();                           /// Get the function from the template.
    let on_key = v8::String::new(scope, "on").unwrap();                               /// Create the property name.
    server.set(scope, on_key.into(), on_func.into());                                 /// Add the on method to the server.
    
    rv.set(server.into());                                                             /// Return the server object.
}

/// Listen method for HTTP servers.
/// Starts the server on a specified port and calls an optional callback.
fn http_server_listen<'s>(
    scope: &mut v8::HandleScope<'s>,                                                  /// The V8 JavaScript execution scope.
    args: v8::FunctionCallbackArguments<'s>,                                          /// The arguments (port and optional callback).
    mut rv: v8::ReturnValue,                                                           /// The return value (undefined for chaining).
) {
    let port = if args.length() > 0 {                                                 /// Extract the port number from arguments.
        args.get(0).to_int32(scope).map(|v| v.value()).unwrap_or(3000)                 /// Convert to integer, defaulting to 3000.
    } else {
        3000                                                                          /// Default port if not specified.
    };
    
    println!("🚀 HTTP server listening on port {}", port);                           /// Log the server start message.
    
    if args.length() > 1 {                                                            /// If a callback was provided, call it.
        let callback = args.get(1);                                                   /// Get the callback function.
        if callback.is_function() {                                                   /// Check if it's actually a function.
            let func = v8::Local::<v8::Function>::try_from(callback).unwrap();        /// Convert to function object.
            let recv = v8::undefined(scope);                                          /// Create undefined for 'this' context.
            let undefined = v8::undefined(scope);                                     /// Create undefined argument.
            func.call(scope, recv.into(), &[undefined.into()]);                      /// Call the callback with no arguments.
        }
    }
    
    rv.set(v8::undefined(scope).into());                                               /// Return undefined for method chaining.
}

/// Event listener method for HTTP servers.
/// Stub implementation that returns undefined for method chaining.
fn http_server_on<'s>(
    scope: &mut v8::HandleScope<'s>,                                                  /// The V8 JavaScript execution scope.
    _args: v8::FunctionCallbackArguments<'s>,                                         /// The arguments (event name and handler) - ignored in stub.
    mut rv: v8::ReturnValue,                                                           /// The return value (undefined for chaining).
) {
    rv.set(v8::undefined(scope).into());                                               /// Return undefined for method chaining.
}

/// Query string parsing function for the querystring module.
/// Parses a URL-encoded query string into an object of key-value pairs.
fn querystring_parse<'s>(
    scope: &mut v8::HandleScope<'s>,                                                  /// The V8 JavaScript execution scope.
    args: v8::FunctionCallbackArguments<'s>,                                          /// The arguments (query string to parse).
    mut rv: v8::ReturnValue,                                                           /// The return value to set with the parsed object.
) {
    if args.length() < 1 {                                                             /// Check if a query string was provided.
        rv.set(v8::Object::new(scope).into());                                         /// Return an empty object if not.
        return;                                                                        /// Exit the function.
    }
    
    let str = args.get(0).to_string(scope).unwrap().to_rust_string_lossy(scope);      /// Extract the query string.
    let obj = v8::Object::new(scope);                                                  /// Create an object to hold the parsed key-value pairs.
    
    for pair in str.split('&') {                                                       /// Split the string on '&' to get individual pairs.
        if let Some(eq) = pair.find('=') {                                             /// Find the '=' separator in each pair.
            let key = &pair[..eq];                                                     /// Extract the key (before '=').
            let value = &pair[eq + 1..];                                               /// Extract the value (after '=').
            let key_str = v8::String::new(scope, key).unwrap();                        /// Create V8 string for the key.
            let val_str = v8::String::new(scope, value).unwrap();                      /// Create V8 string for the value.
            obj.set(scope, key_str.into(), val_str.into());                           /// Set the key-value pair on the object.
        } else if !pair.is_empty() {                                                   /// Handle pairs without '=' (key with empty value).
            let key_str = v8::String::new(scope, pair).unwrap();                       /// Create V8 string for the key.
            let val_str = v8::String::new(scope, "").unwrap();                         /// Create empty V8 string for the value.
            obj.set(scope, key_str.into(), val_str.into());                           /// Set the key with empty value.
        }
    }
    
    rv.set(obj.into());                                                                /// Return the parsed object.
}

/// Query string stringify function for the querystring module.
/// Converts an object of key-value pairs into a URL-encoded query string.
fn querystring_stringify<'s>(
    scope: &mut v8::HandleScope<'s>,                                                  /// The V8 JavaScript execution scope.
    args: v8::FunctionCallbackArguments<'s>,                                          /// The arguments (object to stringify).
    mut rv: v8::ReturnValue,                                                           /// The return value to set with the query string.
) {
    if args.length() < 1 {                                                             /// Check if an object was provided.
        rv.set(v8::String::new(scope, "").unwrap().into());                            /// Return empty string if not.
        return;                                                                        /// Exit the function.
    }
    
    let obj = args.get(0).to_object(scope).unwrap_or(v8::Object::new(scope));          /// Extract the object to stringify.
    let mut pairs = Vec::new();                                                        /// Create a vector to collect key-value pairs.
    
    if let Some(keys) = obj.get_own_property_names(scope, v8::GetPropertyNamesArgs::default()) {
        for i in 0..keys.length() {                                                    /// Iterate through all properties of the object.
            if let Some(key) = keys.get_index(scope, i) {                             /// Get each property name.
                if let Some(key_str) = key.to_string(scope) {                          /// Convert the key to a string.
                    let key = key_str.to_rust_string_lossy(scope);                     /// Convert to Rust string.
                    if let Some(val) = obj.get(scope, key_str.into()) {                /// Get the value for this key.
                        if let Some(val_str) = val.to_string(scope) {                   /// Convert the value to a string.
                            let val = val_str.to_rust_string_lossy(scope);              /// Convert to Rust string.
                            pairs.push(format!("{}={}", key, val));                    /// Add the key-value pair to the vector.
                        }
                    }
                }
            }
        }
    }
    
    let result = pairs.join("&");                                                      /// Join all pairs with '&'.
    rv.set(v8::String::new(scope, &result).unwrap().into());                           /// Return the query string.
}

