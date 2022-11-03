use std::collections::BTreeSet;
use std::fmt::Write;

use ego_tree::NodeRef;
use scraper::{Html, Node};
use telegram_bot_raw::ParseMode::Markdown;
use telegram_bot_raw::{
    InlineKeyboardButton, InlineKeyboardMarkup, MessageChat, MessageKind, Request, SendMessage,
    Update, UpdateKind,
};

use crate::domops;
use crate::{appstate, req};

struct MessageState<'a> {
    chat: MessageChat,
    q: String,
    state: &'a appstate::AppState,
    link: String,
    refs: BTreeSet<String>,
}

impl MessageState<'_> {
    async fn try_full_search(&mut self) -> bool {
        let link = format!(
            "https://en.wiktionary.org/wiki/Special:Search?search={}&fulltext=Full+text+search&ns0=1", self.q
        );
        let Some(fulltext) = self.load(&link).await else { return false };
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

    async fn get(&self, link: &str) -> String {
        req::get(link).await
    }

    async fn load(&self, link: &str) -> Option<Html> {
        let text = self.get(link).await;
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
        self.send(message);
    }

    fn send(&self, message: SendMessage) {
        let r = message.serialize().expect("bad message");
        let tok = &self.state.token;
        req::req(r, tok.clone());
    }

    fn send_article(&mut self, html: &Html, par: &NodeRef<Node>) {
        let mut content = domops::get_main_content(&mut self.refs, &self.state.skip_chapters, par);
        let [ns, vs] = domops::get_forms(&self.state.selectors, html);
        domops::write_noun_forms(&mut content, ns);
        domops::write_verb_forms(&mut content, vs);

        let _ = writeln!(content, "{}", &self.link);
        println!("sending: {:?}", content);
        let q = &self.q;
        let string = format!("*{q}*\n{content}");
        self.send_markdown(string);
    }

    async fn send_link(&mut self) -> State {
        let Some(html) = self.load(&self.link).await else { return State::Err };

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
    let MessageKind::Text { data, .. } = &message.kind else {
        println!("Not a text");
        return None;
    };
    let text = data.to_lowercase();
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

pub(crate) async fn get_update(update: Update, state: &appstate::AppState) {
    let Some((q, chat)) = get_query(&update) else { return };

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
}
