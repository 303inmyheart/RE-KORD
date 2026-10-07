//! A small, lenient markup tokenizer for feeds and HTML heads.
//!
//! Real-world feeds are often not well-formed (stray `&`, unclosed tags,
//! HTML inside descriptions), and a strict parser would reject a feed a
//! podcast app happily plays. This one never fails: it yields start / end
//! tags with attributes and unescaped text, and the callers pick what they
//! know. Names keep their prefix and are lower-cased (`itunes:duration`).

use std::borrow::Cow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token<'a> {
    Start {
        name: String,
        attrs: Vec<(String, String)>,
        self_closing: bool,
    },
    End {
        name: String,
    },
    Text(Cow<'a, str>),
}

pub struct Tokenizer<'a> {
    src: &'a str,
    pos: usize,
    /// Raw-text element being skipped (`script` / `style` in HTML).
    raw_until: Option<&'static str>,
}

impl<'a> Tokenizer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            src,
            pos: 0,
            raw_until: None,
        }
    }

    pub fn offset(&self) -> usize {
        self.pos
    }

    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }
}

impl<'a> Iterator for Tokenizer<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Token<'a>> {
        loop {
            if self.pos >= self.src.len() {
                return None;
            }
            if let Some(end_tag) = self.raw_until.take() {
                // Skip the body of <script>/<style> in one go.
                let rest = self.rest();
                let lower_idx = find_ci(rest, end_tag);
                self.pos += lower_idx.unwrap_or(rest.len());
                continue;
            }
            let rest = self.rest();
            if !rest.starts_with('<') {
                let end = rest.find('<').unwrap_or(rest.len());
                let raw = &rest[..end];
                self.pos += end;
                return Some(Token::Text(unescape(raw)));
            }
            if let Some(body) = rest.strip_prefix("<!--") {
                let end = body.find("-->").map(|i| i + 3).unwrap_or(body.len());
                self.pos += 4 + end;
                continue;
            }
            if let Some(body) = rest.strip_prefix("<![CDATA[") {
                let (text, adv) = match body.find("]]>") {
                    Some(i) => (&body[..i], i + 3),
                    None => (body, body.len()),
                };
                self.pos += 9 + adv;
                return Some(Token::Text(Cow::Borrowed(text)));
            }
            if rest.starts_with("<?") || rest.starts_with("<!") {
                let end = rest.find('>').map(|i| i + 1).unwrap_or(rest.len());
                self.pos += end;
                continue;
            }
            if let Some(body) = rest.strip_prefix("</") {
                let end = body.find('>').unwrap_or(body.len());
                let name = body[..end].trim().to_ascii_lowercase();
                self.pos += 2 + (end + 1).min(body.len());
                return Some(Token::End { name });
            }
            // Start tag: name, then attributes up to an unquoted `>`.
            let body = &rest[1..];
            let name_len = body
                .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
                .unwrap_or(body.len());
            if name_len == 0 {
                // A lone `<` in text ("a < b"): treat it as text.
                self.pos += 1;
                return Some(Token::Text(Cow::Borrowed("<")));
            }
            let name = body[..name_len].to_ascii_lowercase();
            let (attrs, consumed, self_closing) = parse_attrs(&body[name_len..]);
            self.pos += 1 + name_len + consumed;
            if (name == "script" || name == "style") && !self_closing {
                self.raw_until = Some(if name == "script" {
                    "</script"
                } else {
                    "</style"
                });
            }
            return Some(Token::Start {
                name,
                attrs,
                self_closing,
            });
        }
    }
}

fn find_ci(hay: &str, needle: &str) -> Option<usize> {
    let h = hay.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || h.len() < n.len() {
        return None;
    }
    (0..=h.len() - n.len()).find(|&i| h[i..i + n.len()].eq_ignore_ascii_case(n))
}

/// Attributes of a start tag; returns them, the bytes consumed (through `>`)
/// and whether the tag closed itself.
fn parse_attrs(s: &str) -> (Vec<(String, String)>, usize, bool) {
    let b = s.as_bytes();
    let mut i = 0;
    let mut attrs = Vec::new();
    let mut self_closing = false;
    while i < b.len() {
        let c = b[i];
        if c == b'>' {
            return (attrs, i + 1, self_closing);
        }
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c == b'/' {
            self_closing = true;
            i += 1;
            continue;
        }
        self_closing = false;
        let start = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'=' && b[i] != b'>' {
            if b[i] == b'/' && b.get(i + 1) == Some(&b'>') {
                break;
            }
            i += 1;
        }
        let key = s[start..i].to_ascii_lowercase();
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if i < b.len() && b[i] == b'=' {
            i += 1;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
                let q = b[i];
                i += 1;
                let vs = i;
                while i < b.len() && b[i] != q {
                    i += 1;
                }
                value = unescape(&s[vs..i]).into_owned();
                i = (i + 1).min(b.len());
            } else {
                let vs = i;
                while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'>' {
                    i += 1;
                }
                value = unescape(&s[vs..i]).into_owned();
            }
        }
        if !key.is_empty() {
            attrs.push((key, value));
        }
    }
    (attrs, b.len(), self_closing)
}

/// XML predefined entities, numeric references and the HTML entities feeds
/// actually use. Unknown entities stay as written.
pub fn unescape(raw: &str) -> Cow<'_, str> {
    if !raw.contains('&') {
        return Cow::Borrowed(raw);
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        let semi = after.find(';').filter(|&i| i <= 10);
        let Some(semi) = semi else {
            out.push('&');
            rest = after;
            continue;
        };
        let ent = &after[..semi];
        let ch: Option<char> = if let Some(num) = ent.strip_prefix('#') {
            let n = if let Some(hex) = num.strip_prefix(['x', 'X']) {
                u32::from_str_radix(hex, 16).ok()
            } else {
                num.parse::<u32>().ok()
            };
            n.and_then(char::from_u32).filter(|c| *c != '\0')
        } else {
            match ent {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                "hellip" => Some('…'),
                "ndash" => Some('–'),
                "mdash" => Some('—'),
                "rsquo" => Some('’'),
                "lsquo" => Some('‘'),
                "rdquo" => Some('”'),
                "ldquo" => Some('“'),
                "laquo" => Some('«'),
                "raquo" => Some('»'),
                "egrave" => Some('è'),
                "eacute" => Some('é'),
                "agrave" => Some('à'),
                "ograve" => Some('ò'),
                "ugrave" => Some('ù'),
                "igrave" => Some('ì'),
                "ouml" => Some('ö'),
                "uuml" => Some('ü'),
                "auml" => Some('ä'),
                "szlig" => Some('ß'),
                _ => None,
            }
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &after[semi + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// Bytes → text. UTF-8 (with or without BOM) unless the XML declaration or
/// HTML meta says Latin-1 / Windows-1252, which old feeds still use.
pub fn decode_text(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let head_len = bytes.len().min(1024);
    let head = String::from_utf8_lossy(&bytes[..head_len]).to_ascii_lowercase();
    let latin = (head.contains("iso-8859-1")
        || head.contains("windows-1252")
        || head.contains("latin1")
        || head.contains("iso-8859-15"))
        && std::str::from_utf8(bytes).is_err();
    if latin {
        return bytes.iter().map(|&b| b as char).collect();
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Collapse whitespace and drop tags from a text that may carry HTML.
pub fn clean_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    if raw.contains('<') {
        for tok in Tokenizer::new(raw) {
            if let Token::Text(t) = tok {
                out.push_str(&t);
                out.push(' ');
            }
        }
    } else {
        out.push_str(raw);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_tags_attrs_cdata_and_entities() {
        let src = r#"<?xml version="1.0"?><!-- c --><rss a='1'><title>A &amp; B &#233;</title><enclosure url="x?a=1&amp;b=2" length=12 /><d><![CDATA[<p>hi</p>]]></d></rss>"#;
        let toks: Vec<_> = Tokenizer::new(src).collect();
        assert!(
            matches!(&toks[0], Token::Start { name, attrs, .. } if name == "rss" && attrs[0].1 == "1")
        );
        assert_eq!(toks[2], Token::Text(Cow::Owned("A & B é".into())));
        match &toks[4] {
            Token::Start {
                name,
                attrs,
                self_closing,
            } => {
                assert_eq!(name, "enclosure");
                assert_eq!(attrs[0], ("url".into(), "x?a=1&b=2".into()));
                assert_eq!(attrs[1], ("length".into(), "12".into()));
                assert!(*self_closing);
            }
            other => panic!("{other:?}"),
        }
        assert!(toks.contains(&Token::Text(Cow::Borrowed("<p>hi</p>"))));
    }

    #[test]
    fn survives_broken_markup() {
        let src = "<a>fish & chips < 3 <b>ok</b";
        let texts: Vec<String> = Tokenizer::new(src)
            .filter_map(|t| match t {
                Token::Text(s) => Some(s.into_owned()),
                _ => None,
            })
            .collect();
        assert_eq!(texts.join(""), "fish & chips < 3 ok");
    }

    #[test]
    fn skips_script_bodies() {
        let src = "<head><script>if (a<b) { x = '</div>'; }</script><link rel=alternate></head>";
        let names: Vec<String> = Tokenizer::new(src)
            .filter_map(|t| match t {
                Token::Start { name, .. } => Some(name),
                _ => None,
            })
            .collect();
        assert_eq!(names, vec!["head", "script", "link"]);
    }

    #[test]
    fn decodes_latin1_feeds() {
        let bytes = b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><t>Citt\xe0</t>";
        assert!(decode_text(bytes).contains("Città"));
        assert_eq!(decode_text("\u{feff}ok".as_bytes()), "ok");
    }

    #[test]
    fn cleans_html_text() {
        assert_eq!(
            clean_text("<p>Hello <b>world</b></p>\n\n  again"),
            "Hello world again"
        );
    }
}
