use axum::{
    response::IntoResponse,
    routing::{get, post},
    Extension, Json, Router,
};
use axum_macros::debug_handler;
use ego_tree::NodeRef;
use reqwest::{Client, StatusCode};
use scraper::{ElementRef, Html, Node, Selector};
use std::collections::BTreeSet;
use std::{collections::HashSet, error::Error, fmt::Write, net::SocketAddr, sync::Arc};
use telegram_bot::{
    Api, InlineKeyboardButton, InlineKeyboardMarkup, MessageChat, MessageKind, ParseMode::Markdown,
    SendMessage, Update, UpdateKind,
};

fn make_selector(selector: &str) -> Selector {
    Selector::parse(selector).expect("bad selector")
}

struct Selectors {
    finnish: Selector,
    nouns: Vec<Selector>,
    verbs: Vec<Selector>,
    search_result: Selector,
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
            search_result: make_selector(".mw-search-result-heading a"),
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
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Bot is working..."
}

fn first_element_child(node: NodeRef<Node>) -> Option<ElementRef> {
    ElementRef::wrap(ElementRef::wrap(node)?.first_child()?)
}

fn header_text(node: NodeRef<Node>) -> String {
    let s: String = first_element_child(node)
        .map(|e| e.inner_html())
        .unwrap_or_else(|| "".into());
    let s_clean = s.trim_end_matches("edit");
    s_clean.to_string()
}

fn push_no_double_whitespace(buf: &mut String, c: char) {
    if let Some(last) = buf.chars().last() {
        if last.is_whitespace() && c.is_whitespace() {
            return;
        }
    }
    buf.push(c);
}

fn write_content(content: &mut String, e: ElementRef, refs: &mut BTreeSet<String>) {
    e.children().for_each(|c| {
        match c.value() {
            Node::Text(t) => {
                t.chars()
                    .filter(|c| *c != '*')
                    .map(|c| if c.is_whitespace() { ' ' } else { c })
                    .for_each(|c| push_no_double_whitespace(content, c));
            }

            Node::Element(e) => {
                let tag: &str = &e.name.local;
                let er = ElementRef::wrap(c).unwrap();
                match tag {
                    "table" | "sup" | "style" => {
                        //skip
                    }
                    "a" => {
                        if let Some(t) = e.attr("title") {
                            refs.insert(t.to_string());
                        }
                        write_content(content, er, refs);
                    }
                    "i" => {
                        content.push('_');
                        write_content(content, er, refs);
                        content.push('_');
                    }
                    _ => {
                        write_content(content, er, refs);
                    }
                }
            }
            _ => {}
        }
    });
}

struct MessageState<'a> {
    chat: MessageChat,
    q: String,
    state: &'a AppState,
    link: String,
    refs: BTreeSet<String>,
}

impl MessageState<'_> {
    async fn try_full_search(&mut self) -> bool {
        let link = format!(
            "https://en.wiktionary.org/wiki/Special:Search?search={}&fulltext=Full+text+search&ns0=1", self.q
        );
        let fulltext = match self.load(&link).await {
            Some(x) => x,
            _ => return false,
        };
        let res = fulltext
            .select(&self.state.selectors.search_result)
            .collect::<Vec<_>>();
        if let Some(first_el) = res.first() {
            if let Some(found_link) = first_el.value().attr("href") {
                self.link = format!("https://en.wiktionary.org{found_link}");
                return true;
            }
        }
        false
    }

    async fn load(&self, link: &str) -> Option<Html> {
        let text = match self.state.client.get(link).send().await {
            Ok(val) => val,
            Err(err) => {
                self.send_markdown(format!("{link} - error {err:?}"));
                println!("Err {:?}", err);
                return None;
            }
        }
        .text()
        .await
        .expect("failed to read text");
        let html = Html::parse_document(&text);
        Some(html)
    }

    fn send_markdown(&self, text: String) {
        let mut message = SendMessage::new(&self.chat, text);
        message.parse_mode(Markdown);
        if !self.refs.is_empty() {
            let items = self
                .refs
                .iter()
                .map(|s| InlineKeyboardButton::callback(s, s))
                .collect::<Vec<_>>();
            let vec = items.chunks(4).map(|c| c.to_vec()).collect::<Vec<_>>();
            message.reply_markup(InlineKeyboardMarkup::from(vec));
        }
        self.state.api.spawn(message);
    }

    fn send_article(&mut self, html: &Html, par: &NodeRef<Node>) {
        let mut add = false;
        let mut content = String::new();

        for node in par.next_siblings() {
            if let Some(el) = node.value().as_element() {
                if "h2" == &el.name.local {
                    break;
                }
                if &el.name.local == "h3" || &el.name.local == "h4" || &el.name.local == "h5" {
                    let s_clean = header_text(node);
                    add = !self.state.skip_chapters.contains(&s_clean);
                    if add {
                        content.push_str("\n_");
                        content.push_str(&s_clean);
                        content.push_str("_\n");
                    }
                    continue;
                } else {
                    if !add
                        || &el.name.local == "div"
                        || &el.name.local == "table"
                        || &el.name.local == "style"
                    {
                        continue;
                    }
                    if let Some(e) = ElementRef::wrap(node) {
                        write_content(&mut content, e, &mut self.refs);
                        content.push('\n')
                    }
                }
            }
        }
        let [ns, vs] = [&self.state.selectors.nouns, &self.state.selectors.verbs].map(|sels| {
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
            let _ = writeln!(
                content,
                "_Vartalot_\n{vartalo} - {mon_vartalo} p. {par_s} m.p. {par_p}"
            );
        }
        if let [Some(pr1), Some(pr3), Some(pa1), Some(pa3)] = vs.as_slice() {
            let vartalo = pr1.trim_end_matches('n');
            let past_vartalo = pa1.trim_end_matches('n');
            let _ = write!(content, "_Vartalot_\n{vartalo} - {past_vartalo}");
            if let Some(c) = vartalo.chars().last() {
                if pr3 != &format!("{vartalo}{c}") {
                    let _ = write!(content, " p3. {pr3}");
                }
            }
            if pa3 != past_vartalo {
                let _ = write!(content, " past3. {pa3}");
            }
            let _ = writeln!(content);
        }

        let _ = writeln!(content, "{}", &self.link);
        println!("sending: {:?}", content);
        let q = &self.q;
        self.send_markdown(format!("*{q}*\n{content}"));
    }

    async fn send_link(&mut self) -> State {
        let html = match self.load(&self.link).await {
            Some(html) => html,
            None => return State::Err,
        };

        match html
            .select(&self.state.selectors.finnish)
            .next()
            .and_then(|o| o.parent())
        {
            Some(x) => {
                self.send_article(&html, &x);
                State::Sent
            }
            None => State::Missing,
        }
    }
}

enum State {
    Sent,
    Err,
    Missing,
}

fn get_query(update: &Update) -> Option<(String, MessageChat)> {
    let message = match &update.kind {
        UpdateKind::Message(m) => m,
        UpdateKind::EditedMessage(m) => m,
        UpdateKind::CallbackQuery(q) => {
            return q
                .data
                .clone()
                .map(|s| (s, MessageChat::Private(q.from.clone())));
        }
        _ => {
            println!("Not a message");
            return None;
        }
    };
    let text = match &message.kind {
        MessageKind::Text { data, entities: _ } => data.to_lowercase(),
        _ => {
            println!("Not a text");
            return None;
        }
    };
    let is_group = matches!(
        message.chat,
        MessageChat::Group(_) | MessageChat::Supergroup(_)
    );
    let q = if is_group {
        if let Some(text) = text.strip_prefix("/w ").or_else(|| text.strip_prefix('/')) {
            text
        } else {
            println!("Bad Query: {:?}", text);
            return None;
        }
    } else {
        text.trim_start_matches('/')
    };
    Some((q.to_string(), message.chat.clone()))
}

#[debug_handler]
async fn get_update(
    Json(update): Json<Update>,
    Extension(state): Extension<Arc<AppState>>,
) -> impl IntoResponse {
    println!("{:?}", update);
    let ret = (StatusCode::OK, Json(""));
    let (q, chat) = match get_query(&update) {
        Some(q) => q,
        None => return ret,
    };

    println!("Query: {:?}", q);
    let link = format!("https://en.wiktionary.org/wiki/{q}");
    let mut message_state = MessageState {
        chat,
        q,
        state: &*state,
        link,
        refs: BTreeSet::new(),
    };

    match message_state.send_link().await {
        State::Sent => {}
        State::Err => {}
        State::Missing => {
            if message_state.try_full_search().await {
                message_state.send_link().await;
            } else {
                message_state.send_markdown(format!("*{}*\nNo article found", message_state.q));
            }
        }
    }
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
        .route("/health", get(root))
        .route(&path, post(get_update))
        .layer(Extension(Arc::new(state)));

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}

#[test]
fn test1() {
    let html = Html::parse_document(
        r#"<!doctype html><meta charset=utf-8><title>shortest html5</title><body><p><i class="Latn mention" lang="fi"><a href="/wiki/mainos#Finnish" title="mainos">mainos</a></i>
         + <i class="Latn mention" lang="fi"><a href="/wiki/-taa#Finnish" title="-taa">-taa</a></i></p>"#,
    );
    let s = make_selector("p");
    let x = html.select(&s).next().unwrap();
    let mut s = String::new();
    write_content(&mut s, x, &mut BTreeSet::new());
    assert_eq!(s, "/mainos + /-taa");
}
