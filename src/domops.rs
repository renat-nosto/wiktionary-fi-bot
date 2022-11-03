use std::collections::{BTreeSet, HashSet};
use std::fmt::Write;

use ego_tree::NodeRef;
use scraper::{ElementRef, Html, Node};

use crate::selectors::Selectors;

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

pub(crate) fn get_forms(selectors: &Selectors, html: &Html) -> [Vec<Option<String>>; 2] {
    [&selectors.nouns, &selectors.verbs].map(|sels| {
        sels.iter()
            .map(|sel| {
                html.select(sel)
                    .next()
                    .map(|e| e.text().collect::<String>())
            })
            .collect::<Vec<_>>()
    })
}

pub(crate) fn write_noun_forms(content: &mut String, ns: Vec<Option<String>>) {
    if let [Some(par_s), Some(par_p), Some(ines_s), Some(ines_p)] = ns.as_slice() {
        let vartalo = ines_s.trim_end_matches("lle");
        let mon_vartalo = ines_p.trim_end_matches("lle");
        let _ = writeln!(
            content,
            "_Vartalot_\n{vartalo} - {mon_vartalo} p. {par_s} m.p. {par_p}"
        );
    }
}

pub(crate) fn write_verb_forms(content: &mut String, vs: Vec<Option<String>>) {
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
                            if !t.starts_with("w:") && !t.starts_with("Reconstruction:") {
                                refs.insert(t.to_string());
                            }
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

pub(crate) fn get_main_content(
    refs: &mut BTreeSet<String>,
    skip_chapters: &HashSet<String>,
    par: &NodeRef<Node>,
) -> String {
    let mut add = false;
    let mut content = String::new();

    for node in par.next_siblings() {
        let Some(el) = node.value().as_element() else { continue };
        if "h2" == &el.name.local {
            break;
        }
        if &el.name.local == "h3" || &el.name.local == "h4" || &el.name.local == "h5" {
            let s_clean = header_text(node);
            add = !skip_chapters.contains(&s_clean);
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
                write_content(&mut content, e, refs);
                content.push('\n')
            }
        }
    }
    content
}
