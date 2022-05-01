use axum::{
    response::IntoResponse,
    routing::{get, post},
    Extension, Json, Router,
};
use ego_tree::NodeRef;
use reqwest::{Client, StatusCode};
use scraper::{ElementRef, Html, Node, Selector};
use std::{collections::HashSet, error::Error, fmt::Write, net::SocketAddr, sync::Arc};
use telegram_bot::{
    Api, MessageChat, MessageKind, ParseMode::Markdown, SendMessage, ToChatRef, Update, UpdateKind,
};

fn make_selector(selector: &str) -> Selector {
    Selector::parse(selector).expect("bad selector")
}

struct Selectors {
    finnish: Selector,
    nouns: Vec<Selector>,
    verbs: Vec<Selector>,
}

impl Selectors {
    fn new() -> Self {
        Selectors {
            finnish: make_selector("#Finnish"),
            nouns: ["par", "all"]
                .into_iter()
                .flat_map(|infl| {
                    ["s", "p"]
                        .into_iter()
                        .map(move |t| make_selector(&format!(".lang-fi.{infl}\\|{t}-form-of")))
                })
                .collect(),
            verbs: ["pres", "past"]
                .into_iter()
                .flat_map(move |tense| {
                    ["1", "3"].into_iter().map(move |per| {
                        make_selector(&format!(".lang-fi.\\3{per} \\|s\\|{tense}\\|indc-form-of"))
                    })
                })
                .collect(),
        }
    }
}

struct AppState {
    api: Api,
    selectors: Selectors,
    client: Client,
    skip_chapters: HashSet<String>,
}

impl AppState {
    fn new(token: &str) -> Self {
        AppState {
            api: Api::new(token),
            selectors: Selectors::new(),
            client: Client::builder()
                .gzip(true)
                .build()
                .expect("Failed to create a reqwest client"),
            skip_chapters: [
                "Pronunciation",
                "",
                "Anagrams",
                "Conjugation",
                "Declension",
                "References",
                "Derived terms",
                "Related terms",
            ]
                .into_iter()
                .map(String::from)
                .collect(),
        }
    }

    fn send_markdown<C: ToChatRef>(&self, chat: C, text: String) {
        self.api
            .spawn(SendMessage::new(chat, text).parse_mode(Markdown));
    }
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Bot is working..."
}

fn first_element_child(node: NodeRef<Node>) -> Option<ElementRef> {
    ElementRef::wrap(ElementRef::wrap(node)?.first_child()?)
}

async fn get_update(
    Json(update): Json<Update>,
    Extension(state): Extension<Arc<AppState>>,
) -> impl IntoResponse {
    println!("{:?}", update);
    let ret = (StatusCode::OK, Json(""));
    // insert your application logic here
    let message = match update.kind {
        UpdateKind::Message(m) => m,
        _ => {
            println!("Not a message");
            return ret;
        }
    };
    let text = match message.kind {
        MessageKind::Text { data, entities: _ } => data,
        _ => {
            println!("Not a text");
            return ret;
        }
    };
    let is_group = matches!(
        message.chat,
        MessageChat::Group(_) | MessageChat::Supergroup(_)
    );
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
            state.send_markdown(&message.chat, format!("{q} - error {err:?}"));
            println!("Err {:?}", err);
            return ret;
        }
    }
        .text()
        .await
        .expect("failed to read text");
    let html = Html::parse_document(&text);
    let mut el = html.select(&state.selectors.finnish);

    let par = match el.next().and_then(|o| o.parent()) {
        Some(x) => x,
        None => {
            state.send_markdown(&message.chat, format!("{q} not found in Finnish"));
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
                add = !state.skip_chapters.contains(&s);
                if add {
                    writeln!(content, "_{s}_");
                }
                continue;
            } else {
                if !add || &el.name.local == "div" || &el.name.local == "table" || &el.name.local == "style" {
                    continue;
                }
                if let Some(e) = ElementRef::wrap(node) {
                    e.text()
                        .filter(|e| *e != "edit")
                        .map(|s| s.replace('*', ""))
                        .for_each(|s| {
                            write!(content, "{s}");
                        });
                    writeln!(content);
                }
            }
        }
    }
    let [ns, vs] = [&state.selectors.nouns, &state.selectors.verbs].map(|sels| {
        sels.iter()
            .map(|sel| {
                html.select(sel)
                    .next()
                    .map(|e| e.text().collect::<String>())
            })
            .collect::<Vec<_>>()
    });
    if let [Some(par_s), Some(par_p), Some(ines_s), Some(ines_p)] = ns.as_slice() {
        let vartalo = ines_s.trim_end_matches("lle");
        let mon_vartalo = ines_p.trim_end_matches("lle");
        writeln!(content, "_Vartalot_\n{vartalo} - {mon_vartalo} p. {par_s} m.p. {par_p}");
    }
    if let [Some(pr1), Some(pr3), Some(pa1), Some(pa3)] = vs.as_slice() {
        let vartalo = pr1.trim_end_matches("n");
        let past_vartalo = pa1.trim_end_matches("n");
        write!(content, "_Vartalot_\n{vartalo} - {past_vartalo}");
        if let Some(c) = vartalo.chars().last() {
            if pr3 != &format!("{vartalo}{c}") {
                write!(content, " p3. {pr3}");
            }
        }
        if pa3 != past_vartalo {
            write!(content, " past3. {pa3}");
        }
        writeln!(content) ;
    }

    writeln!(content, "https://en.wiktionary.org/wiki/{q}#Finnish");
    println!("sending: {:?}", content);
    state.send_markdown(&message.chat, format!("*{q}*\n{content}"));
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

    let state = AppState::new(&token);
    println!(
        "Setting up hook: {:?}",
        state
            .client
            .get(format!(
                "https://api.telegram.org/bot{token}/setWebhook?url={origin}{path}"
            ))
            .send()
            .await?
            .text()
            .await?
    );

    let app = Router::new()
        // `GET /` goes to `root`
        .route("/health", get(root))
        // `POST /users` goes to `get_update`
        .route(&path, post(get_update))
        .layer(Extension(Arc::new(state)));

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}
