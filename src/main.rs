mod bun_api;
mod console;
mod env_loader;
mod fetch;
mod http_native;
mod module;
mod package_json;
mod package_manager;
mod require;
mod runtime;
mod server;
mod typescript;
mod watcher;

use anyhow::Result;
use env_loader::EnvLoader;
use package_manager::PackageManager;
use runtime::JsRuntime;
use std::env;
use std::fs;
use typescript::TypeScriptTranspiler;

const HELP: &str = r#"Runt - A fast JavaScript runtime

Usage:
  runt <file.js|file.ts>           Run a JavaScript or TypeScript file
  runt run <file>                  Run a file (explicit)
  runt serve <file>                Run a file with Bun.serve()
  runt test <pattern>              Run tests
  
Package Manager:
  runt init [name]                 Initialize a new project
  runt i <package> [packages...]   Install npm packages
  runt install <pkg> [-D|--dev]    Install with dev flag
  runt remove <package>            Remove a package
  runt uninstall <package>         Alias for remove
  runt list                        List installed packages
  runt run <script>                Run a package.json script
  
Options:
  runt --watch <file>              Run with hot reload
  runt --env <file>                Load .env file before running
  runt --help                      Show this help

Environment:
  Runt loads .env and .env.local automatically
"#;

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env files
    let _ = EnvLoader::load();

    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("{}", HELP);
        return Ok(());
    }

    let mut watch_mode = false;
    let mut command = String::new();
    let mut filename = String::new();
    let mut install_packages: Vec<String> = Vec::new();
    let mut dev_flag = false;
    let mut script_name = String::new();

    // Parse arguments
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--watch" => {
                watch_mode = true;
                i += 1;
            }
            "--help" | "-h" => {
                println!("{}", HELP);
                return Ok(());
            }
            "-D" | "--dev" => {
                dev_flag = true;
                i += 1;
            }
            "init" => {
                command = args[i].clone();
                i += 1;
                if i < args.len() && !args[i].starts_with("--") && !args[i].starts_with("-") {
                    filename = args[i].clone();
                    i += 1;
                }
            }
            "i" | "install" => {
                command = args[i].clone();
                i += 1;
                // Collect all packages to install
                while i < args.len() && !args[i].starts_with("--") && !args[i].starts_with("-") {
                    install_packages.push(args[i].clone());
                    i += 1;
                }
            }
            "remove" | "uninstall" => {
                command = args[i].clone();
                i += 1;
                if i < args.len() && !args[i].starts_with("--") {
                    filename = args[i].clone();
                    i += 1;
                }
            }
            "list" => {
                command = args[i].clone();
                i += 1;
            }
            "run" => {
                command = args[i].clone();
                i += 1;
                if i < args.len() && !args[i].starts_with("--") {
                    script_name = args[i].clone();
                    i += 1;
                }
            }
            "serve" | "test" => {
                command = args[i].clone();
                i += 1;
            }
            _ => {
                if filename.is_empty() && !args[i].starts_with("--") {
                    filename = args[i].clone();
                }
                i += 1;
            }
        }
    }

    // Handle init command
    if command == "init" {
        let pm = PackageManager::new();
        let name = if filename.is_empty() { None } else { Some(filename.as_str()) };
        pm.init(name)?;
        return Ok(());
    }

    // Handle install command
    if command == "i" || command == "install" {
        let mut pm = PackageManager::new();
        if install_packages.is_empty() {
            // Install all from package.json
            pm.install(&[], dev_flag).await?;
        } else {
            // Install specific packages
            pm.install(&install_packages, dev_flag).await?;
        }
        return Ok(());
    }

    // Handle remove/uninstall command
    if command == "remove" || command == "uninstall" {
        if filename.is_empty() {
            println!("Usage: runt {} <package>", command);
            return Ok(());
        }
        let mut pm = PackageManager::new();
        pm.remove(&filename).await?;
        return Ok(());
    }

    // Handle list command
    if command == "list" {
        let pm = PackageManager::new();
        pm.list()?;
        return Ok(());
    }

    // Handle run command (package.json scripts)
    if command == "run" && !script_name.is_empty() {
        let pm = PackageManager::new();
        pm.run_script(&script_name)?;
        return Ok(());
    }

    if filename.is_empty() {
        println!("{}", HELP);
        return Ok(());
    }

    // Run once initially
    if let Err(e) = run_file(&filename, &command).await {
        eprintln!("Error: {}", e);
        if !watch_mode {
            return Err(e);
        }
    }

    // Watch mode - restart on file changes
    if watch_mode {
        println!("\n👀 Watching for changes...");
        let mut file_watcher = watcher::FileWatcher::new(&[&filename])?;
        file_watcher.watch(".")?;

        loop {
            if let Some(event) = file_watcher.wait_for_change() {
                if let Some(path) = event.paths.first() {
                    let path_str = path.to_string_lossy();
                    if watcher::should_restart(&path_str) {
                        println!("\n🔄 File changed: {} - Restarting...", path_str);
                        if let Err(e) = run_file(&filename, &command).await {
                            eprintln!("Error: {}", e);
                        }
                        println!("\n👀 Watching for changes...");
                    }
                }
            }
        }
    }

    Ok(())
}

async fn run_file(filename: &str, command: &str) -> Result<()> {
    let source = fs::read_to_string(filename)?;
    unsafe {
        std::env::set_var("RUNT_MAIN", filename);
    }

    // TypeScript transpilation
    let js_code = if TypeScriptTranspiler::is_typescript(filename) {
        let transpiler = TypeScriptTranspiler::new();
        transpiler.transpile(&source)?
    } else {
        source
    };

    let mut runtime = JsRuntime::new();

    match command.as_ref() as &str {
        "serve" => {
            // Pre-load server module
            let server_code = r#"
                const server = Bun.serve({
                    port: 3000,
                    fetch(req) {
                        return new Response("Hello from Runt!");
                    }
                });
                console.log("Server started on port 3000");
            "#;
            runtime.execute(server_code).await?;

            // Run user code
            runtime.execute_file(&filename, &js_code).await?;

            // Keep alive
            println!("Server running. Press Ctrl+C to stop.");
            tokio::signal::ctrl_c().await?;
        }
        "test" => {
            // Run tests
            println!("Running tests...");
            runtime.execute_file(&filename, &js_code).await?;
            println!("Tests complete");
        }
        _ => {
            // Normal run
            runtime.execute_file(&filename, &js_code).await?;
        }
    }

    Ok(())
}
