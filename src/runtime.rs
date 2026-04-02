use crate::console::ConsoleAPI;
use anyhow::{anyhow, Result};
use v8;

pub struct JsRuntime {
    isolate: v8::OwnedIsolate,
    context: v8::Global<v8::Context>,
}

impl JsRuntime {
    pub fn new() -> Self {
        let platform = v8::new_default_platform(0, false).make_shared();
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();

        let mut isolate = v8::Isolate::new(Default::default());
        let global_context = {
            let handle_scope = &mut v8::HandleScope::new(&mut isolate);
            let context = v8::Context::new(handle_scope);
            v8::Global::new(handle_scope, context)
        };

        let mut runtime = Self {
            isolate,
            context: global_context,
        };

        runtime.init_bindings();
        runtime
    }

    fn init_bindings(&mut self) {
        let context = self.context.clone();
        let scope = &mut v8::HandleScope::with_context(&mut self.isolate, context);
        let global = scope.get_current_context().global(scope);

        ConsoleAPI::init(scope, global);
    }

    pub async fn execute(&mut self, source: &str) -> Result<()> {
        let context = self.context.clone();
        let scope = &mut v8::HandleScope::with_context(&mut self.isolate, context);
        let handle_scope = &mut v8::EscapableHandleScope::new(scope);
        let try_catch = &mut v8::TryCatch::new(handle_scope);

        let code = v8::String::new(try_catch, source).ok_or_else(|| anyhow!("Failed to create string"))?;
        let resource_name = v8::String::new(try_catch, "<script>").unwrap();
        let source_map_url = v8::String::new(try_catch, "").unwrap();

        let origin = v8::ScriptOrigin::new(
            try_catch,
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
        // V8 cleanup happens at program exit, not per-isolate
    }
}
