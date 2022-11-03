use telegram_bot_raw::{Body, HttpRequest};
use worker::wasm_bindgen::JsValue;
use worker::wasm_bindgen_futures::spawn_local;
use worker::{Fetch, Headers, Method, Request, RequestInit, Url};

pub(crate) fn req(r: HttpRequest, tok: String) {
    spawn_local(async move {
        let method = match r.method {
            telegram_bot_raw::Method::Get => Method::Get,
            telegram_bot_raw::Method::Post => Method::Post,
        };
        let ri = match r.body {
            Body::Json(s) => {
                let mut headers = Headers::new();
                headers
                    .append("content-type", "application/json")
                    .expect("headers!");
                RequestInit {
                    body: Some(JsValue::from(s)),
                    headers,
                    method,
                    ..RequestInit::default()
                }
            }
            _ => RequestInit {
                method,
                ..RequestInit::default()
            },
        };
        let mut wr = Request::new_with_init(&r.url.url(&tok), &ri).expect("url constructed");
        Fetch::Request(wr).send().await.expect("error sending!");
    })
}

pub(crate) async fn get(uri: &str) -> String {
    Fetch::Url(Url::parse(uri).expect("malformed url"))
        .send()
        .await
        .expect("request error")
        .text()
        .await
        .expect("reading text")
}
