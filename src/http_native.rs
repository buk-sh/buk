use anyhow::Result;
use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use v8;

pub struct HttpNativeFramework;

impl HttpNativeFramework {
    pub fn init<'s>(scope: &mut v8::HandleScope<'s>, global: v8::Local<'s, v8::Object>) {
        // createApp() function
        let name = v8::String::new(scope, "createApp").unwrap();
        let func = v8::FunctionTemplate::new(scope, create_app);
        let func = func.get_function(scope).unwrap();
        global.set(scope, name.into(), func.into());
    }
}

fn create_app<'s>(
    scope: &mut v8::HandleScope<'s>,
    _args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    let app_template = v8::ObjectTemplate::new(scope);

    // app.get(path, handler)
    let name = v8::String::new(scope, "get").unwrap();
    let func = v8::FunctionTemplate::new(scope, app_get);
    app_template.set(name.into(), func.into());

    // app.post(path, handler)
    let name = v8::String::new(scope, "post").unwrap();
    let func = v8::FunctionTemplate::new(scope, app_post);
    app_template.set(name.into(), func.into());

    // app.put(path, handler)
    let name = v8::String::new(scope, "put").unwrap();
    let func = v8::FunctionTemplate::new(scope, app_put);
    app_template.set(name.into(), func.into());

    // app.delete(path, handler)
    let name = v8::String::new(scope, "delete").unwrap();
    let func = v8::FunctionTemplate::new(scope, app_delete);
    app_template.set(name.into(), func.into());

    // app.use(middleware)
    let name = v8::String::new(scope, "use").unwrap();
    let func = v8::FunctionTemplate::new(scope, app_use);
    app_template.set(name.into(), func.into());

    // app.group(prefix, callback)
    let name = v8::String::new(scope, "group").unwrap();
    let func = v8::FunctionTemplate::new(scope, app_group);
    app_template.set(name.into(), func.into());

    // app.listen() - returns listener builder
    let name = v8::String::new(scope, "listen").unwrap();
    let func = v8::FunctionTemplate::new(scope, app_listen);
    app_template.set(name.into(), func.into());

    // app.error(handler)
    let name = v8::String::new(scope, "error").unwrap();
    let func = v8::FunctionTemplate::new(scope, app_error);
    app_template.set(name.into(), func.into());

    let app_obj = app_template.new_instance(scope).unwrap();

    // Routes storage - store in internal field on instance
    let routes_key = v8::String::new(scope, "__routes").unwrap();
    let routes = v8::Array::new(scope, 0);
    app_obj.set(scope, routes_key.into(), routes.into());

    // Middleware storage
    let mw_key = v8::String::new(scope, "__middleware").unwrap();
    let middleware = v8::Array::new(scope, 0);
    app_obj.set(scope, mw_key.into(), middleware.into());

    rv.set(app_obj.into());
}

fn add_route<'s>(
    scope: &mut v8::HandleScope<'s>,
    this: v8::Local<'s, v8::Object>,
    method: &str,
    path: v8::Local<'s, v8::Value>,
    handler: v8::Local<'s, v8::Value>,
) {
    let routes_key = v8::String::new(scope, "__routes").unwrap();
    let routes = this
        .get(scope, routes_key.into())
        .unwrap()
        .to_object(scope)
        .unwrap();

    let len_key = v8::String::new(scope, "length").unwrap();
    let len = routes
        .get(scope, len_key.into())
        .unwrap()
        .number_value(scope)
        .unwrap_or(0.0) as u32;

    let route = v8::Object::new(scope);
    let method_key = v8::String::new(scope, "method").unwrap();
    let method_val = v8::String::new(scope, method).unwrap();
    route.set(scope, method_key.into(), method_val.into());

    let path_key = v8::String::new(scope, "path").unwrap();
    route.set(scope, path_key.into(), path);

    let handler_key = v8::String::new(scope, "handler").unwrap();
    route.set(scope, handler_key.into(), handler);

    let idx = v8::Number::new(scope, len as f64);
    routes.set(scope, idx.into(), route.into());
}

fn app_get<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    if args.length() >= 2 {
        let path = args.get(0);
        let handler = args.get(1);
        add_route(scope, this, "GET", path, handler);
    }
    rv.set(this.into());
}

fn app_post<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    if args.length() >= 2 {
        let path = args.get(0);
        let handler = args.get(1);
        add_route(scope, this, "POST", path, handler);
    }
    rv.set(this.into());
}

fn app_put<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    if args.length() >= 2 {
        let path = args.get(0);
        let handler = args.get(1);
        add_route(scope, this, "PUT", path, handler);
    }
    rv.set(this.into());
}

fn app_delete<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    if args.length() >= 2 {
        let path = args.get(0);
        let handler = args.get(1);
        add_route(scope, this, "DELETE", path, handler);
    }
    rv.set(this.into());
}

fn app_use<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    if args.length() >= 1 {
        let mw_key = v8::String::new(scope, "__middleware").unwrap();
        let middleware = this
            .get(scope, mw_key.into())
            .unwrap()
            .to_object(scope)
            .unwrap();

        let len_key = v8::String::new(scope, "length").unwrap();
        let len = middleware
            .get(scope, len_key.into())
            .unwrap()
            .number_value(scope)
            .unwrap_or(0.0) as u32;

        let mw = args.get(0);
        let idx = v8::Number::new(scope, len as f64);
        middleware.set(scope, idx.into(), mw);
    }
    rv.set(this.into());
}

fn app_group<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    if args.length() >= 2 {
        let _prefix = args.get(0).to_string(scope).unwrap().to_rust_string_lossy(scope);
        let callback = args.get(1);

        // Call the callback with this app (simplified - full implementation would create sub-router)
        if let Some(callback_fn) = callback.to_object(scope).and_then(|o| v8::Local::<v8::Function>::try_from(o).ok()) {
            let recv = v8::undefined(scope);
            let call_args = [this.into()];
            callback_fn.call(scope, recv.into(), &call_args);
        }
    }
    rv.set(this.into());
}

fn app_error<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();
    if args.length() >= 1 {
        let error_key = v8::String::new(scope, "__error_handler").unwrap();
        let handler = args.get(0);
        this.set(scope, error_key.into(), handler);
    }
    rv.set(this.into());
}

fn app_listen<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();

    // Create listener builder
    let builder_template = v8::ObjectTemplate::new(scope);

    // .port(n) method
    let name = v8::String::new(scope, "port").unwrap();
    let func = v8::FunctionTemplate::new(scope, listener_port);
    builder_template.set(name.into(), func.into());

    let builder_obj = builder_template.new_instance(scope).unwrap();

    // Store app reference on the instance (not template)
    let app_key = v8::String::new(scope, "__app").unwrap();
    builder_obj.set(scope, app_key.into(), this.into());

    rv.set(builder_obj.into());
}

fn listener_port<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    let this = args.this();

    let port = if args.length() >= 1 {
        args.get(0).number_value(scope).unwrap_or(3000.0) as u16
    } else {
        3000
    };

    // Store port
    let port_key = v8::String::new(scope, "__port").unwrap();
    let port_val = v8::Number::new(scope, port as f64);
    this.set(scope, port_key.into(), port_val.into());

    // Start server (simplified - doesn't call JS handlers yet)
    let addr: SocketAddr = format!("0.0.0.0:{}", port).parse().unwrap();

    // Start server
    let runtime = tokio::runtime::Handle::current();
    runtime.spawn(async move {
        if let Err(e) = run_http_native_server(addr).await {
            eprintln!("Server error: {}", e);
        }
    });

    println!("🚀 http-native server running on http://localhost:{}/", port);

    // Return server info object
    let server_template = v8::ObjectTemplate::new(scope);
    let server_obj = server_template.new_instance(scope).unwrap();

    let url_key = v8::String::new(scope, "url").unwrap();
    let url_val = v8::String::new(scope, &format!("http://localhost:{}/", port)).unwrap();
    server_obj.set(scope, url_key.into(), url_val.into());

    let port_key = v8::String::new(scope, "port").unwrap();
    server_obj.set(scope, port_key.into(), port_val.into());

    rv.set(server_obj.into());
}

async fn run_http_native_server(addr: SocketAddr) -> Result<()> {
    let listener = TcpListener::bind(addr).await?;

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);

        tokio::spawn(async move {
            let service = service_fn(handle_http_native_request);

            if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
                eprintln!("Error serving connection: {:?}", err);
            }
        });
    }
}

async fn handle_http_native_request(
    req: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    // For now, return a basic response
    // Full implementation would route through the JS handlers
    let path = req.uri().path();
    let method = req.method().as_str();

    let response_body = format!(
        r#"{{"path": "{}", "method": "{}", "framework": "http-native"}}"#,
        path, method
    );

    let response = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::from(response_body)))
        .unwrap();

    Ok(response)
}
