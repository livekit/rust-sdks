use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use hyper::{Method, StatusCode};
use livekit_api::access_token::TokenVerifier;
use livekit_api::webhooks;
use std::convert::Infallible;
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    let make_svc = make_service_fn(|_conn| async { Ok::<_, Infallible>(service_fn(handle)) });
    let server = Server::bind(&addr).serve(make_svc);

    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}

async fn handle(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    match (req.method(), req.uri().path()) {
        (&Method::POST, "/") => {
            let token_verifier = TokenVerifier::new().unwrap();
            let webhook_receiver = webhooks::WebhookReceiver::new(token_verifier);

            let jwt = req
                .headers()
                .get("Authorization")
                .and_then(|hv| hv.to_str().ok())
                .unwrap_or_default()
                .to_string();

            let jwt = jwt.trim();

            println!("Received request with jwt: {}", jwt);

            let body = hyper::body::to_bytes(req.into_body()).await.unwrap();
            let body = std::str::from_utf8(&body).unwrap();

            let res = webhook_receiver.receive(&body, &jwt);
            if let Ok(event) = res {
                println!("Received event: {:?}", event);
                Ok(Response::new(Body::from("OK")))
            } else {
                println!("Failed to receive event: {:?}", res);
                Ok(Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body(Body::from("Invalid request"))
                    .unwrap())
            }
        }
        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Http method not supported"))
            .unwrap()),
    }
}
