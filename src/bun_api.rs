use std::fs;
use std::path::Path;
use v8;

/// This module implements the Bun API,
/// Which provides various utility functions to JavaScript code running in the V8 engine.
pub struct BunAPI;


/// Implementation of the Bun API.
impl BunAPI {
    /// This function initializes the Bun API by creating a new object template and setting various functions and properties on it.
    pub fn init<'s>(scope: &mut v8::HandleScope<'s>, global: v8::Local<'s, v8::Object>) {
        /// We create a new object template for the Bun API, which will hold all the functions and properties we want to expose.
        let bun_template = v8::ObjectTemplate::new(scope);

        // We then define various functions on the Bun API, such as Bun.file, Bun.write, Bun.which, etc.
        // ** Each function does not start with Bun.* anymore. they start with Buk.* to avoid confusion with the actual Bun runtime.
        // We can change this back to Bun.* later if we want, 
        // but for now we will use Buk.* to make it clear that this is our own implementation of the API, not the actual Bun runtime.
        
        // Bun.file(path) - returns a BunFile object
        let name = v8::String::new(scope, "file").unwrap();         /// Function name is "file", which will be called as 
                                                                    /// Buk.file("path/to/file") in JavaScript. This function will 
                                                                    // return a BunFile object that has methods like exists(), text(), json(), 
                                                                    // and size() to interact with the file.

        let func = v8::FunctionTemplate::new(scope, bun_file);      // We create the function template for the "file" function, 
                                                                    // which will call the bun_file Rust function when invoked from JavaScript.
        bun_template.set(name.into(), func.into());                 // We set the "file" function on the bun_template, so it becomes a method
                                                                    // of the Bun API.

        // Bun.write(path, data) - writes file
        let name = v8::String::new(scope, "write").unwrap();        // Function name is "write", which will be called as Buk.write("path/to/file", "data to write") in JavaScript. 
                                                                    // This function will write the specified data to the specified file path.
        let func = v8::FunctionTemplate::new(scope, bun_write);     // We create the function template for the "write" function, 
                                                                    // which will call the bun_write Rust function when invoked from JavaScript.
        bun_template.set(name.into(), func.into());                 // We set the "write" function on the bun_template, so it becomes a method 
                                                                    // of the Bun API.

        // Bun.which(cmd) - find executable
        let name = v8::String::new(scope, "which").unwrap();        // Function name is "which", which will be called as Buk.which("command") in JavaScript. 
                                                                    // This function will search the system PATH for the specified command and return its full path if found.
        let func = v8::FunctionTemplate::new(scope, bun_which);     // We create the function template for the "which" function, 
                                                                    // which will call the bun_which Rust function when invoked from JavaScript.
        bun_template.set(name.into(), func.into());                 // We set the "which" function on the bun_template, so it becomes a method of the Bun API.

        // Bun.sleep(ms) - sleep for milliseconds
        let name = v8::String::new(scope, "sleep").unwrap();        /// Function name is "sleep", which will be called as Buk.sleep(1000) in JavaScript to sleep for 1000 milliseconds (1 second). 
                                                                    /// This function will block the current thread for the specified duration.
        let func = v8::FunctionTemplate::new(scope, bun_sleep);     // We create the function template for the "sleep" function, which will call the bun_sleep Rust function when invoked from JavaScript.
        bun_template.set(name.into(), func.into());                 // We set the "sleep" function on the bun_template, so it becomes a method of the Bun API.

        // Bun.cwd() - current working directory
        let name = v8::String::new(scope, "cwd").unwrap();          /// Function name is "cwd", which will be called as Buk.cwd() in JavaScript to get the current working directory. 
                                                                    /// This function will return the current working directory as a string.
        let func = v8::FunctionTemplate::new(scope, bun_cwd);       // We create the function template for the "cwd" function, which will call the bun_cwd Rust function when invoked from JavaScript.
        bun_template.set(name.into(), func.into());                 // We set the "cwd" function on the bun_template, so it becomes a method of the Bun API.

        let bun_obj = bun_template.new_instance(scope).unwrap();    /// We create a new instance of the bun_template, which will be the 
                                                                    /// actual Bun API object that we expose to JavaScript. 
                                                                    /// We will set various properties on this object, such as env and main.

        // Bun.env - environment variables (set after creation)
        let env_template = v8::ObjectTemplate::new(scope);
        let env_obj = env_template.new_instance(scope).unwrap();
        for (key, value) in std::env::vars() {
            let key_str = v8::String::new(scope, &key).unwrap();
            let val_str = v8::String::new(scope, &value).unwrap();
            env_obj.set(scope, key_str.into(), val_str.into());
        }
        let env_key = v8::String::new(scope, "env").unwrap();
        bun_obj.set(scope, env_key.into(), env_obj.into());

        // Bun.main - main script path
        if let Ok(main) = std::env::var("RUNT_MAIN") {
            let main_str = v8::String::new(scope, &main).unwrap();
            let name = v8::String::new(scope, "main").unwrap();
            bun_obj.set(scope, name.into(), main_str.into());
        }

        let bun_key = v8::String::new(scope, "Buk").unwrap();
        global.set(scope, bun_key.into(), bun_obj.into());
    }
}

fn bun_file<'s>(
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

    let file_template = v8::ObjectTemplate::new(scope);

    // file.exists()
    let name = v8::String::new(scope, "exists").unwrap();
    let func = v8::FunctionTemplate::new(scope, file_exists);
    file_template.set(name.into(), func.into());

    // file.text() - returns content as string
    let name = v8::String::new(scope, "text").unwrap();
    let func = v8::FunctionTemplate::new(scope, file_text);
    file_template.set(name.into(), func.into());

    // file.json() - returns parsed JSON
    let name = v8::String::new(scope, "json").unwrap();
    let func = v8::FunctionTemplate::new(scope, file_json);
    file_template.set(name.into(), func.into());

    // file.size() - returns file size
    let name = v8::String::new(scope, "size").unwrap();
    let func = v8::FunctionTemplate::new(scope, file_size);
    file_template.set(name.into(), func.into());

    // Store path in internal field
    let file_obj = file_template.new_instance(scope).unwrap();
    let path_key = v8::String::new(scope, "__path").unwrap();
    let path_val = v8::String::new(scope, &path).unwrap();
    file_obj.set(scope, path_key.into(), path_val.into());

    rv.set(file_obj.into());
}

fn get_file_path<'s>(scope: &mut v8::HandleScope<'s>, this: v8::Local<v8::Object>) -> String {
    let path_key = v8::String::new(scope, "__path").unwrap();
    this.get(scope, path_key.into())
        .unwrap()
        .to_string(scope)
        .unwrap()
        .to_rust_string_lossy(scope)
}

fn file_exists<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    let path = get_file_path(scope, this);
    let exists = Path::new(&path).exists();
    rv.set(v8::Boolean::new(scope, exists).into());
}

fn file_text<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    let path = get_file_path(scope, this);

    match fs::read_to_string(&path) {
        Ok(content) => {
            let v8_str = v8::String::new(scope, &content).unwrap();
            rv.set(v8_str.into());
        }
        Err(_) => rv.set(v8::undefined(scope).into()),
    }
}

fn file_json<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    let path = get_file_path(scope, this);

    match fs::read_to_string(&path) {
        Ok(content) => {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                let json_str = json.to_string();
                let v8_str = v8::String::new(scope, &json_str).unwrap();
                // Parse JSON in V8
                let json_parse = v8::String::new(scope, "JSON.parse").unwrap();
                if let Some(parse_fn) = json_parse
                    .to_object(scope)
                    .and_then(|obj| v8::Local::<v8::Function>::try_from(obj).ok())
                {
                    let recv = v8::undefined(scope);
                    let args = [v8_str.into()];
                    if let Some(result) = parse_fn.call(scope, recv.into(), &args) {
                        rv.set(result);
                        return;
                    }
                }
            }
        }
        Err(_) => {}
    }
    rv.set(v8::undefined(scope).into());
}

fn file_size<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    let path = get_file_path(scope, this);

    match fs::metadata(&path) {
        Ok(meta) => {
            let size = meta.len() as f64;
            rv.set(v8::Number::new(scope, size).into());
        }
        Err(_) => rv.set(v8::Number::new(scope, -1.0).into()),
    }
}

fn bun_write<'s>(
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

fn bun_which<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    if args.length() < 1 {
        rv.set(v8::undefined(scope).into());
        return;
    }

    let cmd = args
        .get(0)
        .to_string(scope)
        .unwrap()
        .to_rust_string_lossy(scope);

    // Simple PATH search
    if let Ok(path_var) = std::env::var("PATH") {
        let separator = if cfg!(windows) { ';' } else { ':' };
        for path in path_var.split(separator) {
            let full_path = Path::new(path).join(&cmd);
            if full_path.exists() {
                let path_str = v8::String::new(scope, full_path.to_str().unwrap_or("")).unwrap();
                rv.set(path_str.into());
                return;
            }
        }
    }

    rv.set(v8::undefined(scope).into());
}

fn bun_sleep<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    if args.length() < 1 {
        rv.set(v8::undefined(scope).into());
        return;
    }

    let ms = args.get(0).number_value(scope).unwrap_or(0.0) as u64;
    std::thread::sleep(std::time::Duration::from_millis(ms));

    rv.set(v8::undefined(scope).into());
}

fn bun_cwd<'s>(
    scope: &mut v8::HandleScope<'s>,
    _args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    if let Ok(cwd) = std::env::current_dir() {
        let cwd_str = cwd.to_string_lossy().to_string();
        let v8_str = v8::String::new(scope, &cwd_str).unwrap();
        rv.set(v8_str.into());
    } else {
        rv.set(v8::undefined(scope).into());
    }
}
