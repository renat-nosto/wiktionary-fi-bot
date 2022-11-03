use crate::selectors;
use std::collections::HashSet;

pub(crate) struct AppState {
    pub(crate) token: String,
    pub(crate) selectors: selectors::Selectors,
    pub(crate) skip_chapters: HashSet<String>,
}

impl AppState {
    pub(crate) fn new(token: String) -> Self {
        AppState {
            token,
            selectors: selectors::Selectors::new(),
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
