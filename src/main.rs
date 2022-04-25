use axum::response::IntoResponse;
use axum::{
    routing::{get, post},
    Extension, Json, Router,
};
use ego_tree::NodeRef;
use reqwest::Client;
use reqwest::StatusCode;
use scraper::{ElementRef, Html, Node, Selector};
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use telegram_bot::ParseMode::Markdown;
use telegram_bot::{Api, MessageChat, MessageKind, SendMessage, Update, UpdateKind};

fn make_selector(selector: &'static str) -> Selector {
    Selector::parse(selector).expect("bad selector")
}

struct AppState {
    api: Api,
    fin_sel: Selector,
    client: Client,
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}

fn first_element_child(node: NodeRef<Node>) -> Option<ElementRef> {
    ElementRef::wrap(ElementRef::wrap(node)?.first_child()?)
}

async fn get_update(
    Json(update): Json<Update>,
    Extension(state): Extension<Arc<AppState>>,
    // (Json(update), Extension(state)): (Json<Update>, Extension<Arc<AppState>>)
) -> impl IntoResponse {
    println!("{:?}", update);
    let ret = (StatusCode::OK, Json(""));
    // insert your application logic here
    let m = match update.kind {
        UpdateKind::Message(m) => m,
        _ => {
            println!("Not a message");
            return ret;
        }
    };
    let text = match m.kind {
        MessageKind::Text { data, entities: _ } => data,
        _ => {
            println!("Not a text");
            return ret;
        }
    };
    let is_group = matches!(m.chat, MessageChat::Group(_) | MessageChat::Supergroup(_));
    let q = if is_group {
        if let Some(text) = text.strip_prefix("/fw ") {
            text
        } else {
            println!("Bad Query: {:?}", text);
            return ret;
        }
    } else {
        &text
    };

    println!("Query: {:?}", q);
    let text = match state
        .client
        .get(&format!(
            "https://en.wiktionary.org/w/index.php?search={q}&go=Go"
        ))
        .send()
        .await
    {
        Ok(val) => val,
        Err(err) => {
            println!("Err {:?}", err);
            return ret;
        }
    }
    .text()
    .await
    .expect("failed to read text");
    let html = Html::parse_document(&text);
    let mut el = html.select(&state.fin_sel);

    let par = match el.next().and_then(|o| o.parent()) {
        Some(x) => x,
        None => {
            println!("No parent of finish found");
            return ret;
        }
    };

    let mut add = false;
    let mut content = String::new();

    for node in par.next_siblings() {
        if let Some(el) = node.value().as_element() {
            if "h2" == &el.name.local {
                break;
            }
            if &el.name.local == "h3" || &el.name.local == "h4" {
                let s: String = first_element_child(node)
                    .map(|e| e.inner_html())
                    .unwrap_or("".into());
                add = s != "Pronunciation"
                    && s != ""
                    && s != "Anagrams"
                    && s != "Conjugation"
                    && s != "Declension"
                    && s != "Derived terms"
                    && s != "Related terms";
                if add {
                    content += &format!("_{s}_\n");
                }
                continue;
            } else {
                if !add || &el.name.local == "div" || &el.name.local == "table" {
                    continue;
                }
                if let Some(e) = ElementRef::wrap(node) {
                    let s: String = e.text().filter(|e| *e != "edit").collect();
                    content += &s;
                    content += "\n";
                }
            }
        }
    }
    println!("sending: {:?}", content);
    state
        .api
        .spawn(SendMessage::new(m.chat, format!("*{q}*\n {content}")).parse_mode(Markdown));

    // this will be converted into a JSON response
    // with a status code of `201 Created`
    (StatusCode::OK, Json(""))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let token = std::env::var("BOT_TOKEN").expect("Wrong token string");
    let origin = std::env::var("ORIGIN").expect("Wrong origin string");
    let path = std::env::var("SECRET_PATH").expect("Wrong secret string");
    let port = std::env::var("PORT")
        .expect("Wrong port set")
        .parse()
        .expect("Port is not a number");
    let hook_url = format!("https://api.telegram.org/bot{token}/setWebhook?url={origin}{path}");
    let api = Api::new(token);
    let client = reqwest::Client::builder().gzip(true).build()?;
    println!(
        "Setting up hook: {:?}",
        client.get(hook_url).send().await?.text().await?
    );

    let app = Router::new()
        // `GET /` goes to `root`
        .route("/health", get(root))
        // `POST /users` goes to `get_update`
        .route(&path, post(get_update))
        .layer(Extension(Arc::new(AppState {
            api,
            fin_sel: make_selector("#Finnish"),
            client,
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
