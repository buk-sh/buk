use v8;

pub struct ConsoleAPI;

impl ConsoleAPI {
    pub fn init<'s>(scope: &mut v8::HandleScope<'s>, global: v8::Local<'s, v8::Object>) {
        let console_template = v8::ObjectTemplate::new(scope);

        let name = v8::String::new(scope, "log").unwrap();
        let func = v8::FunctionTemplate::new(scope, console_log);
        console_template.set(name.into(), func.into());

        let name = v8::String::new(scope, "error").unwrap();
        let func = v8::FunctionTemplate::new(scope, console_error);
        console_template.set(name.into(), func.into());

        let console_obj = console_template.new_instance(scope).unwrap();
        let console_key = v8::String::new(scope, "console").unwrap();
        global.set(scope, console_key.into(), console_obj.into());
    }
}

fn console_log<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    let mut output = String::new();
    for i in 0..args.length() {
        if i > 0 {
            output.push(' ');
        }
        let arg = args.get(i);
        let str = arg.to_string(scope).unwrap().to_rust_string_lossy(scope);
        output.push_str(&str);
    }
    println!("{}", output);
    rv.set(v8::undefined(scope).into());
}

fn console_error<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    let mut output = String::new();
    for i in 0..args.length() {
        if i > 0 {
            output.push(' ');
        }
        let arg = args.get(i);
        let str = arg.to_string(scope).unwrap().to_rust_string_lossy(scope);
        output.push_str(&str);
    }
    eprintln!("{}", output);
    rv.set(v8::undefined(scope).into());
}
