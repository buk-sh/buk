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
        let env_template = v8::ObjectTemplate::new(scope);          /// We create a new object template for the env property, which will hold all the environment variables as properties. 
                                                                    /// For example, if we have an environment variable named "HOME", it will be accessible as Buk.env.HOME in JavaScript.
        let env_obj = env_template.new_instance(scope).unwrap();    /// We create a new instance of the env_template, which will be the actual env object that we set on the Bun API. 
                                                                    /// We will populate this object with all the environment variables from std::env::vars().
        for (key, value) in std::env::vars() {                      /// We iterate over all environment variables using std::env::vars(), which returns an iterator of (key, value) pairs.
            let key_str = v8::String::new(scope, &key).unwrap();    /// This creates a new V8 string for the environment variable key, which will be used as the property name on the env object.
            let val_str = v8::String::new(scope, &value).unwrap();  /// This creates a new V8 string for the environment variable value, which will be used as the property value on the env object.
            env_obj.set(scope, key_str.into(), val_str.into());     /// We set the environment variable on the env object, so it becomes accessible as Buk.env.KEY in JavaScript, where KEY is the name of the environment variable.
        }
        let env_key = v8::String::new(scope, "env").unwrap();       /// We create a new V8 string for the "env" property name, which will be used to set the env object on the Bun API.
        bun_obj.set(scope, env_key.into(), env_obj.into());         /// We set the env object on the bun_obj, so it becomes accessible as Buk.env in JavaScript.

        // Bun.main - main script path
        if let Ok(main) = std::env::var("RUNT_MAIN") {              /// We check if there is an environment variable named "RUNT_MAIN", which we can set to specify the main script path. 
                                                                    /// If this variable is set, we will add it as a property on the Bun API, so it can be accessed as Buk.main in JavaScript.
            let main_str = v8::String::new(scope, &main).unwrap();  /// We create a new V8 string for the main script path, which will be used as the property value for Buk.main.
            let name = v8::String::new(scope, "main").unwrap();     /// We create a new V8 string for the "main" property name, which will be used to set the main script path on the Bun API.
            bun_obj.set(scope, name.into(), main_str.into());       /// We set the main script path on the bun_obj, so it becomes accessible as Buk.main in JavaScript. 
                                                                    /// This allows JavaScript code to know the path of the main script that is being executed, which can be useful for various purposes such as resolving relative paths.
        }

        let bun_key = v8::String::new(scope, "Buk").unwrap();       /// We create a new V8 string for the "Buk" property name, which will be used to set the Bun API object on the global object. 
                                                                    /// We use "Buk" instead of "Bun" to avoid confusion with the actual Bun runtime, since this is our own implementation of the API.
        global.set(scope, bun_key.into(), bun_obj.into());          /// We set the bun_obj on the global object, so it becomes accessible as Buk in JavaScript. 
                                                                    /// This means that all the functions and properties we defined on bun_obj will be available under the Buk namespace in JavaScript. 
                                                                    /// For example, you can call Buk.file("path/to/file").exists() to check if a file exists, or access environment variables with Buk.env.VAR_NAME.
    }
}

/// The following functions implement the various methods of the Bun API, such as file(), write(), which(), sleep(), and cwd().

/// Bun.file(path) - returns a BunFile object with methods to interact with the file at the specified path.
fn bun_file<'s>(
    scope: &mut v8::HandleScope<'s>,                                /// The scope parameter is used to create and manage V8 values and objects within the context of the function call. 
                                                                    /// It allows us to create V8 strings, objects, and other values that are properly scoped and garbage collected.
    args: v8::FunctionCallbackArguments<'s>,                        /// Make a mutabe reference to the arguments passed to the function from JavaScript. This allows us to access the arguments and their values, such as the file path in this case.
    mut rv: v8::ReturnValue,                                        /// This is used to set the return value of the function that will be returned to JavaScript. We will set this to the BunFile object that we create, so it can be used in JavaScript.   
) {
    if args.length() < 1 {                                          /// We check if at least one argument is passed to the function, which should be the file path. If no arguments are passed, we return undefined.
        rv.set(v8::undefined(scope).into());                        /// If no arguments are passed, we set the return value to undefined and
        return;                                                     /// return early from the function, since we cannot proceed without a file path.
    }

    let path = args                                                 /// Store the argument as a Rust string. We get the first argument (index 0), convert it to a V8 string, and then convert that to a Rust string. 
                                                                    /// This will be the file path that we will use to create the BunFile object.
        .get(0)                                                     /// Get the first argument passed to the function, which should be the file path.
        .to_string(scope)                                           /// Convert the argument to a V8 string, which allows us to work with it in the V8 context. This is necessary because the argument is passed from JavaScript as a V8 value.
        .unwrap()                                                   /// Unwrap the result of to_string, which will panic if the argument cannot be converted to a string. In a production implementation, we should handle this error more gracefully.
        .to_rust_string_lossy(scope);                               /// Convert the V8 string to a Rust string, which allows us to work with it in Rust. The to_rust_string_lossy method will convert the V8 string to a Rust string, and if there are any invalid UTF-8 sequences, it will replace them with the Unicode replacement character. 
                                                                    /// This is useful for handling file paths that may contain non-UTF-8 characters.

    let file_template = v8::ObjectTemplate::new(scope);             /// We create a new object template for the BunFile object, which will have methods like exists(), text(), json(), and size() to interact with the file. 
                                                                    /// This template will be used to create instances of the BunFile object for different file paths.

    // file.exists()
    let name = v8::String::new(scope, "exists").unwrap();           /// We create a new V8 string for the "exists" method name, which will be used to define the exists() method on the BunFile object. 
                                                                    /// This method will check if the file at the specified path exists and return a boolean value.
    let func = v8::FunctionTemplate::new(scope, file_exists);       /// We create a function template for the "exists" method, which will call the file_exists Rust function when invoked from JavaScript. 
                                                                    /// The file_exists function will check if the file exists and return true or false accordingly.
    file_template.set(name.into(), func.into());                    /// We set the "exists" method on the file_template, so it becomes a method of the BunFile object. 
                                                                    /// This means that you can call Buk.file("path/to/file").exists() in JavaScript to check if the file exists.

    // file.text() - returns content as string
    let name = v8::String::new(scope, "text").unwrap();             /// We create a new V8 string for the "text" method name, which will be used to define the text() method on the BunFile object. 
                                                                    /// This method will read the content of the file and return it as a string.
    let func = v8::FunctionTemplate::new(scope, file_text);         /// We create a function template for the "text" method, which will call the file_text Rust function when invoked from JavaScript. 
                                                                    /// The file_text function will read the content of the file and return it as a string, or undefined if there is an error.
    file_template.set(name.into(), func.into());                    /// We set the "text" method on the file_template, so it becomes a method of the BunFile object. 
                                                                    /// This means that you can call Buk.file("path/to/file").text() in JavaScript to get the content of the file as a string.

    // file.json() - returns parsed JSON
    let name = v8::String::new(scope, "json").unwrap();             /// We create a new V8 string for the "json" method name, which will be used to define the json() method on the BunFile object. 
                                                                    /// This method will read the content of the file, parse it as JSON, and return the resulting object. If there is an error (e.g., file does not exist or content is not valid JSON), it will return undefined.
    let func = v8::FunctionTemplate::new(scope, file_json);         /// We create a function template for the "json" method, which will call the file_json Rust function when invoked from JavaScript. 
                                                                    /// The file_json function will read the content of the file, attempt to parse it as JSON, and return the resulting object to JavaScript. If there is an error, it will return undefined.
    file_template.set(name.into(), func.into());                    /// We set the "json" method on the file_template, so it becomes a method of the BunFile object. 
                                                                    /// This means that you can call Buk.file("path/to/file").json() in JavaScript to get the content of the file parsed as JSON.

    // file.size() - returns file size
    let name = v8::String::new(scope, "size").unwrap();             /// We create a new V8 string for the "size" method name, which will be used to define the size() method on the BunFile object. 
                                                                    /// This method will return the size of the file in bytes as a number. If there is an error (e.g., file does not exist), it will return -1.
    let func = v8::FunctionTemplate::new(scope, file_size);         /// We create a function template for the "size" method, which will call the file_size Rust function when invoked from JavaScript. 
                                                                    /// The file_size function will check the metadata of the file and return its size in bytes, or -1 if there is an error.
    file_template.set(name.into(), func.into());                    /// We set the "size" method on the file_template, so it becomes a method of the BunFile object. 
                                                                    /// This means that you can call Buk.file("path/to/file").size() in JavaScript to get the size of the file in bytes.

    // Store path in internal field
    let file_obj = file_template.new_instance(scope).unwrap();      /// We create a new instance of the file_template, which will be the actual BunFile object that we return to JavaScript. 
                                                                    /// This object will have the methods we defined (exists, text, json, size) and we will also store the file path in an internal field for later use by those methods.
    let path_key = v8::String::new(scope, "__path").unwrap();       /// We create a new V8 string for the internal field name "__path", which we will use to store the file path on the BunFile object. 
                                                                    /// This allows us to access the file path later in the methods like exists(), text(), json(), and size() when they are called, so they know which file to operate on.
    let path_val = v8::String::new(scope, &path).unwrap();          /// We create a new V8 string for the file path value, which we will store on the BunFile object under the "__path" key. 
                                                                    /// This allows us to keep track of which file path this BunFile object represents, so that the methods can use it to perform their operations on the correct file.
    file_obj.set(scope, path_key.into(), path_val.into());          /// We set the "__path" property on the file_obj to the file path value, so it becomes an internal field of the BunFile object. 
                                                                    /// This means that when we call methods like exists(), text(), json(), and size() on this object, they can access this "__path" property to know which file to operate on.

    rv.set(file_obj.into());                                        /// Finally, we set the return value of the bun_file function to the file_obj we created, which is the BunFile object that represents the specified file path. 
                                                                    /// This allows JavaScript code to receive this object when they call Buk.file("path/to/file") and then call methods on it to interact with the file.
}

/// Helper function to get the file path from the BunFile object's internal field. 
/// This is used by the methods of the BunFile object to know which file they are operating on.
fn get_file_path<'s>(scope: &mut v8::HandleScope<'s>, this: v8::Local<v8::Object>) -> String {
    let path_key = v8::String::new(scope, "__path").unwrap();       /// We create a new V8 string for the internal field name "__path", which is where we stored the file path on the BunFile object. 
                                                                    /// This key is used to retrieve the file path from the object when we need to perform operations on the file.
    this.get(scope, path_key.into())                                /// We get the value of the "__path" property from the BunFile object (referred to as "this" in the context of the method calls). 
                                                                    /// This should return the file path that we stored when we created the BunFile object.
        .unwrap()                                                   /// We unwrap the result of get, which will panic if the property does not exist. In a production implementation, we should handle this error more gracefully.
        .to_string(scope)                                           /// We convert the value of the "__path" property to a V8 string, which allows us to work with it in the V8 context. This is necessary because the value is stored as a V8 string on the object.
        .unwrap()                                                   /// We unwrap the result of to_string, which will panic if the value cannot be converted to a string. In a production implementation, we should handle this error more gracefully.
        .to_rust_string_lossy(scope)                                /// Finally, we convert the V8 string to a Rust string, which allows us to work with it in Rust. The to_rust_string_lossy method will convert the V8 string to a Rust string, and if there are any invalid UTF-8 sequences, it will replace them with the Unicode replacement character. 
                                                                    /// This gives us the file path as a Rust string that we can use in our file operations.
}

/// Helper functions for the BunFile methods (exists, text, json, size) and other Bun API functions (write, which, sleep, cwd).
fn file_exists<'s>(
    scope: &mut v8::HandleScope<'s>,                                 /// The scope parameter is used to create and manage V8 values and objects within the context of the function call. 
                                                                    /// It allows us to create V8 strings, objects, and other values that are properly scoped and garbage collected.
    args: v8::FunctionCallbackArguments<'s>,                        /// Make a mutable reference to the arguments passed to the function from JavaScript. This allows us to access the arguments and their values, although in this case we will not be using any arguments since the file path is stored in the internal field of the BunFile object.
    mut rv: v8::ReturnValue,                                        /// This is used to set the return value of the function that will be returned to JavaScript. We will set this to a boolean value indicating whether the file exists or not.
) { 
    let this = args.this();                                         /// We get the "this" object from the function arguments, which should be the BunFile object that the method is called on. 
                                                                    /// For example, if we call Buk.file("path/to/file").exists(), then "this" will refer to the BunFile object returned by Buk.file("path/to/file").
    let path = get_file_path(scope, this);                          /// We call the helper function get_file_path to retrieve the file path from the internal field of the BunFile object. 
                                                                    /// This allows us to know which file we are checking for existence.
    let exists = Path::new(&path).exists();                         /// We use std::path::Path to check if the file at the specified path exists. The exists() method returns true if the file exists and false otherwise.
    rv.set(v8::Boolean::new(scope, exists).into());                 /// We set the return value of the function to a V8 boolean value that represents whether the file exists or not. 
                                                                    /// This allows JavaScript code to receive true or false when they call Buk.file("path/to/file").exists().
}


/// The following functions implement the other methods of the BunFile object (text, json, size) 
/// and the other functions of the Bun API (write, which, sleep, cwd).
fn file_text<'s>(
    scope: &mut v8::HandleScope<'s>,                                /// The scope parameter is used to create and manage V8 values and objects within the context of the function call. 
                                                                    /// It allows us to create V8 strings, objects, and other values that are properly scoped and garbage collected.
    args: v8::FunctionCallbackArguments<'s>,                        /// Make a mutable reference to the arguments passed to the function from JavaScript. This allows us to access the arguments and their values, although in this case we will not be using any arguments since the file path is stored in the internal field of the BunFile object.
    mut rv: v8::ReturnValue,                                        /// This is used to set the return value of the function that will be returned to JavaScript. We will set this to the content of the file as a string, or undefined if there is an error.   
) {
    let this = args.this();                                         /// We get the "this" object from the function arguments, which should be the BunFile object that the method is called on. 
                                                                    /// For example, if we call Buk.file("path/to/file").text(), then "this" will refer to the BunFile object returned by Buk.file("path/to/file").
    let path = get_file_path(scope, this);                          /// We call the helper function get_file_path to retrieve the file path from the internal field of the BunFile object. 
                                                                    /// This allows us to know which file we are reading the content from.

    match fs::read_to_string(&path) {                               /// We use std::fs::read_to_string to read the content of the file at the specified path. This will return a Result<String, std::io::Error>, which we can match on to handle success and error cases.
        Ok(content) => {
            let v8_str = v8::String::new(scope, &content).unwrap(); /// If the file is read successfully, we create a new V8 string with the content of the file. 
                                                                    /// This allows us to return the content to JavaScript as a string.
            rv.set(v8_str.into());                                  /// We set the return value of the function to the V8 string containing the file content, 
                                                                    /// so it becomes the result of calling Buk.file("path/to/file").text() in JavaScript.
        }
        Err(_) => rv.set(v8::undefined(scope).into()),              /// If there is an error reading the file (e.g., file does not exist, permission denied, etc.), we set the return value to undefined, 
                                                                    /// so that JavaScript code can check for this case when calling Buk.file("path/to/file").text().
    }
}

/// This function implements the json() method of the BunFile object,
/// which reads the content of the file, parses it as JSON, and returns the resulting object to JavaScript.
fn file_json<'s>(
    scope: &mut v8::HandleScope<'s>,                                /// The scope parameter is used to create and manage V8 values and objects within the context of the function call. 
                                                                    /// It allows us to create V8 strings, objects, and other values that are properly scoped and garbage collected.
    args: v8::FunctionCallbackArguments<'s>,                        /// Make a mutable reference to the arguments passed to the function from JavaScript. This allows us to access the arguments and their values, although in this case we will not be using any arguments since the file path is stored in the internal field of the BunFile object.
    mut rv: v8::ReturnValue,                                        /// This is used to set the return value of the function that will be returned to JavaScript. We will set this to the parsed JSON object if successful, or undefined if there is an error (e.g., file does not exist or content is not valid JSON).
) {
    let this = args.this();                                         /// We get the "this" object from the function arguments, which should be the BunFile object that the method is called on. 
                                                                    /// For example, if we call Buk.file("path/to/file").json(), then "this" will refer to the BunFile object returned by Buk.file("path/to/file").
    let path = get_file_path(scope, this);                          /// We call the helper function get_file_path to retrieve the file path from the internal field of the BunFile object. 
                                                                    /// This allows us to know which file we are reading and parsing as JSON.

    match fs::read_to_string(&path) {                               /// We use std::fs::read_to_string to read the content of the file at the specified path. This will return a Result<String, std::io::Error>, which we can match on to handle success and error cases.
        Ok(content) => {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) { /// If the file is read successfully, we attempt to parse the content as JSON using serde_json::from_str. 
                                                                                    /// This will return a Result<serde_json::Value, serde_json::Error>, which we can check for success. 
                                                                                    /// If the content is valid JSON, we will get a serde_json::Value that represents the parsed JSON data.
                let json_str = json.to_string();
                let v8_str = v8::String::new(scope, &json_str).unwrap();            /// We convert the parsed JSON value back to a string, and then create a new V8 string with that JSON string. 
                                                                                    /// This allows us to pass the JSON data back to JavaScript as a string, which we can then parse in JavaScript to get the actual object.
                // Parse JSON in V8
                let json_parse = v8::String::new(scope, "JSON.parse").unwrap();     /// We create a new V8 string for "JSON.parse", which is the built-in JavaScript function for parsing JSON strings into JavaScript objects. 
                                                                                    /// We will use this function to parse the JSON string we created from the serde_json::Value, so that we can return a proper JavaScript object to JavaScript code instead of just a string.
                if let Some(parse_fn) = json_parse                                  /// We get the "JSON.parse" function from the global object, convert it to a V8 function, and check if it exists.
                    .to_object(scope)                                               /// We convert the "JSON.parse" string to a V8 object, which should be the JSON object in JavaScript.
                    .and_then(|obj| v8::Local::<v8::Function>::try_from(obj).ok())  /// We attempt to convert the JSON object to a V8 function, which should succeed since JSON.parse is a function in JavaScript. If this succeeds, we get a Local<Function> that we can call.
                {
                    let recv = v8::undefined(scope);                                /// The receiver for the function call will be undefined, since JSON.parse does not rely on "this".
                    let args = [v8_str.into()];                                     /// The arguments for the function call will be an array containing the JSON string we created from the serde_json::Value. 
                                                                                    /// This means we will call JSON.parse(json_str) in JavaScript to parse the JSON string into a JavaScript object.   
                    if let Some(result) = parse_fn.call(scope, recv.into(), &args) {
                        rv.set(result);                                             /// If the call to JSON.parse is successful, we set the return value to the result of that call, which should be the parsed JavaScript object.
                        return;                                                     /// We return early from the function since we have successfully parsed the JSON and set the return value. 
                                                                                    /// If there was an error at any point (e.g., file read error, JSON parse error), we will fall through to the end of the function where we set the return value to undefined.   
                    }
                }
            }
        }
        /// If there is an error reading the file or parsing the JSON, we will end up here. 
        /// We simply ignore the error and set the return value to undefined at the end of the function.
        Err(_) => {}
    }
    rv.set(v8::undefined(scope).into());                                        /// If there was an error at any point (e.g., file read error, JSON parse error), we set the return value to undefined, 
                                                                                /// so that JavaScript code can check for this case when calling Buk.file("path/to/file").json().   
}

/// This function implements the size() method of the BunFile object, which returns the size of the file in bytes as a number, 
/// or -1 if there is an error (e.g., file does not exist).
fn file_size<'s>(
    scope: &mut v8::HandleScope<'s>,                                         /// The scope parameter is used to create and manage V8 values and objects within the context of the function call. 
                                                                            /// It allows us to create V8 strings, objects, and other values that are properly scoped and garbage collected.    
    args: v8::FunctionCallbackArguments<'s>,                                /// Make a mutable reference to the arguments passed to the function from JavaScript. This allows us to access the arguments and their values, although in this case we will not be using any arguments since the file path is stored in the internal field of the BunFile object.
    mut rv: v8::ReturnValue,                                                /// This is used to set the return value of the function that will be returned to JavaScript. We will set this to the size of the file in bytes as a number, or -1 if there is an error.
) {
    let this = args.this();                                                 /// We get the "this" object from the function arguments, which should be the BunFile object that the method is called on. 
                                                                            /// For example, if we call Buk.file("path/to/file").size(), then "this" will refer to the BunFile object returned by Buk.file("path/to/file").
    let path = get_file_path(scope, this);                                  /// We call the helper function get_file_path to retrieve the file path from the internal field of the BunFile object. 
                                                                            /// This allows us to know which file we are checking the size of.

    match fs::metadata(&path) {                                             /// We use std::fs::metadata to get the metadata of the file at the specified path. This will return a Result<Metadata, std::io::Error>, which we can match on to handle success and error cases.
        Ok(meta) => {                                                       /// If we successfully get the metadata of the file, we can access its length (size in bytes) using the len() method.
            let size = meta.len() as f64;                                   /// We convert the file size to f64 because V8 numbers are represented as double-precision floating-point values. 
                                                                            /// This allows us to return the file size as a number to JavaScript.
            rv.set(v8::Number::new(scope, size).into());                    /// We set the return value of the function to a V8 number that represents the size of the file in bytes, 
                                                                            /// so it becomes the result of calling Buk.file("path/to/file").size() in JavaScript.
        }
        Err(_) => rv.set(v8::Number::new(scope, -1.0).into()),              /// If there is an error getting the metadata of the file (e.g., file does not exist), we set the return value to -1, 
                                                                            /// so that JavaScript code can check for this case when calling Buk.file("path/to/file").size().
    }
}

/// This function implements the write() method of the Bun API, which writes a string to a file at the specified path. 
/// It takes two arguments: the file path and the data to write. 
/// It returns true if the write operation was successful, or false if there was an error.
fn bun_write<'s>(
    scope: &mut v8::HandleScope<'s>,                                        /// The scope parameter is used to create and manage V8 values and objects within the context of the function call. 
                                                                            /// It allows us to create V8 strings, objects, and other values that are properly scoped and garbage collected.
    args: v8::FunctionCallbackArguments<'s>,                                /// Make a mutable reference to the arguments passed to the function from JavaScript. This allows us to access the arguments and their values, which should be the file path and the data to write.
    mut rv: v8::ReturnValue,                                                /// This is used to set the return value of the function that will be returned to JavaScript. We will set this to true if the write operation was successful, or false if there was an error.   
) {
    if args.length() < 2 {                                                  /// We check if at least two arguments are passed to the function, which should be the file path and the data to write. If not, we return false.
        rv.set(v8::Boolean::new(scope, false).into());                      /// If there are not enough arguments, we set the return value to false, indicating that the write operation was not successful due to missing arguments.
        return;                                                             /// We return early from the function, since we cannot proceed without both the file path and the data to write.      
    }

    /// We get the file path and data from the arguments, convert them to Rust strings, 
    /// and then use std::fs::write to write the data to the specified file path.
    let path = args
        .get(0)                                                             /// We get the first argument, which should be the file path, and convert it to a Rust string using the same process as before (to_string, unwrap, to_rust_string_lossy).
        .to_string(scope)                                                   /// We convert the first argument to a V8 string, which allows us to work with it in the V8 context. This is necessary because the argument is passed from JavaScript as a V8 value.
        .unwrap()                                                           /// We unwrap the result of to_string, which will panic if the argument cannot be converted to a string. In a production implementation, we should handle this error more gracefully.
        .to_rust_string_lossy(scope);                                       /// We convert the V8 string to a Rust string, which allows us to work with it in Rust. The to_rust_string_lossy method will convert the V8 string to a Rust string, and if there are any invalid UTF-8 sequences, it will replace them with the Unicode replacement character. 
                                                                            /// This gives us the file path as a Rust string that we can use in our file operations.
    let data = args                                                         /// We get the second argument, which should be the data to write, and convert it to a Rust string using the same process as before (to_string, unwrap, to_rust_string_lossy).
        .get(1)                                                             /// We get the second argument, which should be the data to write, and convert it to a Rust string using the same process as before (to_string, unwrap, to_rust_string_lossy).       
        .to_string(scope)                                                   /// We convert the second argument to a V8 string, which allows us to work with it in the V8 context. This is necessary because the argument is passed from JavaScript as a V8 value.
        .unwrap()                                                           /// We unwrap the result of to_string, which will panic if the argument cannot be converted to a string. In a production implementation, we should handle this error more gracefully.
        .to_rust_string_lossy(scope);                                       /// We convert the V8 string to a Rust string, which allows us to work with it in Rust. The to_rust_string_lossy method will convert the V8 string to a Rust string, and if there are any invalid UTF-8 sequences, it will replace them with the Unicode replacement character. 
                                                                            /// This gives us the data to write as a Rust string that we can use in our file operations.

    match fs::write(&path, data) {                                          /// We use std::fs::write to write the data to the specified file path. This will return a Result<(), std::io::Error>, which we can match on to determine if the write operation was successful or if there was an error.
        Ok(_) => rv.set(v8::Boolean::new(scope, true).into()),              /// If the write operation was successful, we set the return value to true, indicating that the data was successfully written to the file.
        Err(_) => rv.set(v8::Boolean::new(scope, false).into()),            /// If there was an error during the write operation (e.g., permission denied, invalid path, etc.), we set the return value to false, indicating that the write operation was not successful.
    }
}

/// This function implements the which() method of the Bun API, 
/// Which performs a simple search for an executable in the system's PATH environment variable.
/// It takes one argument, which is the name of the command to search for, and returns
fn bun_which<'s>(
    scope: &mut v8::HandleScope<'s>,                                    /// The scope parameter is used to create and manage V8 values and objects within the context of the function call. 
                                                                        /// It allows us to create V8 strings, objects, and other values that are properly scoped and garbage collected.    
    args: v8::FunctionCallbackArguments<'s>,                            /// Make a mutable reference to the arguments passed to the function from JavaScript. This allows us to access the arguments and their values, which should be the name of the command to search for.
    mut rv: v8::ReturnValue,                                            /// This is used to set the return value of the function that will be returned to JavaScript. We will set this to the full path of the command if found, or undefined if not found.
) { 
    if args.length() < 1 {                                              /// We check if at least one argument is passed to the function, which should be the name of the command to search for. If not, we return undefined.          
        rv.set(v8::undefined(scope).into());                            /// If there are no arguments, we set the return value to undefined, indicating that we cannot perform the search without a command name.
        return;                                                         /// We return early from the function, since we cannot proceed without the command name to search for.
    }

    let cmd = args                                                      /// We get the first argument, which should be the name of the command to search for, and convert it to a Rust string using the same process as before (to_string, unwrap, to_rust_string_lossy).
        .get(0)                                                         /// We get the first argument, which should be the name of the command to search for, and convert it to a Rust string using the same process as before (to_string, unwrap, to_rust_string_lossy).
        .to_string(scope)                                               /// We convert the first argument to a V8 string, which allows us to work with it in the V8 context. This is necessary because the argument is passed from JavaScript as a V8 value.
        .unwrap()                                                       /// We unwrap the result of to_string, which will panic if the argument cannot be converted to a string. In a production implementation, we should handle this error more gracefully.
        .to_rust_string_lossy(scope);                                   /// We convert the V8 string to a Rust string, which allows us to work with it in Rust. The to_rust_string_lossy method will convert the V8 string to a Rust string, and if there are any invalid UTF-8 sequences, it will replace them with the Unicode replacement character. 
                                                                        /// This gives us the command name as a Rust string that we can use in our search for the executable.

    // Simple PATH search
    if let Ok(path_var) = std::env::var("PATH") {                       /// We get the value of the PATH environment variable using std::env::var. This will return a Result<String, std::env::VarError>, which we can check for success. If we successfully get the PATH variable, we can proceed to search for the command in the directories listed in PATH.
        let separator = if cfg!(windows) { ';' } else { ':' };          /// The PATH variable is a list of directories separated by a specific character. On Windows, the separator is ';', while on Unix-like systems, it is ':'. We determine the correct separator based on the target platform using cfg!(windows).
        for path in path_var.split(separator) {                         /// We split the PATH variable into individual directories using the determined separator, and iterate over each directory in the PATH.
            let full_path = Path::new(path).join(&cmd);                 /// For each directory in the PATH, we create a full path by joining the directory with the command name. This gives us a potential full path to the executable we are searching for.
            if full_path.exists() {                                     /// We check if the file at the full path exists. If it does, we have found the executable we are looking for.    
                let path_str = v8::String::new(scope, full_path.to_str().unwrap_or("")).unwrap();   /// If the file exists, we convert the full path to a string and then create a new V8 string with that path. This allows us to return the full path of the command to JavaScript.
                rv.set(path_str.into());                                /// We set the return value of the function to the V8 string containing the full path of the command, 
                                                                        /// so it becomes the result of calling Buk.which("command") in JavaScript./
                return;                                                 /// We return early from the function since we have found the command and set the return value. If we finish iterating through all directories in PATH without finding the command, we will fall through to the end of the function where we set the return value to undefined.
            }
        }
    }

    rv.set(v8::undefined(scope).into());                                /// If we finish iterating through all directories in PATH without finding the command, or if there was an error getting the PATH variable, we set the return value to undefined, 
                                                                        /// so that JavaScript code can check for this case when calling Buk.which("command").
}

/// This function implements the sleep() method of the Bun API, 
/// which blocks the current thread for a specified number of milliseconds.
fn bun_sleep<'s>(
    scope: &mut v8::HandleScope<'s>,                                        /// The scope parameter is used to create and manage V8 values and objects within the context of the function call. 
                                                                            /// It allows us to create V8 strings, objects, and other values that are properly scoped and garbage collected.
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,                                                /// This is used to set the return value of the function that will be returned to JavaScript. We will set this to undefined, since the sleep function does not return any meaningful value. 
) {
    if args.length() < 1 {                                                  /// We check if at least one argument is passed to the function, which should be the number of milliseconds to sleep. If not, we return undefined.
        rv.set(v8::undefined(scope).into());                                /// If there are no arguments, we set the return value to undefined, indicating that we cannot perform the sleep operation without a duration.    
        return;                                                             /// We return early from the function, since we cannot proceed without the duration to sleep for.
    }

    let ms = args.get(0).number_value(scope).unwrap_or(0.0) as u64;         /// We get the first argument, which should be the number of milliseconds to sleep, and convert it to a Rust u64. 
                                                                            /// We use number_value to convert the argument to a V8 number, unwrap it, and then cast it to u64. If the argument cannot be converted to a number, we default to 0 milliseconds.
    std::thread::sleep(std::time::Duration::from_millis(ms));               /// We use std::thread::sleep to block the current thread for the specified duration in milliseconds. We create a Duration using std::time::Duration::from_millis with the ms value we obtained from the arguments.

    rv.set(v8::undefined(scope).into());                                    /// After sleeping for the specified duration, we set the return value to undefined, since the sleep function does not return any meaningful value. 
                                                                            /// This allows JavaScript code to call Buk.sleep(1000) and simply wait for 1 second without expecting any return value.
}

/// This function implements the cwd() method of the Bun API, 
/// which returns the current working directory as a string, or undefined if there is an error.
fn bun_cwd<'s>(
    scope: &mut v8::HandleScope<'s>,                                        /// The scope parameter is used to create and manage V8 values and objects within the context of the function call. 
                                                                            /// It allows us to create V8 strings, objects, and other values that are properly scoped and garbage collected.
    _args: v8::FunctionCallbackArguments<'s>,                               /// We take the function arguments, but we do not use them in this function since cwd() does not require any arguments. We name it _args to indicate that it is intentionally unused.
    mut rv: v8::ReturnValue,                                                /// This is used to set the return value of the function that will be returned to JavaScript. We will set this to the current working directory as a string, or undefined if there is an error.
) {
    if let Ok(cwd) = std::env::current_dir() {                              /// We use std::env::current_dir to get the current working directory. This will return a Result<PathBuf, std::io::Error>, which we can check for success. If we successfully get the current working directory, we can proceed to convert it to a string and return it to JavaScript.
        let cwd_str = cwd.to_string_lossy().to_string();                    /// We convert the PathBuf to a string using to_string_lossy, which will convert it to a Rust String. If there are any invalid UTF-8 sequences in the path, they will be replaced with the Unicode replacement character. 
                                                                            /// This gives us the current working directory as a Rust string that we can use to create a V8 string to return to JavaScript.
        let v8_str = v8::String::new(scope, &cwd_str).unwrap();             /// We create a new V8 string with the current working directory string. This allows us to return the current working directory to JavaScript as a string.
        rv.set(v8_str.into());                                              /// We set the return value of the function to the V8 string containing the current working directory, 
                                                                            /// so it becomes the result of calling Buk.cwd() in JavaScript.
    } else {                                                                /// If there was an error getting the current working directory (e.g., permission denied), we set the return value to undefined, 
                                                                            /// so that JavaScript code can check for this case when calling Buk.cwd().
        rv.set(v8::undefined(scope).into());                                /// 
    }
}
