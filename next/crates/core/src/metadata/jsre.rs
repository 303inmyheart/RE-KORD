//! A small backtracking regex engine with JavaScript semantics, used to port
//! the legacy title / wikitext cleaning rules exactly (the `regex` crate has
//! no lookahead and is not a dependency of this crate).
//!
//! Supported: literals, `.`, classes (`[a-z]`, `[^…]`, `\d \w \s` and their
//! negations, also inside classes), `^ $` (multiline with `m`), `\b \B`,
//! groups `( )` / `(?: )`, lookahead `(?= )` / `(?! )`, alternation,
//! quantifiers `? * + {n} {n,} {n,m}` (greedy and lazy). Flags: `i`
//! (case-insensitive), `m` (multiline anchors), `s` (dot matches newline).
//! Like JavaScript without the `u` flag, `\w` and `\b` are ASCII-only.
//!
//! Patterns are trusted constants compiled once; [`JsRegex::new`] panics on a
//! malformed pattern, which the unit tests catch.

#[derive(Debug, Clone)]
enum ClassItem {
    Range(char, char),
    Digit(bool),
    Word(bool),
    Space(bool),
}

#[derive(Debug, Clone)]
struct ClassSet {
    negated: bool,
    items: Vec<ClassItem>,
}

#[derive(Debug, Clone)]
enum Node {
    Empty,
    Char(char),
    Any,
    Class(Box<ClassSet>),
    Start,
    End,
    WordBoundary(bool),
    Group(Box<Node>, Option<usize>),
    Look(Box<Node>, bool),
    Concat(Vec<Node>),
    Alt(Vec<Node>),
    Repeat {
        node: Box<Node>,
        min: usize,
        max: usize,
        greedy: bool,
    },
}

type Caps = Vec<Option<(usize, usize)>>;

/// A compiled pattern.
#[derive(Debug, Clone)]
pub struct JsRegex {
    root: Node,
    groups: usize,
    icase: bool,
    multiline: bool,
    dotall: bool,
}

/// One match, in char offsets of the searched text.
#[derive(Debug, Clone)]
pub struct JsMatch {
    chars: Vec<char>,
    pub start: usize,
    pub end: usize,
    caps: Caps,
}

impl JsMatch {
    /// Whole match (`$&`).
    pub fn text(&self) -> String {
        self.chars[self.start..self.end].iter().collect()
    }

    /// Capture group `i` (1-based); `None` when it did not participate.
    pub fn group(&self, i: usize) -> Option<String> {
        if i == 0 {
            return Some(self.text());
        }
        let (a, b) = (*self.caps.get(i - 1)?)?;
        Some(self.chars[a..b].iter().collect())
    }
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// JavaScript `\s` (ASCII whitespace plus the Unicode space separators).
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

fn lower(c: char) -> char {
    let mut l = c.to_lowercase();
    match (l.next(), l.next()) {
        (Some(x), None) => x,
        _ => c,
    }
}

fn upper(c: char) -> char {
    let mut u = c.to_uppercase();
    match (u.next(), u.next()) {
        (Some(x), None) => x,
        _ => c,
    }
}

impl ClassSet {
    fn contains_raw(&self, c: char) -> bool {
        self.items.iter().any(|it| match *it {
            ClassItem::Range(a, b) => a <= c && c <= b,
            ClassItem::Digit(neg) => c.is_ascii_digit() != neg,
            ClassItem::Word(neg) => is_word(c) != neg,
            ClassItem::Space(neg) => is_js_space(c) != neg,
        })
    }

    fn matches(&self, c: char, icase: bool) -> bool {
        let hit = self.contains_raw(c)
            || (icase && (self.contains_raw(lower(c)) || self.contains_raw(upper(c))));
        hit != self.negated
    }
}

struct Parser<'a> {
    p: &'a [char],
    i: usize,
    groups: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.p.get(self.i).copied()
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn parse_alt(&mut self) -> Node {
        let mut alts = vec![self.parse_concat()];
        while self.eat('|') {
            alts.push(self.parse_concat());
        }
        if alts.len() == 1 {
            alts.pop().unwrap()
        } else {
            Node::Alt(alts)
        }
    }

    fn parse_concat(&mut self) -> Node {
        let mut items = Vec::new();
        while let Some(c) = self.peek() {
            if c == '|' || c == ')' {
                break;
            }
            let atom = self.parse_atom();
            let atom = self.parse_quant(atom);
            items.push(atom);
        }
        match items.len() {
            0 => Node::Empty,
            1 => items.pop().unwrap(),
            _ => Node::Concat(items),
        }
    }

    fn parse_number(&mut self) -> Option<usize> {
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.i += 1;
        }
        if start == self.i {
            return None;
        }
        self.p[start..self.i]
            .iter()
            .collect::<String>()
            .parse()
            .ok()
    }

    fn parse_quant(&mut self, atom: Node) -> Node {
        let (min, max) = match self.peek() {
            Some('?') => {
                self.i += 1;
                (0, 1)
            }
            Some('*') => {
                self.i += 1;
                (0, usize::MAX)
            }
            Some('+') => {
                self.i += 1;
                (1, usize::MAX)
            }
            Some('{') => {
                let save = self.i;
                self.i += 1;
                let Some(n) = self.parse_number() else {
                    self.i = save;
                    return atom;
                };
                let range = if self.eat(',') {
                    match self.parse_number() {
                        Some(m) => (n, m),
                        None => (n, usize::MAX),
                    }
                } else {
                    (n, n)
                };
                if !self.eat('}') {
                    self.i = save;
                    return atom;
                }
                range
            }
            _ => return atom,
        };
        let greedy = !self.eat('?');
        Node::Repeat {
            node: Box::new(atom),
            min,
            max,
            greedy,
        }
    }

    fn parse_escape_in_class(&mut self) -> ClassItem {
        let c = self.p[self.i];
        self.i += 1;
        match c {
            'd' => ClassItem::Digit(false),
            'D' => ClassItem::Digit(true),
            'w' => ClassItem::Word(false),
            'W' => ClassItem::Word(true),
            's' => ClassItem::Space(false),
            'S' => ClassItem::Space(true),
            'n' => ClassItem::Range('\n', '\n'),
            't' => ClassItem::Range('\t', '\t'),
            'r' => ClassItem::Range('\r', '\r'),
            other => ClassItem::Range(other, other),
        }
    }

    fn parse_class(&mut self) -> Node {
        // after '['
        let negated = self.eat('^');
        let mut items = Vec::new();
        let mut first = true;
        loop {
            let Some(c) = self.peek() else {
                panic!("unterminated class");
            };
            if c == ']' && !first {
                self.i += 1;
                break;
            }
            first = false;
            self.i += 1;
            let item = if c == '\\' {
                self.parse_escape_in_class()
            } else {
                ClassItem::Range(c, c)
            };
            // Range `a-b` (a literal `-` at the edges stays a literal).
            if let ClassItem::Range(a, _) = item {
                if self.peek() == Some('-') && self.p.get(self.i + 1).is_some_and(|n| *n != ']') {
                    self.i += 1;
                    let hi = self.p[self.i];
                    self.i += 1;
                    let hi = if hi == '\\' {
                        match self.parse_escape_in_class() {
                            ClassItem::Range(h, _) => h,
                            _ => panic!("bad class range"),
                        }
                    } else {
                        hi
                    };
                    items.push(ClassItem::Range(a, hi));
                    continue;
                }
            }
            items.push(item);
        }
        Node::Class(Box::new(ClassSet { negated, items }))
    }

    fn parse_atom(&mut self) -> Node {
        let c = self.p[self.i];
        self.i += 1;
        match c {
            '.' => Node::Any,
            '^' => Node::Start,
            '$' => Node::End,
            '[' => self.parse_class(),
            '(' => {
                let (cap, look) = if self.eat('?') {
                    match self.peek() {
                        Some(':') => {
                            self.i += 1;
                            (None, None)
                        }
                        Some('=') => {
                            self.i += 1;
                            (None, Some(false))
                        }
                        Some('!') => {
                            self.i += 1;
                            (None, Some(true))
                        }
                        _ => panic!("unsupported group"),
                    }
                } else {
                    self.groups += 1;
                    (Some(self.groups - 1), None)
                };
                let inner = self.parse_alt();
                assert!(self.eat(')'), "unbalanced group");
                match look {
                    Some(neg) => Node::Look(Box::new(inner), neg),
                    None => Node::Group(Box::new(inner), cap),
                }
            }
            '\\' => {
                let e = self.p[self.i];
                self.i += 1;
                let class = |item| {
                    Node::Class(Box::new(ClassSet {
                        negated: false,
                        items: vec![item],
                    }))
                };
                match e {
                    'b' => Node::WordBoundary(true),
                    'B' => Node::WordBoundary(false),
                    'd' => class(ClassItem::Digit(false)),
                    'D' => class(ClassItem::Digit(true)),
                    'w' => class(ClassItem::Word(false)),
                    'W' => class(ClassItem::Word(true)),
                    's' => class(ClassItem::Space(false)),
                    'S' => class(ClassItem::Space(true)),
                    'n' => Node::Char('\n'),
                    't' => Node::Char('\t'),
                    'r' => Node::Char('\r'),
                    other => Node::Char(other),
                }
            }
            other => Node::Char(other),
        }
    }
}

/// Single-character matcher (lets repeats of it run without recursion).
fn single_char(node: &Node) -> bool {
    matches!(node, Node::Char(_) | Node::Any | Node::Class(_))
}

impl JsRegex {
    /// Compile `pattern` with JS flags (`"i"`, `"gim"`…; `g` is ignored, use
    /// [`Self::replace`] / [`Self::replace_all`] instead).
    pub fn new(pattern: &str, flags: &str) -> Self {
        let chars: Vec<char> = pattern.chars().collect();
        let mut p = Parser {
            p: &chars,
            i: 0,
            groups: 0,
        };
        let root = p.parse_alt();
        assert!(p.i == chars.len(), "trailing pattern input in {pattern}");
        Self {
            root,
            groups: p.groups,
            icase: flags.contains('i'),
            multiline: flags.contains('m'),
            dotall: flags.contains('s'),
        }
    }

    fn char_eq(&self, a: char, b: char) -> bool {
        a == b || (self.icase && (lower(a) == lower(b) || upper(a) == upper(b)))
    }

    fn single(&self, node: &Node, c: char) -> bool {
        match node {
            Node::Char(x) => self.char_eq(*x, c),
            Node::Any => self.dotall || !matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}'),
            Node::Class(set) => set.matches(c, self.icase),
            _ => false,
        }
    }

    fn m(
        &self,
        node: &Node,
        s: &[char],
        i: usize,
        caps: &mut Caps,
        k: &mut dyn FnMut(usize, &mut Caps) -> bool,
    ) -> bool {
        match node {
            Node::Empty => k(i, caps),
            Node::Char(_) | Node::Any | Node::Class(_) => {
                i < s.len() && self.single(node, s[i]) && k(i + 1, caps)
            }
            Node::Start => {
                (i == 0 || (self.multiline && matches!(s[i - 1], '\n' | '\r'))) && k(i, caps)
            }
            Node::End => {
                (i == s.len() || (self.multiline && matches!(s[i], '\n' | '\r'))) && k(i, caps)
            }
            Node::WordBoundary(want) => {
                let before = i > 0 && is_word(s[i - 1]);
                let after = i < s.len() && is_word(s[i]);
                ((before != after) == *want) && k(i, caps)
            }
            Node::Group(inner, cap) => match cap {
                None => self.m(inner, s, i, caps, k),
                Some(idx) => {
                    let idx = *idx;
                    let start = i;
                    self.m(inner, s, i, caps, &mut |j, caps: &mut Caps| {
                        let prev = caps[idx];
                        caps[idx] = Some((start, j));
                        if k(j, caps) {
                            true
                        } else {
                            caps[idx] = prev;
                            false
                        }
                    })
                }
            },
            Node::Look(inner, negate) => {
                let mut probe = caps.clone();
                let found = self.m(inner, s, i, &mut probe, &mut |_, _| true);
                if found != *negate {
                    if found {
                        *caps = probe;
                    }
                    k(i, caps)
                } else {
                    false
                }
            }
            Node::Concat(items) => self.seq(items, s, i, caps, k),
            Node::Alt(alts) => {
                for a in alts {
                    if self.m(a, s, i, caps, k) {
                        return true;
                    }
                }
                false
            }
            Node::Repeat {
                node,
                min,
                max,
                greedy,
            } => {
                if single_char(node) {
                    let mut run = 0usize;
                    while run < *max && i + run < s.len() && self.single(node, s[i + run]) {
                        run += 1;
                    }
                    if run < *min {
                        return false;
                    }
                    if *greedy {
                        for n in (*min..=run).rev() {
                            if k(i + n, caps) {
                                return true;
                            }
                        }
                    } else {
                        for n in *min..=run {
                            if k(i + n, caps) {
                                return true;
                            }
                        }
                    }
                    false
                } else {
                    self.rep(node, *min, *max, *greedy, 0, s, i, caps, k)
                }
            }
        }
    }

    fn seq(
        &self,
        items: &[Node],
        s: &[char],
        i: usize,
        caps: &mut Caps,
        k: &mut dyn FnMut(usize, &mut Caps) -> bool,
    ) -> bool {
        match items.split_first() {
            None => k(i, caps),
            Some((first, rest)) => self.m(first, s, i, caps, &mut |j, caps: &mut Caps| {
                self.seq(rest, s, j, caps, k)
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn rep(
        &self,
        node: &Node,
        min: usize,
        max: usize,
        greedy: bool,
        count: usize,
        s: &[char],
        i: usize,
        caps: &mut Caps,
        k: &mut dyn FnMut(usize, &mut Caps) -> bool,
    ) -> bool {
        let more = |caps: &mut Caps, k: &mut dyn FnMut(usize, &mut Caps) -> bool| {
            count < max
                && self.m(node, s, i, caps, &mut |j, caps: &mut Caps| {
                    // A zero-width iteration past the minimum would loop forever.
                    if j == i && count >= min {
                        return false;
                    }
                    self.rep(node, min, max, greedy, count + 1, s, j, caps, k)
                })
        };
        if greedy {
            if more(caps, k) {
                return true;
            }
            count >= min && k(i, caps)
        } else {
            if count >= min && k(i, caps) {
                return true;
            }
            more(caps, k)
        }
    }

    fn find_in(&self, chars: &[char], from: usize) -> Option<(usize, usize, Caps)> {
        for start in from..=chars.len() {
            let mut caps: Caps = vec![None; self.groups];
            let mut end = None;
            let mut found_caps = None;
            if self.m(
                &self.root,
                chars,
                start,
                &mut caps,
                &mut |j, caps: &mut Caps| {
                    end = Some(j);
                    found_caps = Some(caps.clone());
                    true
                },
            ) {
                return Some((start, end?, found_caps?));
            }
        }
        None
    }

    /// `re.test(s)`.
    pub fn is_match(&self, s: &str) -> bool {
        let chars: Vec<char> = s.chars().collect();
        self.find_in(&chars, 0).is_some()
    }

    /// `s.match(re)` (first match).
    pub fn find(&self, s: &str) -> Option<JsMatch> {
        let chars: Vec<char> = s.chars().collect();
        let (start, end, caps) = self.find_in(&chars, 0)?;
        Some(JsMatch {
            chars,
            start,
            end,
            caps,
        })
    }

    /// Every non-overlapping match, left to right (like a `g` regex).
    pub fn find_all(&self, s: &str) -> Vec<JsMatch> {
        let chars: Vec<char> = s.chars().collect();
        let mut out = Vec::new();
        let mut from = 0;
        while from <= chars.len() {
            let Some((start, end, caps)) = self.find_in(&chars, from) else {
                break;
            };
            from = if end == start { end + 1 } else { end };
            out.push(JsMatch {
                chars: chars.clone(),
                start,
                end,
                caps,
            });
        }
        out
    }

    fn replace_impl(&self, s: &str, global: bool, f: &mut dyn FnMut(&JsMatch) -> String) -> String {
        let chars: Vec<char> = s.chars().collect();
        let mut out = String::with_capacity(s.len());
        let mut last = 0usize;
        let mut from = 0usize;
        while from <= chars.len() {
            let Some((start, end, caps)) = self.find_in(&chars, from) else {
                break;
            };
            out.extend(&chars[last..start]);
            let m = JsMatch {
                chars: chars.clone(),
                start,
                end,
                caps,
            };
            out.push_str(&f(&m));
            last = end;
            if !global {
                break;
            }
            if end == start {
                if end < chars.len() {
                    out.push(chars[end]);
                }
                last = end + 1;
                from = end + 1;
            } else {
                from = end;
            }
        }
        if last < chars.len() {
            out.extend(&chars[last..]);
        }
        out
    }

    /// `s.replace(re, f)` for a non-global regex: first match only.
    pub fn replace_with(&self, s: &str, mut f: impl FnMut(&JsMatch) -> String) -> String {
        self.replace_impl(s, false, &mut f)
    }

    /// `s.replace(re, f)` for a global regex.
    pub fn replace_all_with(&self, s: &str, mut f: impl FnMut(&JsMatch) -> String) -> String {
        self.replace_impl(s, true, &mut f)
    }

    /// First match replaced by `rep` (`$1`…`$9` and `$&` expanded).
    pub fn replace(&self, s: &str, rep: &str) -> String {
        self.replace_with(s, |m| expand(rep, m))
    }

    /// Every match replaced by `rep` (`$1`…`$9` and `$&` expanded).
    pub fn replace_all(&self, s: &str, rep: &str) -> String {
        self.replace_all_with(s, |m| expand(rep, m))
    }

    /// `s.split(re)[0]`.
    pub fn split_first<'a>(&self, s: &'a str) -> &'a str {
        let chars: Vec<char> = s.chars().collect();
        match self.find_in(&chars, 0) {
            // JS split ignores an empty match at index 0.
            Some((start, end, _)) if !(start == 0 && end == 0) => {
                let byte = s
                    .char_indices()
                    .nth(start)
                    .map(|(b, _)| b)
                    .unwrap_or(s.len());
                &s[..byte]
            }
            _ => s,
        }
    }
}

fn expand(rep: &str, m: &JsMatch) -> String {
    let mut out = String::new();
    let mut it = rep.chars().peekable();
    while let Some(c) = it.next() {
        if c == '$' {
            match it.peek().copied() {
                Some('&') => {
                    it.next();
                    out.push_str(&m.text());
                    continue;
                }
                Some(d) if d.is_ascii_digit() && d != '0' => {
                    it.next();
                    let idx = d.to_digit(10).unwrap_or(0) as usize;
                    out.push_str(&m.group(idx).unwrap_or_default());
                    continue;
                }
                Some('$') => {
                    it.next();
                    out.push('$');
                    continue;
                }
                _ => {}
            }
        }
        out.push(c);
    }
    out
}

/// Compile-once helper: `static RE: LazyLock<JsRegex> = js_re!("a+", "i");`.
#[macro_export]
#[doc(hidden)]
macro_rules! js_re {
    ($pat:expr, $flags:expr) => {
        std::sync::LazyLock::new(|| $crate::metadata::jsre::JsRegex::new($pat, $flags))
    };
}

#[cfg(test)]
mod tests {
    use super::JsRegex;

    #[test]
    fn basics() {
        let re = JsRegex::new(r"^\d+\s*[-–—]\s*", "i");
        assert_eq!(re.replace("01 - Song", ""), "Song");
        assert_eq!(re.replace("Song", ""), "Song");
        let re = JsRegex::new(r"\bremaster(ed|ing)?\b", "i");
        assert!(re.is_match("2011 Remastered"));
        assert!(!re.is_match("remasters"));
        let re = JsRegex::new(r"\b(?:video|audio|clip)\b(?!\s*feat)", "i");
        assert!(re.is_match("official video"));
        assert!(!re.is_match("video feat. X"));
        let re = JsRegex::new(r"\{\{[^{}]*\}\}", "g");
        assert_eq!(re.replace_all("a{{x}}b{{y|z}}c", ""), "abc");
        let re = JsRegex::new(r"<ref[^>]*>[\s\S]*?<\/ref>", "gi");
        assert_eq!(
            re.replace_all("a<ref name=x>1</ref>b<REF>2</ref>c", ""),
            "abc"
        );
        let re = JsRegex::new(r"\[\[[^\]|]*\|([^\]]*)\]\]", "g");
        assert_eq!(re.replace_all("x [[A|B]] y", "$1"), "x B y");
        let re = JsRegex::new(r"^=+.*=+\s*$", "gm");
        assert_eq!(re.replace_all("a\n== H ==\nb", ""), "a\n\nb");
        let re = JsRegex::new(r"[À-ÿ]", "i");
        assert!(re.is_match("é"));
        let re = JsRegex::new(r"a{2,3}", "");
        assert_eq!(re.replace_all("aaaa", "X"), "Xa");
        let re = JsRegex::new(r"\s*[|｜]\s*", "");
        assert_eq!(re.split_first("A | B"), "A");
        let re = JsRegex::new(r"(?:^|[^\d])\bhd\b|^\s*hd\s*$", "i");
        assert!(re.is_match("HD"));
        assert!(re.is_match("full hd"));
        let re = JsRegex::new(r"x*", "g");
        assert_eq!(re.replace_all("ab", "-"), "-a-b-");
    }

    #[test]
    fn lazy_and_groups() {
        let re = JsRegex::new(r"\s*[\[【]([\s\S]*?)[\]】]", "gi");
        let out = re.replace_all_with("Song [Skit] [HD]", |m| {
            if m.group(1).unwrap().trim().eq_ignore_ascii_case("skit") {
                m.text()
            } else {
                " ".into()
            }
        });
        assert_eq!(out, "Song [Skit] ");
        let re = JsRegex::new(r"^(.*)\s*[-–—|]\s*Linkin Park(?:\s+(\([^)]+\)))?\s*$", "i");
        let m = re.find("Good Goodbye - Linkin Park (feat. X)").unwrap();
        assert_eq!(m.group(1).unwrap(), "Good Goodbye ");
        assert_eq!(m.group(2).unwrap(), "(feat. X)");
    }
}
