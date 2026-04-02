use crate::bun_api::BunAPI;
use crate::console::ConsoleAPI;
use crate::http_native::HttpNativeFramework;
use crate::server::BunServer;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::Once;
use v8;

static INIT: Once = Once::new();

pub struct JsRuntime {
    isolate: v8::OwnedIsolate,
    context: v8::Global<v8::Context>,
    compiled_cache: HashMap<String, v8::Global<v8::UnboundScript>>,
}

impl JsRuntime {
    pub fn new() -> Self {
        INIT.call_once(|| {
            let platform = v8::new_default_platform(4, true).make_shared();
            v8::V8::initialize_platform(platform);
            v8::V8::initialize();
        });

        let params = v8::CreateParams::default()
            .heap_limits(0, 512 * 1024 * 1024);

        let mut isolate = v8::Isolate::new(params);
        
        isolate.set_capture_stack_trace_for_uncaught_exceptions(true, 10);

        let global_context = {
            let handle_scope = &mut v8::HandleScope::new(&mut isolate);
            let context = v8::Context::new(handle_scope);
            v8::Global::new(handle_scope, context)
        };

        let mut runtime = Self {
            isolate,
            context: global_context,
            compiled_cache: HashMap::new(),
        };

        runtime.init_bindings();
        runtime
    }

    fn init_bindings(&mut self) {
        let context = self.context.clone();
        let scope = &mut v8::HandleScope::with_context(&mut self.isolate, context);
        let global = scope.get_current_context().global(scope);

        ConsoleAPI::init(scope, global);
        BunAPI::init(scope, global);
        BunServer::init(scope, global);
        HttpNativeFramework::init(scope, global);
    }

    pub fn execute_cached(&mut self, name: &str, source: &str) -> Result<()> {
        let context = self.context.clone();
        let scope = &mut v8::HandleScope::with_context(&mut self.isolate, context);
        
        let code = v8::String::new(scope, source)
            .ok_or_else(|| anyhow!("Failed to create string"))?;
        let resource_name = v8::String::new(scope, name).unwrap();
        let source_map_url = v8::undefined(scope);
        
        let origin = v8::ScriptOrigin::new(
            scope,
            resource_name.into(),
            0,
            0,
            false,
            0,
            source_map_url.into(),
            false,
            false,
            false,
        );

        let script = v8::Script::compile(scope, code, Some(&origin))
            .ok_or_else(|| anyhow!("Failed to compile script"))?;
        
        script.run(scope);
        Ok(())
    }

    pub async fn execute(&mut self, source: &str) -> Result<()> {
        self.execute_cached("<script>", source)
    }

    pub fn execute_optimized(&mut self, source: &str, optimize_for_size: bool) -> Result<()> {
        let context = self.context.clone();
        let scope = &mut v8::HandleScope::with_context(&mut self.isolate, context);
        let try_catch = &mut v8::TryCatch::new(scope);

        let code = v8::String::new(try_catch, source)
            .ok_or_else(|| anyhow!("Failed to create string"))?;
        
        let resource_name = v8::String::new(try_catch, "<script>").unwrap();
        let source_map_url = v8::undefined(try_catch);
        let origin = v8::ScriptOrigin::new(
            try_catch,
            resource_name.into(),
            0,
            0,
            false,
            0,
            source_map_url.into(),
            false,
            optimize_for_size,
            false,
        );

        let script = v8::Script::compile(try_catch, code, Some(&origin))
            .ok_or_else(|| anyhow!("Failed to compile script"))?;

        let result = script.run(try_catch);

        if let Some(exc) = try_catch.exception() {
            let msg = exc.to_string(try_catch).unwrap().to_rust_string_lossy(try_catch);
            return Err(anyhow!("JavaScript exception: {}", msg));
        }

        match result {
            Some(_) => Ok(()),
            None => Err(anyhow!("JavaScript execution failed"))
        }
    }
}

impl Drop for JsRuntime {
    fn drop(&mut self) {
        self.compiled_cache.clear();
    }
}
