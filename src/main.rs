mod bun_api;
mod console;
mod env_loader;
mod fetch;
mod http_native;
mod module;
mod package_manager;
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
  runt <file.js|file.ts>     Run a JavaScript or TypeScript file
  runt run <file>            Run a file (explicit)
  runt serve <file>          Run a file with Bun.serve()
  runt i <package>           Install npm package (like bun i)
  runt install <package>     Install npm package
  runt --watch <file>        Run with hot reload
  runt --env <file>          Load .env file before running
  runt test <pattern>        Run tests
  runt --help                Show this help

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
    let mut install_package = String::new();

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
            "i" | "install" => {
                command = args[i].clone();
                i += 1;
                if i < args.len() && !args[i].starts_with("--") {
                    install_package = args[i].clone();
                    i += 1;
                }
            }
            "run" | "serve" | "test" => {
                command = args[i].clone();
                i += 1;
            }
            _ => {
                if filename.is_empty() && !args[i].starts_with("--") && command != "i" && command != "install" {
                    filename = args[i].clone();
                }
                i += 1;
            }
        }
    }

    // Handle install command
    if command == "i" || command == "install" {
        if install_package.is_empty() {
            println!("Usage: runt i <package>");
            return Ok(());
        }
        let pm = PackageManager::new();
        pm.install(&install_package).await?;
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
            runtime.execute(&js_code).await?;

            // Keep alive
            println!("Server running. Press Ctrl+C to stop.");
            tokio::signal::ctrl_c().await?;
        }
        "test" => {
            // Run tests
            println!("Running tests...");
            runtime.execute(&js_code).await?;
            println!("Tests complete");
        }
        _ => {
            // Normal run
            runtime.execute(&js_code).await?;
        }
    }

    Ok(())
}
