use std::error::Error;
use std::fmt::format;
use std::net::SocketAddr;
use std::sync::Arc;
use axum::{Extension, Json, Router, routing::{get, post}};
use axum::response::IntoResponse;
use reqwest::StatusCode;
use scraper::{ElementRef, Html, Node, Selector};
use telegram_bot::{Api, MessageChat, MessageKind, SendMessage, Update, UpdateKind};
use ego_tree::{
    NodeRef
};

macro_rules! tryp {
    ($expr:expr $(,)?) => {
        match $expr {
            Ok(val) => val,
            Err(err) => {
                println!("Err {:?}", err);
                return
            }
        }
    };
}

macro_rules! opp {
    ($expr:expr $(,)?) => {
        match $expr {
            Some(val) => val,
            None => {
                return
            }
        }
    };
}

fn make_selector(selector: &'static str) -> Selector {
    Selector::parse(selector).expect("bad selector")
}

struct AppState {
    api: Api,
    fin_sel: Selector,
}


// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}

async fn handle(update: Update, state: Arc<AppState>) {
    let m = match update.kind {
        UpdateKind::Message(m) => m,
        _ => return
    };
    let text = match m.kind {
        MessageKind::Text { data, entities } => { data }
        _ => return
    };
    let is_group = matches!(m.chat, MessageChat::Group(_) | MessageChat::Supergroup(_));
    let q = if is_group {
        if let Some(text) = text.strip_prefix("/fw ") {
            text
        } else {
            return;
        }
    } else {
        &text
    };

    let client = reqwest::Client::builder().gzip(true).build().expect("Unable to create client");
    let a = tryp!(client.get(&format!("https://en.wiktionary.org/w/index.php?search={q}&go=Go"))
        .send()
        .await);
    let text = tryp!(a.text().await);
    let dom = Html::parse_document(&text);
    let mut el = dom.select(&state.fin_sel);

    let par = match el.next().and_then(|o| o.parent()) {
        Some(x) => x,
        None => return
    };
    state.api.send(SendMessage::new(
        m.chat,
        format!("{q} found"),
    )).await;

    let mut add = false;
    let mut content = String::new();

    for node in par.next_siblings() {
        if let Some(el) = node.value().as_element() {
            if "h2" == &el.name.local {
                break;
            }
            if &el.name.local == "h3" {
                let s: String = first_element_child(node).map(|e| e.inner_html()).unwrap_or("".into());
                add = s != "Etymology" || s != "Pronunciation" || s != "";
                if add {
                    content += &format!("*{s}*");
                }
                continue
            }
            
        }
    }

    ;
}

fn first_element_child<'a>(node: NodeRef<'a, Node>) -> Option<ElementRef<'a>> {
    ElementRef::wrap(ElementRef::wrap(node)?.first_child()?)
}

async fn get_update(
    // this argument tells axum to parse the request body
    // as JSON into a `CreateUser` type
    Json(update): Json<Update>,
    Extension(state): Extension<Arc<AppState>>,
) -> impl IntoResponse {
    // insert your application logic here
    handle(update, state.clone()).await;
    // this will be converted into a JSON response
    // with a status code of `201 Created`
    (StatusCode::OK, Json(""))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let token = std::env::var("BOT_TOKEN").expect("Wrong token string");
    let origin = std::env::var("ORIGIN").expect("Wrong origin string");
    let path = std::env::var("SECRET_PATH").expect("Wrong secret string");
    let port = std::env::var("PORT").expect("Wrong port set").parse().expect("Port is not a number");
    let hook_url = format!("https://api.telegram.org/bot{token}/setWebhook?url={origin}{path}");
    let api = Api::new(token);
    let client = reqwest::Client::builder().gzip(true).build()?;
    println!("Setting up hook: {:?}", client.get(hook_url).send().await?.text().await?);


    let app = Router::new()
        // `GET /` goes to `root`
        .route("/health", get(root))
        // `POST /users` goes to `get_update`
        .route(&path, post(get_update))
        .layer(Extension(Arc::new(AppState {
            api,
            fin_sel: make_selector("#Finnish"),
        })));

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    //tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();


    Ok(())
    //let mut stream = api.stream();

    // loop {
    //     let update = match stream.next().await {
    //         Some(Ok(u)) => u,
    //         other => {
    //             println!("{:?}", other);
    //             continue;
    //         }
    //     };
    //     let m= match  update.kind {
    //         UpdateKind::Message(m) => m,
    //         _ => continue;
    //     }
    //         let text = match m.kind {
    //             MessageKind::Text { data, entities } => { data }
    //             _ => continue
    //         };
    //         let is_group = matches!(m.chat, MessageChat::Group(_) | MessageChat::Supergroup(_));
    //         let q = if is_group {
    //             if let Some(text) = text.strip_prefix("fw ") {
    //                 text
    //             } else {
    //                 continue;
    //             }
    //         } else {
    //             &text
    //         };
    //
    //
    //     //api.stream()
    // }
}
