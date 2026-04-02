mod console;
mod fetch;
mod module;
mod runtime;

use crate::console::ConsoleAPI;
use crate::fetch::FetchAPI;
use anyhow::Result;
use runtime::JsRuntime;
use std::env;
use std::fs;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("Usage: runt <script.js>");
        let mut runtime = JsRuntime::new();
        let scope = runtime.scope();
        let global = runtime.global();
        ConsoleAPI::init(scope, global);
        FetchAPI::init(scope, global);
        return Ok(());
    }

    let filename = &args[1];
    let source = fs::read_to_string(filename)?;

    let mut runtime = JsRuntime::new();
    runtime.execute(&source).await?;

    Ok(())
}
