use scraper::Selector;

pub(crate) struct Selectors {
    pub(crate) finnish: Selector,
    pub(crate) nouns: Vec<Selector>,
    pub(crate) verbs: Vec<Selector>,
    pub(crate) search_result: Selector,
}

fn make_selector(selector: &str) -> Selector {
    Selector::parse(selector).expect("bad selector")
}

impl Selectors {
    pub(crate) fn new() -> Self {
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
