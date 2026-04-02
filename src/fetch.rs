use std::sync::Arc;
use tokio::sync::Mutex;
use v8;

pub struct FetchAPI;

impl FetchAPI {
    pub fn init<'s>(scope: &mut v8::HandleScope<'s>, global: v8::Local<'s, v8::Object>) {
        let name = v8::String::new(scope, "fetch").unwrap();
        let func = v8::FunctionTemplate::new(scope, fetch_callback);
        let func = func.get_function(scope).unwrap();
        global.set(scope, name.into(), func.into());
    }
}

fn fetch_callback<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    if args.length() < 1 {
        rv.set(v8::undefined(scope).into());
        return;
    }

    let url = args
        .get(0)
        .to_string(scope)
        .unwrap()
        .to_rust_string_lossy(scope);

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(async {
        match reqwest::get(&url).await {
            Ok(resp) => match resp.text().await {
                Ok(text) => text,
                Err(e) => format!("Error reading body: {}", e),
            },
            Err(e) => format!("Fetch error: {}", e),
        }
    });

    let v8_str = v8::String::new(scope, &result).unwrap();
    rv.set(v8_str.into());
}
