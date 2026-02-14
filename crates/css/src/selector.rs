use crate::token::CssToken;

/// Combinator between compound selectors in a complex selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Combinator {
    /// Whitespace: ancestor descendant
    Descendant,
    /// `>`: parent > child
    Child,
    /// `+`: prev + next
    NextSibling,
    /// `~`: prev ~ subsequent
    SubsequentSibling,
}

/// Attribute selector operator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttrOp {
    /// `[attr]`
    Exists,
    /// `[attr=val]`
    Eq,
    /// `[attr~=val]`
    Includes,
    /// `[attr|=val]`
    DashMatch,
    /// `[attr^=val]`
    Prefix,
    /// `[attr$=val]`
    Suffix,
    /// `[attr*=val]`
    Substring,
}

/// Pseudo-class selectors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PseudoClass {
    Hover,
    Active,
    Focus,
    FirstChild,
    LastChild,
    /// `nth-child(an+b)` with coefficients `(a, b)`.
    NthChild(i32, i32),
    /// `:not(...)` containing a compound selector.
    Not(Box<CompoundSelector>),
    Link,
    Visited,
    Root,
    FirstOfType,
    LastOfType,
    OnlyChild,
    OnlyOfType,
    Empty,
    Enabled,
    Disabled,
    Checked,
    AnyLink,
    FocusVisible,
    FocusWithin,
    Placeholder,
}

/// Pseudo-element selectors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PseudoElement {
    Before,
    After,
    FirstLine,
    FirstLetter,
}

/// A single simple selector component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimpleSelector {
    /// Type selector, e.g. `div`, `p`.
    Type(String),
    /// Universal selector `*`.
    Universal,
    /// ID selector `#foo`.
    Id(String),
    /// Class selector `.bar`.
    Class(String),
    /// Attribute selector `[name op value]`.
    Attribute {
        name: String,
        op: AttrOp,
        value: Option<String>,
    },
    /// Pseudo-class selector.
    PseudoClass(PseudoClass),
    /// Pseudo-element selector.
    PseudoElement(PseudoElement),
}

/// A compound selector is a sequence of simple selectors
/// without any combinator between them (e.g. `div.foo#bar`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompoundSelector {
    pub simples: Vec<SimpleSelector>,
}

/// A complex selector is a chain of compound selectors separated by combinators.
/// Stored right-to-left for efficient matching: `parts[0]` is the rightmost
/// (subject) compound selector.
///
/// Each element is `(compound_selector, optional_combinator_to_the_left)`.
/// The last element's combinator is always `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComplexSelector {
    pub parts: Vec<(CompoundSelector, Option<Combinator>)>,
}

/// CSS specificity as a triple `(a, b, c)`:
///   - `a`: count of ID selectors
///   - `b`: count of class selectors, attribute selectors, and pseudo-classes
///   - `c`: count of type selectors and pseudo-elements
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Specificity {
    pub a: u32,
    pub b: u32,
    pub c: u32,
}

impl Specificity {
    pub fn new(a: u32, b: u32, c: u32) -> Self {
        Self { a, b, c }
    }

    pub fn zero() -> Self {
        Self { a: 0, b: 0, c: 0 }
    }

    /// Add two specificities component-wise.
    pub fn add(self, other: Specificity) -> Specificity {
        Specificity {
            a: self.a + other.a,
            b: self.b + other.b,
            c: self.c + other.c,
        }
    }
}

impl PartialOrd for Specificity {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Specificity {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.a
            .cmp(&other.a)
            .then(self.b.cmp(&other.b))
            .then(self.c.cmp(&other.c))
    }
}

/// Compute the specificity of a complex selector.
pub fn compute_specificity(selector: &ComplexSelector) -> Specificity {
    let mut spec = Specificity::zero();
    for (compound, _) in &selector.parts {
        spec = spec.add(compound_specificity(compound));
    }
    spec
}

/// Compute the specificity contribution of a compound selector.
fn compound_specificity(compound: &CompoundSelector) -> Specificity {
    let mut spec = Specificity::zero();
    for simple in &compound.simples {
        spec = spec.add(simple_specificity(simple));
    }
    spec
}

/// Compute the specificity contribution of a single simple selector.
fn simple_specificity(simple: &SimpleSelector) -> Specificity {
    match simple {
        SimpleSelector::Id(_) => Specificity::new(1, 0, 0),
        SimpleSelector::Class(_) => Specificity::new(0, 1, 0),
        SimpleSelector::Attribute { .. } => Specificity::new(0, 1, 0),
        SimpleSelector::PseudoClass(pc) => match pc {
            // :not() uses the specificity of its argument
            PseudoClass::Not(inner) => compound_specificity(inner),
            _ => Specificity::new(0, 1, 0),
        },
        SimpleSelector::Type(_) => Specificity::new(0, 0, 1),
        SimpleSelector::PseudoElement(_) => Specificity::new(0, 0, 1),
        SimpleSelector::Universal => Specificity::zero(),
    }
}

/// Parse a selector list from a CSS selector string.
/// Returns a vector of complex selectors separated by commas.
pub fn parse_selector_list(input: &str) -> Vec<ComplexSelector> {
    use crate::token::CssTokenizer;

    let mut tokenizer = CssTokenizer::new(input);
    let tokens = tokenizer.tokenize_all();
    parse_selector_list_from_tokens(&tokens)
}

/// Parse a selector list from a slice of tokens.
pub fn parse_selector_list_from_tokens(tokens: &[CssToken]) -> Vec<ComplexSelector> {
    let mut selectors = Vec::new();
    let mut pos = 0;

    // Skip leading whitespace
    while pos < tokens.len() && tokens[pos] == CssToken::Whitespace {
        pos += 1;
    }

    loop {
        if pos >= tokens.len() {
            break;
        }

        let (selector, new_pos) = parse_complex_selector(tokens, pos);
        if !selector.parts.is_empty() {
            selectors.push(selector);
        }
        pos = new_pos;

        // Skip comma separator
        while pos < tokens.len() && tokens[pos] == CssToken::Whitespace {
            pos += 1;
        }
        if pos < tokens.len() && tokens[pos] == CssToken::Comma {
            pos += 1;
            while pos < tokens.len() && tokens[pos] == CssToken::Whitespace {
                pos += 1;
            }
        } else {
            break;
        }
    }

    selectors
}

/// Parse a single complex selector from tokens starting at `pos`.
/// Returns the parsed selector and the position after it.
fn parse_complex_selector(tokens: &[CssToken], start: usize) -> (ComplexSelector, usize) {
    let mut parts_ltr: Vec<(CompoundSelector, Option<Combinator>)> = Vec::new();
    let mut pos = start;

    // Parse first compound selector
    let (compound, new_pos) = parse_compound_selector(tokens, pos);
    if compound.simples.is_empty() {
        return (ComplexSelector { parts: Vec::new() }, new_pos);
    }
    parts_ltr.push((compound, None));
    pos = new_pos;

    // Parse combinator + compound pairs
    loop {
        let had_whitespace = pos < tokens.len() && tokens[pos] == CssToken::Whitespace;
        if had_whitespace {
            while pos < tokens.len() && tokens[pos] == CssToken::Whitespace {
                pos += 1;
            }
        }

        if pos >= tokens.len() {
            break;
        }

        // Check for explicit combinator
        let combinator = match &tokens[pos] {
            CssToken::Delim('>') => {
                pos += 1;
                while pos < tokens.len() && tokens[pos] == CssToken::Whitespace {
                    pos += 1;
                }
                Some(Combinator::Child)
            }
            CssToken::Delim('+') => {
                pos += 1;
                while pos < tokens.len() && tokens[pos] == CssToken::Whitespace {
                    pos += 1;
                }
                Some(Combinator::NextSibling)
            }
            CssToken::Delim('~') => {
                pos += 1;
                while pos < tokens.len() && tokens[pos] == CssToken::Whitespace {
                    pos += 1;
                }
                Some(Combinator::SubsequentSibling)
            }
            _ => {
                if had_whitespace {
                    Some(Combinator::Descendant)
                } else {
                    None
                }
            }
        };

        // If no combinator found (and no whitespace), we're done
        let combinator = match combinator {
            Some(c) => c,
            None => break,
        };

        // Check if next thing looks like a compound selector start
        if pos >= tokens.len() || is_selector_terminator(&tokens[pos]) {
            break;
        }

        let (compound, new_pos) = parse_compound_selector(tokens, pos);
        if compound.simples.is_empty() {
            break;
        }
        parts_ltr.push((compound, Some(combinator)));
        pos = new_pos;
    }

    // Reverse to right-to-left order.
    // In LTR: parts_ltr = [(A, None), (B, Some(Child)), (C, Some(Descendant))]
    // Each element's combinator describes how it connects to the previous element.
    // Reversed: [(C, Some(Descendant)), (B, Some(Child)), (A, None)]
    // Now each element's combinator describes how to traverse to the next element in RTL.
    parts_ltr.reverse();

    (ComplexSelector { parts: parts_ltr }, pos)
}

/// Parse a compound selector (sequence of simple selectors without combinators).
fn parse_compound_selector(tokens: &[CssToken], start: usize) -> (CompoundSelector, usize) {
    let mut simples = Vec::new();
    let mut pos = start;

    loop {
        if pos >= tokens.len() {
            break;
        }

        match &tokens[pos] {
            // Type selector or universal
            CssToken::Ident(name) if simples.is_empty() || !has_type_or_universal(&simples) => {
                simples.push(SimpleSelector::Type(name.to_ascii_lowercase()));
                pos += 1;
            }
            CssToken::Ident(_) if has_type_or_universal(&simples) => {
                // Can't have two type selectors; this must be something else
                break;
            }
            CssToken::Delim('*') => {
                simples.push(SimpleSelector::Universal);
                pos += 1;
            }

            // ID selector: Hash token with is_id
            CssToken::Hash { value, .. } => {
                simples.push(SimpleSelector::Id(value.clone()));
                pos += 1;
            }

            // Class selector: . followed by ident
            CssToken::Delim('.') => {
                pos += 1;
                if pos < tokens.len() {
                    if let CssToken::Ident(name) = &tokens[pos] {
                        simples.push(SimpleSelector::Class(name.clone()));
                        pos += 1;
                    }
                }
            }

            // Attribute selector
            CssToken::LBracket => {
                let (attr_sel, new_pos) = parse_attribute_selector(tokens, pos);
                if let Some(sel) = attr_sel {
                    simples.push(sel);
                }
                pos = new_pos;
            }

            // Pseudo-element (::before, ::after, etc.)
            CssToken::Colon if pos + 1 < tokens.len() && tokens[pos + 1] == CssToken::Colon => {
                pos += 2; // skip ::
                if pos < tokens.len() {
                    if let CssToken::Ident(name) = &tokens[pos] {
                        let pe = match name.to_ascii_lowercase().as_str() {
                            "before" => Some(PseudoElement::Before),
                            "after" => Some(PseudoElement::After),
                            "first-line" => Some(PseudoElement::FirstLine),
                            "first-letter" => Some(PseudoElement::FirstLetter),
                            _ => None,
                        };
                        if let Some(pe) = pe {
                            simples.push(SimpleSelector::PseudoElement(pe));
                        }
                        pos += 1;
                    }
                }
            }

            // Pseudo-class (:hover, :first-child, :nth-child(...), :not(...))
            CssToken::Colon => {
                pos += 1; // skip :
                if pos < tokens.len() {
                    match &tokens[pos] {
                        CssToken::Ident(name) => {
                            let pc = match name.to_ascii_lowercase().as_str() {
                                "hover" => Some(PseudoClass::Hover),
                                "active" => Some(PseudoClass::Active),
                                "focus" => Some(PseudoClass::Focus),
                                "focus-visible" => Some(PseudoClass::FocusVisible),
                                "focus-within" => Some(PseudoClass::FocusWithin),
                                "first-child" => Some(PseudoClass::FirstChild),
                                "last-child" => Some(PseudoClass::LastChild),
                                "first-of-type" => Some(PseudoClass::FirstOfType),
                                "last-of-type" => Some(PseudoClass::LastOfType),
                                "only-child" => Some(PseudoClass::OnlyChild),
                                "only-of-type" => Some(PseudoClass::OnlyOfType),
                                "empty" => Some(PseudoClass::Empty),
                                "enabled" => Some(PseudoClass::Enabled),
                                "disabled" => Some(PseudoClass::Disabled),
                                "checked" => Some(PseudoClass::Checked),
                                "any-link" => Some(PseudoClass::AnyLink),
                                "placeholder-shown" => Some(PseudoClass::Placeholder),
                                "link" => Some(PseudoClass::Link),
                                "visited" => Some(PseudoClass::Visited),
                                "root" => Some(PseudoClass::Root),
                                _ => None,
                            };
                            if let Some(pc) = pc {
                                simples.push(SimpleSelector::PseudoClass(pc));
                            }
                            pos += 1;
                        }
                        CssToken::Function(name) => {
                            let lower = name.to_ascii_lowercase();
                            match lower.as_str() {
                                "nth-child" => {
                                    let (a, b, new_pos) = parse_nth_args(tokens, pos + 1);
                                    simples.push(SimpleSelector::PseudoClass(
                                        PseudoClass::NthChild(a, b),
                                    ));
                                    pos = new_pos;
                                }
                                "not" => {
                                    let (inner, new_pos) = parse_not_args(tokens, pos + 1);
                                    simples.push(SimpleSelector::PseudoClass(PseudoClass::Not(
                                        Box::new(inner),
                                    )));
                                    pos = new_pos;
                                }
                                "is" | "where" | "matches" | "any"
                                | "-webkit-any" | "-moz-any" | "has" => {
                                    // Treat as always-matching: skip arguments but
                                    // add Universal so the rule still applies.
                                    pos = skip_to_matching_rparen(tokens, pos + 1);
                                    simples.push(SimpleSelector::Universal);
                                }
                                _ => {
                                    // Skip unknown function and its arguments
                                    pos = skip_to_matching_rparen(tokens, pos + 1);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }

            _ => break,
        }
    }

    (CompoundSelector { simples }, pos)
}

fn has_type_or_universal(simples: &[SimpleSelector]) -> bool {
    simples.iter().any(|s| {
        matches!(s, SimpleSelector::Type(_) | SimpleSelector::Universal)
    })
}

fn is_selector_terminator(token: &CssToken) -> bool {
    matches!(
        token,
        CssToken::Comma | CssToken::LBrace | CssToken::RBrace | CssToken::RParen | CssToken::EOF
    )
}

/// Parse an attribute selector `[name op? value?]`.
fn parse_attribute_selector(
    tokens: &[CssToken],
    start: usize,
) -> (Option<SimpleSelector>, usize) {
    let mut pos = start + 1; // skip '['

    // Skip whitespace
    while pos < tokens.len() && tokens[pos] == CssToken::Whitespace {
        pos += 1;
    }

    // Attribute name
    let name = if pos < tokens.len() {
        if let CssToken::Ident(n) = &tokens[pos] {
            pos += 1;
            n.clone()
        } else {
            return (None, skip_to_rbracket(tokens, pos));
        }
    } else {
        return (None, pos);
    };

    // Skip whitespace
    while pos < tokens.len() && tokens[pos] == CssToken::Whitespace {
        pos += 1;
    }

    // Check for ] (exists-only selector)
    if pos < tokens.len() && tokens[pos] == CssToken::RBracket {
        pos += 1;
        return (
            Some(SimpleSelector::Attribute {
                name,
                op: AttrOp::Exists,
                value: None,
            }),
            pos,
        );
    }

    // Parse operator
    let op = if pos < tokens.len() {
        match &tokens[pos] {
            CssToken::Delim('=') => {
                pos += 1;
                AttrOp::Eq
            }
            CssToken::Delim('~') if matches!(tokens.get(pos + 1), Some(CssToken::Delim('='))) => {
                pos += 2;
                AttrOp::Includes
            }
            CssToken::Delim('|') if matches!(tokens.get(pos + 1), Some(CssToken::Delim('='))) => {
                pos += 2;
                AttrOp::DashMatch
            }
            CssToken::Delim('^') if matches!(tokens.get(pos + 1), Some(CssToken::Delim('='))) => {
                pos += 2;
                AttrOp::Prefix
            }
            CssToken::Delim('$') if matches!(tokens.get(pos + 1), Some(CssToken::Delim('='))) => {
                pos += 2;
                AttrOp::Suffix
            }
            CssToken::Delim('*') if matches!(tokens.get(pos + 1), Some(CssToken::Delim('='))) => {
                pos += 2;
                AttrOp::Substring
            }
            _ => return (None, skip_to_rbracket(tokens, pos)),
        }
    } else {
        return (None, pos);
    };

    // Skip whitespace
    while pos < tokens.len() && tokens[pos] == CssToken::Whitespace {
        pos += 1;
    }

    // Parse value (ident or string)
    let value = if pos < tokens.len() {
        match &tokens[pos] {
            CssToken::Ident(v) => {
                pos += 1;
                Some(v.clone())
            }
            CssToken::String(v) => {
                pos += 1;
                Some(v.clone())
            }
            _ => None,
        }
    } else {
        None
    };

    // Skip whitespace
    while pos < tokens.len() && tokens[pos] == CssToken::Whitespace {
        pos += 1;
    }

    // Expect ]
    if pos < tokens.len() && tokens[pos] == CssToken::RBracket {
        pos += 1;
    }

    (Some(SimpleSelector::Attribute { name, op, value }), pos)
}

fn skip_to_rbracket(tokens: &[CssToken], start: usize) -> usize {
    let mut pos = start;
    while pos < tokens.len() && tokens[pos] != CssToken::RBracket {
        pos += 1;
    }
    if pos < tokens.len() {
        pos + 1
    } else {
        pos
    }
}

fn skip_to_matching_rparen(tokens: &[CssToken], start: usize) -> usize {
    let mut pos = start;
    let mut depth = 1;
    while pos < tokens.len() && depth > 0 {
        match &tokens[pos] {
            CssToken::LParen | CssToken::Function(_) => depth += 1,
            CssToken::RParen => depth -= 1,
            _ => {}
        }
        pos += 1;
    }
    pos
}

/// Parse `an+b` notation arguments for `:nth-child(...)`.
fn parse_nth_args(tokens: &[CssToken], start: usize) -> (i32, i32, usize) {
    let mut pos = start;

    // Skip whitespace
    while pos < tokens.len() && tokens[pos] == CssToken::Whitespace {
        pos += 1;
    }

    // Simple cases: "odd", "even", or a plain number
    if let Some(CssToken::Ident(name)) = tokens.get(pos) {
        let lower = name.to_ascii_lowercase();
        match lower.as_str() {
            "odd" => {
                pos += 1;
                let end = skip_to_matching_rparen(tokens, pos);
                return (2, 1, end);
            }
            "even" => {
                pos += 1;
                let end = skip_to_matching_rparen(tokens, pos);
                return (2, 0, end);
            }
            _ => {}
        }
    }

    if let Some(&CssToken::Number { value, .. }) = tokens.get(pos) {
        let b = value as i32;
        pos += 1;
        let end = skip_to_matching_rparen(tokens, pos);
        return (0, b, end);
    }

    // Skip to closing paren for anything more complex
    let end = skip_to_matching_rparen(tokens, pos);
    (0, 0, end)
}

/// Parse the argument of `:not(...)`.
fn parse_not_args(tokens: &[CssToken], start: usize) -> (CompoundSelector, usize) {
    let mut pos = start;

    // Skip whitespace
    while pos < tokens.len() && tokens[pos] == CssToken::Whitespace {
        pos += 1;
    }

    let (compound, new_pos) = parse_compound_selector(tokens, pos);
    pos = new_pos;

    // Skip whitespace
    while pos < tokens.len() && tokens[pos] == CssToken::Whitespace {
        pos += 1;
    }

    // Expect )
    if pos < tokens.len() && tokens[pos] == CssToken::RParen {
        pos += 1;
    }

    (compound, pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_type_selector() {
        let selectors = parse_selector_list("div");
        assert_eq!(selectors.len(), 1);
        assert_eq!(selectors[0].parts.len(), 1);
        assert_eq!(
            selectors[0].parts[0].0.simples,
            vec![SimpleSelector::Type("div".into())]
        );
    }

    #[test]
    fn test_class_and_id() {
        let selectors = parse_selector_list("div.foo#bar");
        assert_eq!(selectors.len(), 1);
        let simples = &selectors[0].parts[0].0.simples;
        assert_eq!(simples.len(), 3);
        assert_eq!(simples[0], SimpleSelector::Type("div".into()));
        assert_eq!(simples[1], SimpleSelector::Class("foo".into()));
        assert_eq!(simples[2], SimpleSelector::Id("bar".into()));
    }

    #[test]
    fn test_descendant_combinator() {
        let selectors = parse_selector_list("div p");
        assert_eq!(selectors.len(), 1);
        let parts = &selectors[0].parts;
        // RTL: p first, div second
        assert_eq!(parts.len(), 2);
        assert_eq!(
            parts[0].0.simples,
            vec![SimpleSelector::Type("p".into())]
        );
        assert_eq!(parts[0].1, Some(Combinator::Descendant));
        assert_eq!(
            parts[1].0.simples,
            vec![SimpleSelector::Type("div".into())]
        );
        assert_eq!(parts[1].1, None);
    }

    #[test]
    fn test_child_combinator() {
        let selectors = parse_selector_list("ul > li");
        assert_eq!(selectors.len(), 1);
        let parts = &selectors[0].parts;
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].1, Some(Combinator::Child));
    }

    #[test]
    fn test_selector_list_comma() {
        let selectors = parse_selector_list("h1, h2, h3");
        assert_eq!(selectors.len(), 3);
    }

    #[test]
    fn test_specificity_type() {
        let selectors = parse_selector_list("div");
        let spec = compute_specificity(&selectors[0]);
        assert_eq!(spec, Specificity::new(0, 0, 1));
    }

    #[test]
    fn test_specificity_class() {
        let selectors = parse_selector_list(".foo");
        let spec = compute_specificity(&selectors[0]);
        assert_eq!(spec, Specificity::new(0, 1, 0));
    }

    #[test]
    fn test_specificity_id() {
        let selectors = parse_selector_list("#bar");
        let spec = compute_specificity(&selectors[0]);
        assert_eq!(spec, Specificity::new(1, 0, 0));
    }

    #[test]
    fn test_specificity_complex() {
        // div.foo#bar → (1, 1, 1)
        let selectors = parse_selector_list("div.foo#bar");
        let spec = compute_specificity(&selectors[0]);
        assert_eq!(spec, Specificity::new(1, 1, 1));
    }

    #[test]
    fn test_specificity_ordering() {
        let s1 = Specificity::new(0, 0, 1);
        let s2 = Specificity::new(0, 1, 0);
        let s3 = Specificity::new(1, 0, 0);
        assert!(s1 < s2);
        assert!(s2 < s3);
        assert!(s1 < s3);
    }

    #[test]
    fn test_specificity_descendant() {
        // div p → (0, 0, 2)
        let selectors = parse_selector_list("div p");
        let spec = compute_specificity(&selectors[0]);
        assert_eq!(spec, Specificity::new(0, 0, 2));
    }

    #[test]
    fn test_universal_zero_specificity() {
        let selectors = parse_selector_list("*");
        let spec = compute_specificity(&selectors[0]);
        assert_eq!(spec, Specificity::new(0, 0, 0));
    }

    #[test]
    fn test_pseudo_class() {
        let selectors = parse_selector_list("a:hover");
        assert_eq!(selectors.len(), 1);
        let simples = &selectors[0].parts[0].0.simples;
        assert_eq!(simples.len(), 2);
        assert_eq!(simples[1], SimpleSelector::PseudoClass(PseudoClass::Hover));
    }

    #[test]
    fn test_attribute_selector() {
        let selectors = parse_selector_list("[href]");
        assert_eq!(selectors.len(), 1);
        let simples = &selectors[0].parts[0].0.simples;
        assert_eq!(
            simples[0],
            SimpleSelector::Attribute {
                name: "href".into(),
                op: AttrOp::Exists,
                value: None,
            }
        );
    }

    #[test]
    fn test_attribute_eq_selector() {
        let selectors = parse_selector_list(r#"[type="text"]"#);
        assert_eq!(selectors.len(), 1);
        let simples = &selectors[0].parts[0].0.simples;
        assert_eq!(
            simples[0],
            SimpleSelector::Attribute {
                name: "type".into(),
                op: AttrOp::Eq,
                value: Some("text".into()),
            }
        );
    }

    #[test]
    fn test_pseudo_element() {
        let selectors = parse_selector_list("p::before");
        assert_eq!(selectors.len(), 1);
        let simples = &selectors[0].parts[0].0.simples;
        assert_eq!(simples.len(), 2);
        assert_eq!(
            simples[1],
            SimpleSelector::PseudoElement(PseudoElement::Before)
        );
    }
}
