use anyhow::Result;
use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use v8;

pub struct BunServer;

impl BunServer {
    pub fn init<'s>(scope: &mut v8::HandleScope<'s>, global: v8::Local<'s, v8::Object>) {
        let name = v8::String::new(scope, "serve").unwrap();
        let func = v8::FunctionTemplate::new(scope, bun_serve);
        let func = func.get_function(scope).unwrap();
        global.set(scope, name.into(), func.into());
    }
}

fn bun_serve<'s>(
    scope: &mut v8::HandleScope<'s>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    if args.length() < 1 {
        rv.set(v8::undefined(scope).into());
        return;
    }

    let options = args.get(0).to_object(scope).unwrap();

    // Get port
    let port_key = v8::String::new(scope, "port").unwrap();
    let port = options
        .get(scope, port_key.into())
        .unwrap()
        .number_value(scope)
        .unwrap_or(3000.0) as u16;

    // Get hostname
    let hostname_key = v8::String::new(scope, "hostname").unwrap();
    let hostname = options
        .get(scope, hostname_key.into())
        .unwrap_or_else(|| v8::String::new(scope, "0.0.0.0").unwrap().into())
        .to_string(scope)
        .unwrap()
        .to_rust_string_lossy(scope);

    let addr: SocketAddr = format!("{}:{}", hostname, port).parse().unwrap();

    // Create server object
    let server_template = v8::ObjectTemplate::new(scope);

    // server.stop() method
    let stop_name = v8::String::new(scope, "stop").unwrap();
    let stop_func = v8::FunctionTemplate::new(scope, server_stop);
    server_template.set(stop_name.into(), stop_func.into());

    // Store server info
    let server_obj = server_template.new_instance(scope).unwrap();
    let port_val = v8::Number::new(scope, port as f64);
    let port_key = v8::String::new(scope, "port").unwrap();
    server_obj.set(scope, port_key.into(), port_val.into());

    let hostname_val = v8::String::new(scope, &hostname).unwrap();
    let hostname_key = v8::String::new(scope, "hostname").unwrap();
    server_obj.set(scope, hostname_key.into(), hostname_val.into());

    // Start server in background task
    let runtime = tokio::runtime::Handle::current();

    runtime.spawn(async move {
        if let Err(e) = run_server(addr).await {
            eprintln!("Server error: {}", e);
        }
    });

    println!("Server running at http://{}:{}/", hostname, port);
    rv.set(server_obj.into());
}

async fn run_server(addr: SocketAddr) -> Result<()> {
    let listener = TcpListener::bind(addr).await?;

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);

        let service = service_fn(handle_request);

        tokio::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(io, service)
                .await
            {
                eprintln!("Error serving connection: {:?}", err);
            }
        });
    }
}

async fn handle_request(
    req: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    // For now, simple static response
    // In real implementation, would call the JS fetch handler
    let path = req.uri().path();
    let method = req.method().as_str();

    let response_body = format!(
        r#"{{"path": "{}", "method": "{}", "message": "Bun.serve() placeholder - JS handler not yet fully implemented"}}"#,
        path, method
    );

    let response = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::from(response_body)))
        .unwrap();

    Ok(response)
}

fn server_stop<'s>(
    scope: &mut v8::HandleScope<'s>,
    _args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue,
) {
    println!("Server stopping (placeholder)");
    rv.set(v8::undefined(scope).into());
}
