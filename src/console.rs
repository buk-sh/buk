use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Instant;
use v8;

thread_local! {
    static STRING_BUFFER: RefCell<String> = RefCell::new(String::with_capacity(4096));
    static TIMERS: RefCell<HashMap<String, Instant>> = RefCell::new(HashMap::new());
}

pub struct ConsoleAPI;

impl ConsoleAPI {
    pub fn init<'s>(scope: &mut v8::HandleScope<'s>, global: v8::Local<'s, v8::Object>) {
        let console_template = v8::ObjectTemplate::new(scope);

        let name = v8::String::new(scope, "log").unwrap();
        let func = v8::FunctionTemplate::new(scope, fast_console_log);
        console_template.set(name.into(), func.into());

        let name = v8::String::new(scope, "error").unwrap();
        let func = v8::FunctionTemplate::new(scope, fast_console_error);
        console_template.set(name.into(), func.into());

        let name = v8::String::new(scope, "time").unwrap();
        let func = v8::FunctionTemplate::new(scope, console_time);
        console_template.set(name.into(), func.into());

        let name = v8::String::new(scope, "timeEnd").unwrap();
        let func = v8::FunctionTemplate::new(scope, console_time_end);
        console_template.set(name.into(), func.into());

        let console_obj = console_template.new_instance(scope).unwrap();
        let console_key = v8::String::new(scope, "console").unwrap();
        global.set(scope, console_key.into(), console_obj.into());
    }
}

fn fast_console_log<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    STRING_BUFFER.with(|buf| {
        let mut output = buf.borrow_mut();
        output.clear();
        
        for i in 0..args.length() {
            if i > 0 {
                output.push(' ');
            }
            let arg = args.get(i);
            if let Some(str) = arg.to_string(scope) {
                output.push_str(&str.to_rust_string_lossy(scope));
            }
        }
        
        if !output.is_empty() {
            println!("{}", output);
        }
    });
    
    rv.set(v8::undefined(scope).into());
}

fn fast_console_error<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    STRING_BUFFER.with(|buf| {
        let mut output = buf.borrow_mut();
        output.clear();
        
        for i in 0..args.length() {
            if i > 0 {
                output.push(' ');
            }
            let arg = args.get(i);
            if let Some(str) = arg.to_string(scope) {
                output.push_str(&str.to_rust_string_lossy(scope));
            }
        }
        
        if !output.is_empty() {
            eprintln!("{}", output);
        }
    });
    
    rv.set(v8::undefined(scope).into());
}

fn console_time<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    let label = if args.length() > 0 {
        args.get(0).to_string(scope).unwrap().to_rust_string_lossy(scope)
    } else {
        "default".to_string()
    };
    
    TIMERS.with(|timers| {
        timers.borrow_mut().insert(label, std::time::Instant::now());
    });
    
    rv.set(v8::undefined(scope).into());
}

fn console_time_end<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    let label = if args.length() > 0 {
        args.get(0).to_string(scope).unwrap().to_rust_string_lossy(scope)
    } else {
        "default".to_string()
    };
    
    TIMERS.with(|timers| {
        if let Some(start) = timers.borrow_mut().remove(&label) {
            let elapsed = start.elapsed();
            println!("{}: {:?}", label, elapsed);
        }
    });
    
    rv.set(v8::undefined(scope).into());
}
