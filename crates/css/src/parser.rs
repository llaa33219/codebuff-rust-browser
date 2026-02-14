use crate::selector::{ComplexSelector, parse_selector_list_from_tokens};
use crate::token::{CssToken, CssTokenizer};
use crate::value::{CssValue, parse_value_from_tokens};

/// A CSS declaration (property: value).
#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    /// Property name, e.g. `color`, `margin-left`.
    pub name: String,
    /// Parsed values.
    pub value: Vec<CssValue>,
    /// Whether `!important` was specified.
    pub important: bool,
}

/// A CSS style rule: selectors + declarations.
#[derive(Debug, Clone)]
pub struct CssRule {
    /// The selector list for this rule.
    pub selectors: Vec<ComplexSelector>,
    /// The declarations in the rule body.
    pub declarations: Vec<Declaration>,
}

/// A parsed CSS stylesheet.
#[derive(Debug)]
pub struct Stylesheet {
    /// All rules in the stylesheet, in source order.
    pub rules: Vec<CssRule>,
}

/// Parse a complete CSS stylesheet from a string.
pub fn parse_stylesheet(input: &str) -> Stylesheet {
    let mut tokenizer = CssTokenizer::new(input);
    let tokens = tokenizer.tokenize_all();
    let rules = parse_rules(&tokens);
    Stylesheet { rules }
}

/// Parse a list of CSS rules from a token stream.
fn parse_rules(tokens: &[CssToken]) -> Vec<CssRule> {
    let mut rules = Vec::new();
    let mut pos = 0;

    loop {
        // Skip whitespace
        while pos < tokens.len() && tokens[pos] == CssToken::Whitespace {
            pos += 1;
        }

        if pos >= tokens.len() {
            break;
        }

        // Handle at-rules: parse @media/@supports content, skip others.
        if let CssToken::AtKeyword(name) = &tokens[pos] {
            let lower_name = name.to_ascii_lowercase();
            match lower_name.as_str() {
                "media" | "supports" | "document" | "-moz-document" | "layer" | "container" => {
                    pos += 1; // skip @keyword
                    // Skip the condition/query part until '{'
                    while pos < tokens.len() && tokens[pos] != CssToken::LBrace {
                        pos += 1;
                    }
                    if pos < tokens.len() {
                        pos += 1; // skip '{'
                        // Find the matching '}' accounting for nested blocks
                        let block_start = pos;
                        let mut depth = 1;
                        while pos < tokens.len() && depth > 0 {
                            match &tokens[pos] {
                                CssToken::LBrace => depth += 1,
                                CssToken::RBrace => depth -= 1,
                                _ => {}
                            }
                            if depth > 0 {
                                pos += 1;
                            }
                        }
                        let block_tokens = &tokens[block_start..pos];
                        // Recursively parse rules inside the block
                        let inner_rules = parse_rules(block_tokens);
                        rules.extend(inner_rules);
                        if pos < tokens.len() {
                            pos += 1; // skip closing '}'
                        }
                    }
                }
                _ => {
                    // For @import, @charset, @namespace, @keyframes, @font-face, etc.
                    pos = skip_at_rule(tokens, pos);
                }
            }
            continue;
        }

        // Skip CDO/CDC (legacy HTML comment tokens in CSS)
        if tokens[pos] == CssToken::CDO || tokens[pos] == CssToken::CDC {
            pos += 1;
            continue;
        }

        // Try to parse a qualified rule (selector { declarations })
        match parse_qualified_rule(tokens, pos) {
            Some((rule, new_pos)) => {
                rules.push(rule);
                pos = new_pos;
            }
            None => {
                // Skip to next rule on error
                pos = skip_to_next_rule(tokens, pos);
            }
        }
    }

    rules
}

/// Skip an at-rule (consume until matching `;` or `{ ... }`).
fn skip_at_rule(tokens: &[CssToken], start: usize) -> usize {
    let mut pos = start + 1; // skip @keyword

    loop {
        if pos >= tokens.len() {
            return pos;
        }
        match &tokens[pos] {
            CssToken::Semicolon => return pos + 1,
            CssToken::LBrace => {
                return skip_block(tokens, pos);
            }
            _ => pos += 1,
        }
    }
}

/// Skip a `{ ... }` block, handling nested blocks.
fn skip_block(tokens: &[CssToken], start: usize) -> usize {
    let mut pos = start + 1; // skip '{'
    let mut depth = 1;
    while pos < tokens.len() && depth > 0 {
        match &tokens[pos] {
            CssToken::LBrace => depth += 1,
            CssToken::RBrace => depth -= 1,
            _ => {}
        }
        pos += 1;
    }
    pos
}

/// Skip to the start of the next rule.
fn skip_to_next_rule(tokens: &[CssToken], start: usize) -> usize {
    let mut pos = start;
    loop {
        if pos >= tokens.len() {
            return pos;
        }
        match &tokens[pos] {
            CssToken::RBrace => return pos + 1,
            CssToken::LBrace => return skip_block(tokens, pos),
            _ => pos += 1,
        }
    }
}

/// Parse a qualified rule: `selectors { declarations }`.
fn parse_qualified_rule(tokens: &[CssToken], start: usize) -> Option<(CssRule, usize)> {
    let mut pos = start;

    // Collect selector tokens until '{'
    let selector_start = pos;
    while pos < tokens.len() && tokens[pos] != CssToken::LBrace {
        pos += 1;
    }

    if pos >= tokens.len() {
        return None; // no '{' found
    }

    let selector_tokens = &tokens[selector_start..pos];
    let selectors = parse_selector_list_from_tokens(selector_tokens);

    if selectors.is_empty() {
        return None;
    }

    // Skip '{'
    pos += 1;

    // Collect declaration tokens until '}'
    let decl_start = pos;
    let mut depth = 1;
    while pos < tokens.len() && depth > 0 {
        match &tokens[pos] {
            CssToken::LBrace => depth += 1,
            CssToken::RBrace => depth -= 1,
            _ => {}
        }
        if depth > 0 {
            pos += 1;
        }
    }

    let decl_tokens = &tokens[decl_start..pos];
    let declarations = parse_declaration_block(decl_tokens);

    // Skip '}'
    if pos < tokens.len() {
        pos += 1;
    }

    Some((
        CssRule {
            selectors,
            declarations,
        },
        pos,
    ))
}

/// Parse a declaration block (the content between `{` and `}`).
/// Returns a list of declarations.
pub fn parse_declaration_block(tokens: &[CssToken]) -> Vec<Declaration> {
    let mut declarations = Vec::new();
    let mut pos = 0;

    loop {
        // Skip whitespace and semicolons
        while pos < tokens.len()
            && (tokens[pos] == CssToken::Whitespace || tokens[pos] == CssToken::Semicolon)
        {
            pos += 1;
        }

        if pos >= tokens.len() {
            break;
        }

        match parse_declaration(tokens, pos) {
            Some((decl, new_pos)) => {
                declarations.push(decl);
                pos = new_pos;
            }
            None => {
                // Skip to next semicolon or end on error
                while pos < tokens.len() && tokens[pos] != CssToken::Semicolon {
                    pos += 1;
                }
                if pos < tokens.len() {
                    pos += 1; // skip ';'
                }
            }
        }
    }

    declarations
}

/// Parse a single declaration: `property: value [!important]`.
fn parse_declaration(tokens: &[CssToken], start: usize) -> Option<(Declaration, usize)> {
    let mut pos = start;

    // Skip whitespace
    while pos < tokens.len() && tokens[pos] == CssToken::Whitespace {
        pos += 1;
    }

    // Expect property name (ident)
    let name = match tokens.get(pos) {
        Some(CssToken::Ident(name)) => {
            let n = name.to_ascii_lowercase();
            pos += 1;
            n
        }
        _ => return None,
    };

    // Skip whitespace
    while pos < tokens.len() && tokens[pos] == CssToken::Whitespace {
        pos += 1;
    }

    // Expect ':'
    if tokens.get(pos) != Some(&CssToken::Colon) {
        return None;
    }
    pos += 1;

    // Skip whitespace
    while pos < tokens.len() && tokens[pos] == CssToken::Whitespace {
        pos += 1;
    }

    // Collect value tokens until ';', '}', or end
    let value_start = pos;
    while pos < tokens.len()
        && tokens[pos] != CssToken::Semicolon
        && tokens[pos] != CssToken::RBrace
    {
        pos += 1;
    }

    let mut value_tokens = &tokens[value_start..pos];

    // Trim trailing whitespace tokens
    while !value_tokens.is_empty() && value_tokens.last() == Some(&CssToken::Whitespace) {
        value_tokens = &value_tokens[..value_tokens.len() - 1];
    }

    // Check for !important
    let (value_tokens, important) = check_important(value_tokens);

    // Parse values
    let value = parse_value_from_tokens(value_tokens);

    // Skip the semicolon if present
    if pos < tokens.len() && tokens[pos] == CssToken::Semicolon {
        pos += 1;
    }

    Some((Declaration { name, value, important }, pos))
}

/// Check if the value tokens end with `!important`, and strip it if so.
fn check_important(tokens: &[CssToken]) -> (&[CssToken], bool) {
    // Look for: Delim('!') Whitespace? Ident("important")
    let len = tokens.len();
    if len == 0 {
        return (tokens, false);
    }

    // Walk backwards to find !important pattern
    let mut end = len;

    // Trim trailing whitespace
    while end > 0 && tokens[end - 1] == CssToken::Whitespace {
        end -= 1;
    }

    // Check for "important" ident
    if end > 0 {
        if let CssToken::Ident(name) = &tokens[end - 1] {
            if name.eq_ignore_ascii_case("important") {
                let important_pos = end - 1;
                let mut check = important_pos;

                // Skip whitespace before "important"
                while check > 0 && tokens[check - 1] == CssToken::Whitespace {
                    check -= 1;
                }

                // Check for '!'
                if check > 0 {
                    if let CssToken::Delim('!') = &tokens[check - 1] {
                        return (&tokens[..check - 1], true);
                    }
                }
            }
        }
    }

    (tokens, false)
}

/// Parse a selector list string into complex selectors.
/// This is a convenience re-export that uses the selector module.
pub fn parse_selector_list(input: &str) -> Vec<ComplexSelector> {
    crate::selector::parse_selector_list(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selector::{
        Combinator, SimpleSelector, Specificity, compute_specificity,
    };
    use crate::value::{CssColor, CssValue, LengthUnit};

    #[test]
    fn test_parse_simple_stylesheet() {
        let css = r#"
            body {
                color: red;
                margin: 10px;
            }
        "#;
        let stylesheet = parse_stylesheet(css);
        assert_eq!(stylesheet.rules.len(), 1);

        let rule = &stylesheet.rules[0];
        assert_eq!(rule.selectors.len(), 1);
        assert_eq!(
            rule.selectors[0].parts[0].0.simples,
            vec![SimpleSelector::Type("body".into())]
        );
        assert_eq!(rule.declarations.len(), 2);
        assert_eq!(rule.declarations[0].name, "color");
        assert_eq!(rule.declarations[1].name, "margin");
    }

    #[test]
    fn test_parse_multiple_selectors() {
        let css = "h1, h2, h3 { font-weight: bold; }";
        let stylesheet = parse_stylesheet(css);
        assert_eq!(stylesheet.rules.len(), 1);
        assert_eq!(stylesheet.rules[0].selectors.len(), 3);
    }

    #[test]
    fn test_parse_multiple_rules() {
        let css = r#"
            body { color: black; }
            p { font-size: 16px; }
            .highlight { background-color: yellow; }
        "#;
        let stylesheet = parse_stylesheet(css);
        assert_eq!(stylesheet.rules.len(), 3);
    }

    #[test]
    fn test_parse_declaration_values() {
        let css = "div { margin: 10px 20px; }";
        let stylesheet = parse_stylesheet(css);
        let decl = &stylesheet.rules[0].declarations[0];
        assert_eq!(decl.name, "margin");
        assert_eq!(decl.value.len(), 2);
        assert_eq!(decl.value[0], CssValue::Length(10.0, LengthUnit::Px));
        assert_eq!(decl.value[1], CssValue::Length(20.0, LengthUnit::Px));
    }

    #[test]
    fn test_parse_color_values() {
        let css = "p { color: #ff0000; background: rgb(0, 128, 255); }";
        let stylesheet = parse_stylesheet(css);
        let decls = &stylesheet.rules[0].declarations;

        assert_eq!(decls[0].name, "color");
        assert_eq!(decls[0].value[0], CssValue::Color(CssColor::rgb(255, 0, 0)));

        assert_eq!(decls[1].name, "background");
        assert_eq!(
            decls[1].value[0],
            CssValue::Color(CssColor::rgb(0, 128, 255))
        );
    }

    #[test]
    fn test_parse_important() {
        let css = "p { color: red !important; font-size: 12px; }";
        let stylesheet = parse_stylesheet(css);
        let decls = &stylesheet.rules[0].declarations;

        assert_eq!(decls[0].name, "color");
        assert!(decls[0].important);

        assert_eq!(decls[1].name, "font-size");
        assert!(!decls[1].important);
    }

    #[test]
    fn test_parse_descendant_selector() {
        let css = "div p { color: blue; }";
        let stylesheet = parse_stylesheet(css);
        let sel = &stylesheet.rules[0].selectors[0];

        // RTL: p is first, div is second
        assert_eq!(sel.parts.len(), 2);
        assert_eq!(
            sel.parts[0].0.simples,
            vec![SimpleSelector::Type("p".into())]
        );
        assert_eq!(sel.parts[0].1, Some(Combinator::Descendant));
        assert_eq!(
            sel.parts[1].0.simples,
            vec![SimpleSelector::Type("div".into())]
        );
    }

    #[test]
    fn test_parse_child_selector() {
        let css = "ul > li { list-style: none; }";
        let stylesheet = parse_stylesheet(css);
        let sel = &stylesheet.rules[0].selectors[0];
        assert_eq!(sel.parts.len(), 2);
        assert_eq!(sel.parts[0].1, Some(Combinator::Child));
    }

    #[test]
    fn test_parse_class_and_id_selectors() {
        let css = "#main .content { padding: 10px; }";
        let stylesheet = parse_stylesheet(css);
        let sel = &stylesheet.rules[0].selectors[0];

        assert_eq!(sel.parts.len(), 2);
        // RTL: .content is first
        assert_eq!(
            sel.parts[0].0.simples,
            vec![SimpleSelector::Class("content".into())]
        );
        // #main is second
        assert_eq!(
            sel.parts[1].0.simples,
            vec![SimpleSelector::Id("main".into())]
        );
    }

    #[test]
    fn test_specificity_from_parsed_selectors() {
        let css = "div.foo#bar { color: red; }";
        let stylesheet = parse_stylesheet(css);
        let sel = &stylesheet.rules[0].selectors[0];
        let spec = compute_specificity(sel);
        // 1 ID + 1 class + 1 type = (1, 1, 1)
        assert_eq!(spec, Specificity::new(1, 1, 1));
    }

    #[test]
    fn test_specificity_comparison() {
        // #id → (1,0,0) vs .class → (0,1,0)
        let css1 = "#id { color: red; }";
        let css2 = ".class { color: blue; }";
        let s1 = parse_stylesheet(css1);
        let s2 = parse_stylesheet(css2);

        let spec1 = compute_specificity(&s1.rules[0].selectors[0]);
        let spec2 = compute_specificity(&s2.rules[0].selectors[0]);
        assert!(spec1 > spec2);
    }

    #[test]
    fn test_parse_named_color_value() {
        let css = "body { color: navy; }";
        let stylesheet = parse_stylesheet(css);
        let decl = &stylesheet.rules[0].declarations[0];
        assert_eq!(decl.value[0], CssValue::Color(CssColor::rgb(0, 0, 128)));
    }

    #[test]
    fn test_parse_keyword_value() {
        let css = "div { display: block; }";
        let stylesheet = parse_stylesheet(css);
        let decl = &stylesheet.rules[0].declarations[0];
        assert_eq!(decl.value[0], CssValue::Keyword("block".into()));
    }

    #[test]
    fn test_parse_none_value() {
        let css = "div { display: none; }";
        let stylesheet = parse_stylesheet(css);
        let decl = &stylesheet.rules[0].declarations[0];
        assert_eq!(decl.value[0], CssValue::None);
    }

    #[test]
    fn test_parse_inherit_value() {
        let css = "div { color: inherit; }";
        let stylesheet = parse_stylesheet(css);
        let decl = &stylesheet.rules[0].declarations[0];
        assert_eq!(decl.value[0], CssValue::Inherit);
    }

    #[test]
    fn test_parse_comments_ignored() {
        let css = r#"
            /* This is a comment */
            body {
                color: /* inline comment */ red;
            }
        "#;
        let stylesheet = parse_stylesheet(css);
        assert_eq!(stylesheet.rules.len(), 1);
        assert_eq!(stylesheet.rules[0].declarations[0].name, "color");
    }

    #[test]
    fn test_parse_empty_stylesheet() {
        let css = "   \n\t  ";
        let stylesheet = parse_stylesheet(css);
        assert_eq!(stylesheet.rules.len(), 0);
    }

    #[test]
    fn test_parse_empty_rule() {
        let css = "div { }";
        let stylesheet = parse_stylesheet(css);
        assert_eq!(stylesheet.rules.len(), 1);
        assert_eq!(stylesheet.rules[0].declarations.len(), 0);
    }

    #[test]
    fn test_parse_at_rule_skipped() {
        let css = r#"
            @import url("style.css");
            body { color: red; }
        "#;
        let stylesheet = parse_stylesheet(css);
        assert_eq!(stylesheet.rules.len(), 1);
        assert_eq!(
            stylesheet.rules[0].selectors[0].parts[0].0.simples,
            vec![SimpleSelector::Type("body".into())]
        );
    }

    #[test]
    fn test_parse_media_rule() {
        let css = r#"
            @media screen {
                body { color: blue; }
                p { font-size: 14px; }
            }
        "#;
        let stylesheet = parse_stylesheet(css);
        assert_eq!(stylesheet.rules.len(), 2);
        assert_eq!(stylesheet.rules[0].declarations[0].name, "color");
        assert_eq!(stylesheet.rules[1].declarations[0].name, "font-size");
    }

    #[test]
    fn test_parse_media_rule_with_outer_rules() {
        let css = r#"
            h1 { color: red; }
            @media (min-width: 768px) {
                h1 { color: green; }
            }
            p { margin: 10px; }
        "#;
        let stylesheet = parse_stylesheet(css);
        assert_eq!(stylesheet.rules.len(), 3);
    }

    #[test]
    fn test_parse_nested_media_rules() {
        let css = r#"
            @media screen {
                @media (min-width: 0) {
                    div { color: red; }
                }
            }
        "#;
        let stylesheet = parse_stylesheet(css);
        assert_eq!(stylesheet.rules.len(), 1);
        assert_eq!(
            stylesheet.rules[0].selectors[0].parts[0].0.simples,
            vec![SimpleSelector::Type("div".into())]
        );
    }

    #[test]
    fn test_parse_percentage_value() {
        let css = "div { width: 50%; }";
        let stylesheet = parse_stylesheet(css);
        let decl = &stylesheet.rules[0].declarations[0];
        assert_eq!(decl.value[0], CssValue::Percentage(50.0));
    }

    #[test]
    fn test_parse_em_units() {
        let css = "p { font-size: 1.5em; }";
        let stylesheet = parse_stylesheet(css);
        let decl = &stylesheet.rules[0].declarations[0];
        assert_eq!(decl.value[0], CssValue::Length(1.5, LengthUnit::Em));
    }

    #[test]
    fn test_parse_comprehensive() {
        let css = r#"
            * { margin: 0; padding: 0; }
            body { font-size: 16px; color: #333; }
            h1, h2 { color: navy; }
            .container { width: 80%; margin: 0 auto; }
            #header > nav a:hover { color: red !important; }
        "#;
        let stylesheet = parse_stylesheet(css);
        assert_eq!(stylesheet.rules.len(), 5);

        // Rule 0: * { margin: 0; padding: 0; }
        assert_eq!(
            stylesheet.rules[0].selectors[0].parts[0].0.simples,
            vec![SimpleSelector::Universal]
        );
        assert_eq!(stylesheet.rules[0].declarations.len(), 2);

        // Rule 1: body { ... }
        assert_eq!(stylesheet.rules[1].declarations.len(), 2);

        // Rule 2: h1, h2
        assert_eq!(stylesheet.rules[2].selectors.len(), 2);

        // Rule 3: .container
        assert_eq!(
            stylesheet.rules[3].selectors[0].parts[0].0.simples,
            vec![SimpleSelector::Class("container".into())]
        );

        // Rule 4: #header > nav a:hover
        let sel = &stylesheet.rules[4].selectors[0];
        assert!(sel.parts.len() >= 2);
        assert!(stylesheet.rules[4].declarations[0].important);
    }
}
