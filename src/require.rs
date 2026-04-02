use anyhow::{anyhow, Result};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use v8;

use crate::package_manager::PackageManager;
use crate::typescript::TypeScriptTranspiler;

thread_local! {
    static MODULE_CACHE: RefCell<HashMap<String, v8::Global<v8::Value>>> = RefCell::new(HashMap::new());
    static CURRENT_DIR: RefCell<String> = RefCell::new(".".to_string());
}

pub fn set_current_dir(dir: &str) {
    CURRENT_DIR.with(|d| *d.borrow_mut() = dir.to_string());
}

pub struct RequireAPI;

impl RequireAPI {
    pub fn init<'s>(scope: &mut v8::HandleScope<'s>, global: v8::Local<'s, v8::Object>) {
        let name = v8::String::new(scope, "require").unwrap();
        let func = v8::FunctionTemplate::new(scope, require_callback);
        let func = func.get_function(scope).unwrap();
        global.set(scope, name.into(), func.into());

        let dirname = v8::String::new(scope, ".").unwrap();
        let dirname_key = v8::String::new(scope, "__dirname").unwrap();
        global.set(scope, dirname_key.into(), dirname.into());

        let filename = v8::String::new(scope, "<main>").unwrap();
        let filename_key = v8::String::new(scope, "__filename").unwrap();
        global.set(scope, filename_key.into(), filename.into());

        let import_func = v8::FunctionTemplate::new(scope, es_dynamic_import);
        let import_key = v8::String::new(scope, "__dynamicImport").unwrap();
        let import_fn = import_func.get_function(scope).unwrap();
        global.set(scope, import_key.into(), import_fn.into());
    }
}

fn is_es_module(content: &str) -> bool {
    let trimmed = content.trim();
    let starts_with_comment_or_empty =
        trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("/*");

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with("/*") {
            continue;
        }
        if line.starts_with("import ") || line.starts_with("export ") {
            if content.contains("module.exports") || content.contains("require(") {
                return false;
            }
            return true;
        }
        if line.starts_with("var ") || line.starts_with("const ") || line.starts_with("let ") {
            continue;
        }
        if !starts_with_comment_or_empty {
            break;
        }
    }
    false
}

fn shim_es_module(content: &str, file_path: &str) -> String {
    let mut result = String::new();
    let dirname = Path::new(file_path)
        .parent()
        .map(|p| p.to_string_lossy())
        .unwrap_or_default()
        .to_string();
    let mut named_exports: Vec<String> = Vec::new();
    let mut default_export: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("import ") {
            let shimmed = shim_import_line(trimmed, &dirname);
            result.push_str(&shimmed);
            result.push('\n');
        } else if trimmed.starts_with("export default") {
            let shimmed = shim_export_default_line(trimmed);
            default_export = Some(shimmed);
            result.push_str("// default export: ");
            result.push_str(&trimmed.replace("export default", "var"));
            result.push('\n');
        } else if trimmed.starts_with("export ") {
            let shimmed = shim_export_line(trimmed);
            if !shimmed.starts_with("/* skipped:") {
                named_exports.push(shimmed);
            }
            result.push_str(&trimmed.replace("export ", ""));
            result.push('\n');
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }

    if !named_exports.is_empty() {
        result.push_str("\nmodule.exports = { ");
        result.push_str(&named_exports.join(", "));
        result.push_str(" };\n");
    }

    if let Some(default) = default_export {
        if !named_exports.is_empty() {
            result.push_str("module.exports.default = ");
        }
        result.push_str(&default);
        result.push('\n');
    }

    result
}

fn shim_import_line(line: &str, dirname: &str) -> String {
    let line = line.trim_end_matches(';');

    if line.contains('{') && line.contains('}') {
        let between_braces = line
            .split('{')
            .nth(1)
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

fn shim_export_line(line: &str) -> String {
    let line = line.trim_end_matches(';').trim();

    if line.starts_with("export {") {
        let start = line.find("{").unwrap_or(line.len());
        let end = line.rfind("}").unwrap_or(line.len());
        let between_braces = &line[start + 1..end];

        let exports: Vec<String> = between_braces
            .split(',')
            .filter_map(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    return None;
                }
                let parts: Vec<&str> = trimmed.split_ascii_whitespace().collect();
                if parts.is_empty() {
                    return None;
                }
                if trimmed.contains(" as ") {
                    let name = parts[0];
                    let alias = parts.last().unwrap_or(&name);
                    Some(format!("{}: {}", name, alias))
                } else {
                    Some(format!("{}", parts[0]))
                }
            })
            .collect();

        return exports.join(", ");
    }

    if line.contains("function") {
        let re = regex::Regex::new(r"export\s+function\s+(\w+)").unwrap();
        if let Some(caps) = re.captures(line) {
            if let Some(name) = caps.get(1) {
                return name.as_str().to_string();
            }
        }
        let re2 = regex::Regex::new(r"function\s+(\w+)").unwrap();
        if let Some(caps) = re2.captures(line) {
            if let Some(name) = caps.get(1) {
                return name.as_str().to_string();
            }
        }
    }

    if line.contains("=") {
        let re = regex::Regex::new(r"export\s+const\s+(\w+)").unwrap();
        if let Some(caps) = re.captures(line) {
            if let Some(name) = caps.get(1) {
                return name.as_str().to_string();
            }
        }
        let re2 = regex::Regex::new(r"export\s+let\s+(\w+)").unwrap();
        if let Some(caps) = re2.captures(line) {
            if let Some(name) = caps.get(1) {
                return name.as_str().to_string();
            }
        }
        let re3 = regex::Regex::new(r"export\s+var\s+(\w+)").unwrap();
        if let Some(caps) = re3.captures(line) {
            if let Some(name) = caps.get(1) {
                return name.as_str().to_string();
            }
        }
    }

    String::new()
}

fn shim_export_default_line(line: &str) -> String {
    let line = line.trim_end_matches(';');

    if line.contains("export default") {
        if let Some(expr) = line.strip_prefix("export default") {
            let expr = expr.trim();

            if expr.contains("class") {
                if let Some(name) = expr.split_whitespace().nth(1) {
                    return format!("var {}", name);
                }
            }

            if expr.contains("function") {
                if let Some(name) = expr.split_whitespace().nth(1) {
                    return format!("var {}", name);
                }
            }

            return format!("var __default = {}", expr);
        }
    }

    String::new()
}

fn require_callback<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    if args.length() < 1 {
        rv.set(v8::undefined(scope).into());
        return;
    }

    let module_name = args
        .get(0)
        .to_string(scope)
        .unwrap()
        .to_rust_string_lossy(scope);

    let base_dir = CURRENT_DIR.with(|d| d.borrow().clone());

    match resolve_and_load(scope, &module_name, &base_dir) {
        Ok(module_exports) => {
            rv.set(module_exports.into());
        }
        Err(e) => {
            let error_msg = format!("Error loading module '{}': {}", module_name, e);
            let error = v8::String::new(scope, &error_msg).unwrap();
            let error_obj = v8::Exception::error(scope, error);
            scope.throw_exception(error_obj);
        }
    }
}

fn resolve_and_load<'s>(
    scope: &mut v8::HandleScope<'s>,
    module_name: &str,
    base_dir: &str,
) -> Result<v8::Local<'s, v8::Object>> {
    if module_name == "fs" || module_name == "path" || module_name == "http" || module_name == "querystring" {
        return create_builtin_module(scope, module_name);
    }

    let resolved_path = if module_name.starts_with("./") || module_name.starts_with("../") {
        resolve_relative_path(module_name, base_dir)?
    } else if let Ok(package_path) =
        PackageManager::resolve_package_in_dir(module_name, Path::new(base_dir))
    {
        package_path
    } else if Path::new(module_name).exists() {
        module_name.to_string()
    } else {
        return Err(anyhow!("Cannot find module '{}'", module_name));
    };

    let abs_path = Path::new(&resolved_path)
        .canonicalize()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| resolved_path.clone());

    let cached = MODULE_CACHE.with(|cache| cache.borrow().get(&abs_path).cloned());
    if let Some(cached_module) = cached {
        let local = v8::Local::new(scope, cached_module);
        return Ok(local.to_object(scope).unwrap_or(v8::Object::new(scope)));
    }

    let content = fs::read_to_string(&abs_path)?;

    let is_es = is_es_module(&content);

    if is_es {
        load_es_module_shimmed(scope, &abs_path, &content)
    } else {
        load_commonjs_module(scope, &abs_path, &content)
    }
}

fn resolve_relative_path(module_name: &str, base_dir: &str) -> Result<String> {
    let base = Path::new(base_dir);
    let path = base.join(module_name);

    if path.is_file() {
        return Ok(path.to_string_lossy().to_string());
    }

    let with_js = path.with_extension("js");
    if with_js.is_file() {
        return Ok(with_js.to_string_lossy().to_string());
    }

    let with_mjs = path.with_extension("mjs");
    if with_mjs.is_file() {
        return Ok(with_mjs.to_string_lossy().to_string());
    }

    let with_ts = path.with_extension("ts");
    if with_ts.is_file() {
        return Ok(with_ts.to_string_lossy().to_string());
    }

    let index_js = path.join("index.js");
    if index_js.is_file() {
        return Ok(index_js.to_string_lossy().to_string());
    }

    let index_mjs = path.join("index.mjs");
    if index_mjs.is_file() {
        return Ok(index_mjs.to_string_lossy().to_string());
    }

    let index_ts = path.join("index.ts");
    if index_ts.is_file() {
        return Ok(index_ts.to_string_lossy().to_string());
    }

    Err(anyhow!(
        "Cannot resolve '{}' from '{}'",
        module_name,
        base_dir
    ))
}

fn load_es_module_shimmed<'s>(
    scope: &mut v8::HandleScope<'s>,
    file_path: &str,
    content: &str,
) -> Result<v8::Local<'s, v8::Object>> {
    let abs_path = Path::new(file_path)
        .canonicalize()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| file_path.to_string());

    let js_code = if file_path.ends_with(".ts") || file_path.ends_with(".tsx") {
        let transpiler = TypeScriptTranspiler::new();
        let transpiled = transpiler.transpile(content)?;
        shim_es_module(&transpiled, &abs_path)
    } else {
        shim_es_module(content, &abs_path)
    };

    load_commonjs_module_from_code(scope, &abs_path, &js_code)
}

fn load_commonjs_module<'s>(
    scope: &mut v8::HandleScope<'s>,
    file_path: &str,
    content: &str,
) -> Result<v8::Local<'s, v8::Object>> {
    let abs_path = Path::new(file_path)
        .canonicalize()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| file_path.to_string());

    let js_code = if file_path.ends_with(".ts") || file_path.ends_with(".tsx") {
        let transpiler = TypeScriptTranspiler::new();
        transpiler.transpile(content)?
    } else {
        content.to_string()
    };

    load_commonjs_module_from_code(scope, &abs_path, &js_code)
}

fn load_commonjs_module_from_code<'s>(
    scope: &mut v8::HandleScope<'s>,
    file_path: &str,
    js_code: &str,
) -> Result<v8::Local<'s, v8::Object>> {
    let abs_path = Path::new(file_path)
        .canonicalize()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| file_path.to_string());

    let module_obj = v8::Object::new(scope);
    let exports_obj = v8::Object::new(scope);

    let module_key = v8::String::new(scope, "exports").unwrap();
    module_obj.set(scope, module_key.into(), exports_obj.into());

    let path = Path::new(&abs_path);
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&abs_path);
    let dirname = path.parent().and_then(|p| p.to_str()).unwrap_or(".");

    let filename_str = v8::String::new(scope, filename).unwrap();
    let dirname_str = v8::String::new(scope, dirname).unwrap();

    let prev_dir = CURRENT_DIR.with(|d| d.borrow().clone());
    CURRENT_DIR.with(|d| *d.borrow_mut() = dirname.to_string());

    let wrapper = format!(
        "(function(exports, require, module, __filename, __dirname) {{\n{}\n}})",
        js_code
    );

    let code = v8::String::new(scope, &wrapper)
        .ok_or_else(|| anyhow!("Failed to create module code string"))?;

    let script = v8::Script::compile(scope, code, None)
        .ok_or_else(|| anyhow!("Failed to compile module wrapper"))?;

    let result = script
        .run(scope)
        .ok_or_else(|| anyhow!("Failed to execute module wrapper"))?;

    let func = v8::Local::<v8::Function>::try_from(result)
        .map_err(|_| anyhow!("Module wrapper did not return a function"))?;

    let recv = v8::undefined(scope);
    let require_func = create_require_func(scope);
    let args = [
        exports_obj.into(),
        require_func.into(),
        module_obj.into(),
        filename_str.into(),
        dirname_str.into(),
    ];

    func.call(scope, recv.into(), &args);

    CURRENT_DIR.with(|d| *d.borrow_mut() = prev_dir);

    let exports_key = v8::String::new(scope, "exports").unwrap();
    let final_exports = module_obj
        .get(scope, exports_key.into())
        .unwrap_or(exports_obj.into());

    let exports_object = final_exports.to_object(scope).unwrap_or(exports_obj);

    MODULE_CACHE.with(|cache| {
        let exports_val: v8::Local<v8::Value> = exports_object.into();
        cache
            .borrow_mut()
            .insert(abs_path.clone(), v8::Global::new(scope, exports_val));
    });

    Ok(exports_object)
}

fn es_dynamic_import<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    if args.length() < 1 {
        rv.set(v8::undefined(scope).into());
        return;
    }

    let specifier = args
        .get(0)
        .to_string(scope)
        .unwrap()
        .to_rust_string_lossy(scope);

    let base_dir = CURRENT_DIR.with(|d| d.borrow().clone());

    let resolved = match resolve_relative_path(&specifier, &base_dir) {
        Ok(path) => path,
        Err(_) => match PackageManager::resolve_package_in_dir(&specifier, Path::new(&base_dir)) {
            Ok(pkg_path) => pkg_path,
            Err(_) => {
                let error_str =
                    v8::String::new(scope, &format!("Cannot find module '{}'", specifier)).unwrap();
                let error = v8::Exception::error(scope, error_str);
                scope.throw_exception(error);
                return;
            }
        },
    };

    let abs_path = Path::new(&resolved)
        .canonicalize()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| resolved.clone());

    let dirname = Path::new(&abs_path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or(".");
    CURRENT_DIR.with(|d| *d.borrow_mut() = dirname.to_string());

    match resolve_and_load(scope, &abs_path, &abs_path) {
        Ok(exports) => rv.set(exports.into()),
        Err(e) => {
            let error_str =
                v8::String::new(scope, &format!("Error loading '{}': {}", specifier, e)).unwrap();
            let error = v8::Exception::error(scope, error_str);
            scope.throw_exception(error);
        }
    }
}

fn create_require_func<'s>(scope: &mut v8::HandleScope<'s>) -> v8::Local<'s, v8::Function> {
    let func = v8::FunctionTemplate::new(scope, require_callback);
    func.get_function(scope).unwrap()
}

fn create_builtin_module<'s>(
    scope: &mut v8::HandleScope<'s>,
    module_name: &str,
) -> Result<v8::Local<'s, v8::Object>> {
    let module = v8::Object::new(scope);

    match module_name {
        "fs" => {
            let read_file_sync = v8::FunctionTemplate::new(scope, fs_read_file_sync);
            let name = v8::String::new(scope, "readFileSync").unwrap();
            let func = read_file_sync.get_function(scope).unwrap();
            module.set(scope, name.into(), func.into());

            let write_file_sync = v8::FunctionTemplate::new(scope, fs_write_file_sync);
            let name = v8::String::new(scope, "writeFileSync").unwrap();
            let func = write_file_sync.get_function(scope).unwrap();
            module.set(scope, name.into(), func.into());
        }
        "path" => {
            let join = v8::FunctionTemplate::new(scope, path_join);
            let name = v8::String::new(scope, "join").unwrap();
            let func = join.get_function(scope).unwrap();
            module.set(scope, name.into(), func.into());
        }
        "http" => {
            let create_server = v8::FunctionTemplate::new(scope, http_create_server);
            let name = v8::String::new(scope, "createServer").unwrap();
            let func = create_server.get_function(scope).unwrap();
            module.set(scope, name.into(), func.into());
        }
        "querystring" => {
            let parse = v8::FunctionTemplate::new(scope, querystring_parse);
            let name = v8::String::new(scope, "parse").unwrap();
            let func = parse.get_function(scope).unwrap();
            module.set(scope, name.into(), func.into());
            
            let stringify = v8::FunctionTemplate::new(scope, querystring_stringify);
            let name = v8::String::new(scope, "stringify").unwrap();
            let func = stringify.get_function(scope).unwrap();
            module.set(scope, name.into(), func.into());
        }
        _ => {}
    }

    Ok(module)
}

fn fs_read_file_sync<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    if args.length() < 1 {
        rv.set(v8::undefined(scope).into());
        return;
    }

    let path = args
        .get(0)
        .to_string(scope)
        .unwrap()
        .to_rust_string_lossy(scope);

    match fs::read_to_string(&path) {
        Ok(content) => {
            let result = v8::String::new(scope, &content).unwrap();
            rv.set(result.into());
        }
        Err(_) => {
            rv.set(v8::undefined(scope).into());
        }
    }
}

fn fs_write_file_sync<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    if args.length() < 2 {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    }

    let path = args
        .get(0)
        .to_string(scope)
        .unwrap()
        .to_rust_string_lossy(scope);
    let data = args
        .get(1)
        .to_string(scope)
        .unwrap()
        .to_rust_string_lossy(scope);

    match fs::write(&path, data) {
        Ok(_) => rv.set(v8::Boolean::new(scope, true).into()),
        Err(_) => rv.set(v8::Boolean::new(scope, false).into()),
    }
}

fn path_join<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    let mut path_parts = Vec::new();

    for i in 0..args.length() {
        let part = args
            .get(i)
            .to_string(scope)
            .unwrap()
            .to_rust_string_lossy(scope);
        path_parts.push(part);
    }

    let joined = path_parts.join("/");
    let result = v8::String::new(scope, &joined).unwrap();
    rv.set(result.into());
}

fn http_create_server<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    // Create a server object with methods
    let server = v8::Object::new(scope);
    
    // Store the request handler if provided
    if args.length() > 0 && args.get(0).is_function() {
        let handler_key = v8::String::new(scope, "__handler").unwrap();
        server.set(scope, handler_key.into(), args.get(0));
    }
    
    // Add listen method
    let listen_template = v8::FunctionTemplate::new(scope, http_server_listen);
    let listen_func = listen_template.get_function(scope).unwrap();
    let listen_key = v8::String::new(scope, "listen").unwrap();
    server.set(scope, listen_key.into(), listen_func.into());
    
    // Add on method (event handler stub)
    let on_template = v8::FunctionTemplate::new(scope, http_server_on);
    let on_func = on_template.get_function(scope).unwrap();
    let on_key = v8::String::new(scope, "on").unwrap();
    server.set(scope, on_key.into(), on_func.into());
    
    rv.set(server.into());
}

fn http_server_listen<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    // Get port from args
    let port = if args.length() > 0 {
        args.get(0).to_int32(scope).map(|v| v.value()).unwrap_or(3000)
    } else {
        3000
    };
    
    println!("🚀 HTTP server listening on port {}", port);
    
    // Call callback if provided
    if args.length() > 1 {
        let callback = args.get(1);
        if callback.is_function() {
            let func = v8::Local::<v8::Function>::try_from(callback).unwrap();
            let recv = v8::undefined(scope);
            let undefined = v8::undefined(scope);
            func.call(scope, recv.into(), &[undefined.into()]);
        }
    }
    
    rv.set(v8::undefined(scope).into());
}

fn http_server_on<'s>(
    scope: &mut v8::HandleScope<'s>,
    _args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    // Stub for event listener - just return server for chaining
    rv.set(v8::undefined(scope).into());
}

fn querystring_parse<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    if args.length() < 1 {
        rv.set(v8::Object::new(scope).into());
        return;
    }
    
    let str = args.get(0).to_string(scope).unwrap().to_rust_string_lossy(scope);
    let obj = v8::Object::new(scope);
    
    for pair in str.split('&') {
        if let Some(eq) = pair.find('=') {
            let key = &pair[..eq];
            let value = &pair[eq + 1..];
            let key_str = v8::String::new(scope, key).unwrap();
            let val_str = v8::String::new(scope, value).unwrap();
            obj.set(scope, key_str.into(), val_str.into());
        } else if !pair.is_empty() {
            let key_str = v8::String::new(scope, pair).unwrap();
            let val_str = v8::String::new(scope, "").unwrap();
            obj.set(scope, key_str.into(), val_str.into());
        }
    }
    
    rv.set(obj.into());
}

fn querystring_stringify<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    if args.length() < 1 {
        rv.set(v8::String::new(scope, "").unwrap().into());
        return;
    }
    
    let obj = args.get(0).to_object(scope).unwrap_or(v8::Object::new(scope));
    let mut pairs = Vec::new();
    
    if let Some(keys) = obj.get_own_property_names(scope, v8::GetPropertyNamesArgs::default()) {
        for i in 0..keys.length() {
            if let Some(key) = keys.get_index(scope, i) {
                if let Some(key_str) = key.to_string(scope) {
                    let key = key_str.to_rust_string_lossy(scope);
                    if let Some(val) = obj.get(scope, key_str.into()) {
                        if let Some(val_str) = val.to_string(scope) {
                            let val = val_str.to_rust_string_lossy(scope);
                            pairs.push(format!("{}={}", key, val));
                        }
                    }
                }
            }
        }
    }
    
    let result = pairs.join("&");
    rv.set(v8::String::new(scope, &result).unwrap().into());
}

