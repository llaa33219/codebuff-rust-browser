//! Cascade resolution — collect matching rules, sort, and resolve computed values.
//!
//! Implements the CSS cascade: importance → origin → specificity → source order.
//! Handles inheritance of inheritable properties (color, font-*, text-align, line-height).

use css::{
    Declaration, Specificity, Stylesheet, compute_specificity,
    CssValue, CssColor, LengthUnit,
};
use dom::{Dom, NodeId};

use crate::computed::*;
use crate::matching::matches_selector;
use common::{Color, Edges};

// ─────────────────────────────────────────────────────────────────────────────
// Origin
// ─────────────────────────────────────────────────────────────────────────────

/// The origin of a CSS rule (determines cascade priority).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StyleOrigin {
    UserAgent,
    User,
    Author,
}

// ─────────────────────────────────────────────────────────────────────────────
// MatchedRule
// ─────────────────────────────────────────────────────────────────────────────

/// A rule that matched a particular element, annotated with cascade metadata.
#[derive(Debug, Clone)]
pub struct MatchedRule {
    pub specificity: Specificity,
    pub origin: StyleOrigin,
    pub source_order: usize,
    pub declarations: Vec<Declaration>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Collect matching rules
// ─────────────────────────────────────────────────────────────────────────────

/// Collect all rules from the given stylesheets that match `node_id`.
pub fn collect_matching_rules(
    dom: &Dom,
    node_id: NodeId,
    stylesheets: &[(Stylesheet, StyleOrigin)],
) -> Vec<MatchedRule> {
    let mut matched = Vec::new();
    let mut source_order = 0usize;

    for (stylesheet, origin) in stylesheets {
        for rule in &stylesheet.rules {
            // Check if any selector in the rule's selector list matches.
            let mut best_spec: Option<Specificity> = None;
            for selector in &rule.selectors {
                if matches_selector(dom, node_id, selector) {
                    let spec = compute_specificity(selector);
                    best_spec = Some(match best_spec {
                        Some(prev) if spec > prev => spec,
                        Some(prev) => prev,
                        None => spec,
                    });
                }
            }

            if let Some(specificity) = best_spec {
                matched.push(MatchedRule {
                    specificity,
                    origin: *origin,
                    source_order,
                    declarations: rule.declarations.clone(),
                });
                source_order += 1;
            }
        }
    }

    matched
}

// ─────────────────────────────────────────────────────────────────────────────
// Resolve style
// ─────────────────────────────────────────────────────────────────────────────

/// Resolve the computed style for a node given its matched rules and parent's
/// computed style (for inheritance).
///
/// Cascade order (low → high priority):
///   1. User-agent normal
///   2. User normal
///   3. Author normal
///   4. Author !important
///   5. User !important
///   6. User-agent !important
///
/// Within each level: higher specificity wins, then later source order wins.
pub fn resolve_style(
    _dom: &Dom,
    _node_id: NodeId,
    matched_rules: &[MatchedRule],
    parent_style: Option<&ComputedStyle>,
) -> ComputedStyle {
    // Start with inherited values from parent, or defaults.
    let mut style = match parent_style {
        Some(ps) => inherit_from_parent(ps),
        None => ComputedStyle::default(),
    };

    // Separate declarations into normal and important, then sort.
    let mut normal_decls: Vec<(&Declaration, Specificity, StyleOrigin, usize)> = Vec::new();
    let mut important_decls: Vec<(&Declaration, Specificity, StyleOrigin, usize)> = Vec::new();

    for rule in matched_rules {
        for decl in &rule.declarations {
            let entry = (decl, rule.specificity, rule.origin, rule.source_order);
            if decl.important {
                important_decls.push(entry);
            } else {
                normal_decls.push(entry);
            }
        }
    }

    // Sort normal declarations: origin (UA < User < Author), then specificity, then source order.
    normal_decls.sort_by(|a, b| {
        a.2.cmp(&b.2)
            .then(a.1.cmp(&b.1))
            .then(a.3.cmp(&b.3))
    });

    // Sort important declarations: origin reversed (Author < User < UA), then specificity, then source order.
    important_decls.sort_by(|a, b| {
        // For !important, origin priority is reversed
        let origin_a = important_origin_rank(a.2);
        let origin_b = important_origin_rank(b.2);
        origin_a
            .cmp(&origin_b)
            .then(a.1.cmp(&b.1))
            .then(a.3.cmp(&b.3))
    });

    // Apply normal declarations first, then important (later declarations win).
    for (decl, _, _, _) in &normal_decls {
        apply_declaration(&mut style, decl, parent_style);
    }
    for (decl, _, _, _) in &important_decls {
        apply_declaration(&mut style, decl, parent_style);
    }

    style
}

/// For !important, the origin priority is reversed:
/// Author !important < User !important < UA !important
fn important_origin_rank(origin: StyleOrigin) -> u8 {
    match origin {
        StyleOrigin::Author => 0,
        StyleOrigin::User => 1,
        StyleOrigin::UserAgent => 2,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Inheritance
// ─────────────────────────────────────────────────────────────────────────────

/// Create a new style that inherits inheritable properties from the parent.
/// Non-inheritable properties get their initial (default) values.
fn inherit_from_parent(parent: &ComputedStyle) -> ComputedStyle {
    let mut s = ComputedStyle::default();

    // Inherited properties:
    s.color = parent.color;
    s.font_size_px = parent.font_size_px;
    s.font_weight = parent.font_weight;
    s.font_family = parent.font_family.clone();
    s.font_style = parent.font_style;
    s.line_height_px = parent.line_height_px;
    s.text_align = parent.text_align;
    s.text_transform = parent.text_transform;
    s.text_indent = parent.text_indent;
    s.letter_spacing = parent.letter_spacing;
    s.word_spacing = parent.word_spacing;
    s.white_space = parent.white_space;
    s.visibility = parent.visibility;
    s.cursor = parent.cursor;
    s.list_style_type = parent.list_style_type;

    s
}

// ─────────────────────────────────────────────────────────────────────────────
// Apply a single declaration
// ─────────────────────────────────────────────────────────────────────────────

pub fn apply_declaration(
    style: &mut ComputedStyle,
    decl: &Declaration,
    parent_style: Option<&ComputedStyle>,
) {
    // Handle `inherit` / `initial` for any property.
    if decl.value.len() == 1 {
        match &decl.value[0] {
            CssValue::Inherit => {
                if let Some(ps) = parent_style {
                    apply_inherit(style, &decl.name, ps);
                }
                return;
            }
            CssValue::Initial => {
                apply_initial(style, &decl.name);
                return;
            }
            _ => {}
        }
    }

    match decl.name.as_str() {
        "display" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.display = match kw {
                    "none" => Display::None,
                    "block" => Display::Block,
                    "inline" => Display::Inline,
                    "inline-block" => Display::InlineBlock,
                    "flex" => Display::Flex,
                    "inline-flex" => Display::InlineFlex,
                    "grid" => Display::Grid,
                    "inline-grid" => Display::InlineGrid,
                    _ => style.display,
                };
            } else if matches!(decl.value.first(), Some(CssValue::None)) {
                style.display = Display::None;
            }
        }

        "position" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.position = match kw {
                    "static" => Position::Static,
                    "relative" => Position::Relative,
                    "absolute" => Position::Absolute,
                    "fixed" => Position::Fixed,
                    "sticky" => Position::Sticky,
                    _ => style.position,
                };
            }
        }

        "float" => {
            if let Some(kw) = first_keyword_or_none(&decl.value) {
                style.float = match kw {
                    "none" => Float::None,
                    "left" => Float::Left,
                    "right" => Float::Right,
                    _ => style.float,
                };
            }
        }

        "color" => {
            if let Some(c) = first_color(&decl.value) {
                style.color = c;
            }
        }

        "background-color" => {
            if let Some(c) = first_color(&decl.value) {
                style.background_color = c;
            }
        }

        "background" => {
            if let Some(c) = first_color(&decl.value) {
                style.background_color = c;
            }
        }

        "font-size" => {
            if let Some(px) = first_length_px(&decl.value, style.font_size_px) {
                style.font_size_px = px;
                // Update line-height proportionally (1.2 * font-size).
                style.line_height_px = px * 1.2;
            }
        }

        "font-weight" => {
            if let Some(w) = resolve_font_weight(&decl.value) {
                style.font_weight = w;
            }
        }

        "font-family" => {
            if let Some(fam) = first_string_or_keyword(&decl.value) {
                style.font_family = fam;
            }
        }

        "line-height" => {
            if let Some(v) = &decl.value.first() {
                match v {
                    CssValue::Number(n) => {
                        style.line_height_px = *n as f32 * style.font_size_px;
                    }
                    CssValue::Length(val, unit) => {
                        style.line_height_px = resolve_length(*val, unit, style.font_size_px);
                    }
                    CssValue::Percentage(p) => {
                        style.line_height_px = (*p as f32 / 100.0) * style.font_size_px;
                    }
                    _ => {}
                }
            }
        }

        "text-align" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.text_align = match kw {
                    "left" => TextAlign::Left,
                    "right" => TextAlign::Right,
                    "center" => TextAlign::Center,
                    "justify" => TextAlign::Justify,
                    _ => style.text_align,
                };
            }
        }

        "margin" => apply_edge_shorthand(&decl.value, &mut style.margin, style.font_size_px),
        "margin-top" => {
            if let Some(v) = first_length_or_auto(&decl.value, style.font_size_px) {
                style.margin.top = v;
            }
        }
        "margin-right" => {
            if let Some(v) = first_length_or_auto(&decl.value, style.font_size_px) {
                style.margin.right = v;
            }
        }
        "margin-bottom" => {
            if let Some(v) = first_length_or_auto(&decl.value, style.font_size_px) {
                style.margin.bottom = v;
            }
        }
        "margin-left" => {
            if let Some(v) = first_length_or_auto(&decl.value, style.font_size_px) {
                style.margin.left = v;
            }
        }

        "padding" => apply_edge_shorthand(&decl.value, &mut style.padding, style.font_size_px),
        "padding-top" => {
            if let Some(v) = first_length_px(&decl.value, style.font_size_px) {
                style.padding.top = v;
            }
        }
        "padding-right" => {
            if let Some(v) = first_length_px(&decl.value, style.font_size_px) {
                style.padding.right = v;
            }
        }
        "padding-bottom" => {
            if let Some(v) = first_length_px(&decl.value, style.font_size_px) {
                style.padding.bottom = v;
            }
        }
        "padding-left" => {
            if let Some(v) = first_length_px(&decl.value, style.font_size_px) {
                style.padding.left = v;
            }
        }

        "border-width" => {
            let vals = collect_lengths(&decl.value, style.font_size_px);
            if !vals.is_empty() {
                let (t, r, b, l) = expand_shorthand_4(&vals);
                style.border.top.width = t;
                style.border.right.width = r;
                style.border.bottom.width = b;
                style.border.left.width = l;
            }
        }

        "border-style" => {
            if let Some(kw) = first_keyword(&decl.value) {
                let bs = parse_border_style(kw);
                style.border.top.style = bs;
                style.border.right.style = bs;
                style.border.bottom.style = bs;
                style.border.left.style = bs;
            }
        }

        "border-color" => {
            if let Some(c) = first_color(&decl.value) {
                style.border.top.color = c;
                style.border.right.color = c;
                style.border.bottom.color = c;
                style.border.left.color = c;
            }
        }

        "border" => {
            // Shorthand: width style color
            for v in &decl.value {
                match v {
                    CssValue::Length(val, unit) => {
                        let px = resolve_length(*val, unit, style.font_size_px);
                        style.border.top.width = px;
                        style.border.right.width = px;
                        style.border.bottom.width = px;
                        style.border.left.width = px;
                    }
                    CssValue::Number(n) => {
                        let px = *n as f32;
                        style.border.top.width = px;
                        style.border.right.width = px;
                        style.border.bottom.width = px;
                        style.border.left.width = px;
                    }
                    CssValue::Keyword(kw) => {
                        let bs = parse_border_style(kw);
                        if bs != BorderStyle::None {
                            style.border.top.style = bs;
                            style.border.right.style = bs;
                            style.border.bottom.style = bs;
                            style.border.left.style = bs;
                        }
                    }
                    CssValue::Color(c) => {
                        let color = css_color_to_color(c);
                        style.border.top.color = color;
                        style.border.right.color = color;
                        style.border.bottom.color = color;
                        style.border.left.color = color;
                    }
                    _ => {}
                }
            }
        }

        "width" => style.width = first_length_or_none(&decl.value, style.font_size_px),
        "height" => style.height = first_length_or_none(&decl.value, style.font_size_px),
        "min-width" => style.min_width = first_length_or_none(&decl.value, style.font_size_px),
        "min-height" => style.min_height = first_length_or_none(&decl.value, style.font_size_px),
        "max-width" => style.max_width = first_length_or_none(&decl.value, style.font_size_px),
        "max-height" => style.max_height = first_length_or_none(&decl.value, style.font_size_px),

        "flex-direction" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.flex.direction = match kw {
                    "row" => FlexDirection::Row,
                    "row-reverse" => FlexDirection::RowReverse,
                    "column" => FlexDirection::Column,
                    "column-reverse" => FlexDirection::ColumnReverse,
                    _ => style.flex.direction,
                };
            }
        }

        "flex-wrap" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.flex.wrap = match kw {
                    "nowrap" => FlexWrap::NoWrap,
                    "wrap" => FlexWrap::Wrap,
                    "wrap-reverse" => FlexWrap::WrapReverse,
                    _ => style.flex.wrap,
                };
            }
        }

        "justify-content" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.flex.justify_content = match kw {
                    "flex-start" => JustifyContent::FlexStart,
                    "flex-end" => JustifyContent::FlexEnd,
                    "center" => JustifyContent::Center,
                    "space-between" => JustifyContent::SpaceBetween,
                    "space-around" => JustifyContent::SpaceAround,
                    "space-evenly" => JustifyContent::SpaceEvenly,
                    _ => style.flex.justify_content,
                };
            }
        }

        "align-items" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.flex.align_items = match kw {
                    "stretch" => AlignItems::Stretch,
                    "flex-start" => AlignItems::FlexStart,
                    "flex-end" => AlignItems::FlexEnd,
                    "center" => AlignItems::Center,
                    "baseline" => AlignItems::Baseline,
                    _ => style.flex.align_items,
                };
            }
        }

        "flex-grow" => {
            if let Some(n) = first_number(&decl.value) {
                style.flex.grow = n;
            }
        }

        "flex-shrink" => {
            if let Some(n) = first_number(&decl.value) {
                style.flex.shrink = n;
            }
        }

        "flex-basis" => {
            style.flex.basis = first_length_or_none(&decl.value, style.font_size_px);
        }

        "z-index" => {
            if let Some(CssValue::Number(n)) = decl.value.first() {
                style.z_index = Some(*n as i32);
            } else if matches!(decl.value.first(), Some(CssValue::Auto)) {
                style.z_index = None;
            }
        }

        "overflow" => {
            if let Some(kw) = first_keyword(&decl.value) {
                let ov = parse_overflow(kw);
                style.overflow_x = ov;
                style.overflow_y = ov;
            }
        }
        "overflow-x" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.overflow_x = parse_overflow(kw);
            }
        }
        "overflow-y" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.overflow_y = parse_overflow(kw);
            }
        }

        "opacity" => {
            if let Some(n) = first_number(&decl.value) {
                style.opacity = n.clamp(0.0, 1.0);
            }
        }

        "visibility" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.visibility = match kw {
                    "visible" => Visibility::Visible,
                    "hidden" => Visibility::Hidden,
                    "collapse" => Visibility::Collapse,
                    _ => style.visibility,
                };
            }
        }

        "box-sizing" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.box_sizing = match kw {
                    "content-box" => BoxSizing::ContentBox,
                    "border-box" => BoxSizing::BorderBox,
                    _ => style.box_sizing,
                };
            }
        }

        "text-decoration" | "text-decoration-line" => {
            if let Some(kw) = first_keyword_or_none(&decl.value) {
                style.text_decoration = match kw {
                    "none" => TextDecoration::None,
                    "underline" => TextDecoration::Underline,
                    "overline" => TextDecoration::Overline,
                    "line-through" => TextDecoration::LineThrough,
                    _ => style.text_decoration,
                };
            }
        }

        "font-style" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.font_style = match kw {
                    "normal" => FontStyle::Normal,
                    "italic" => FontStyle::Italic,
                    "oblique" => FontStyle::Oblique,
                    _ => style.font_style,
                };
            }
        }

        "white-space" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.white_space = match kw {
                    "normal" => WhiteSpace::Normal,
                    "nowrap" => WhiteSpace::NoWrap,
                    "pre" => WhiteSpace::Pre,
                    "pre-wrap" => WhiteSpace::PreWrap,
                    "pre-line" => WhiteSpace::PreLine,
                    _ => style.white_space,
                };
            }
        }

        "text-transform" => {
            if let Some(kw) = first_keyword_or_none(&decl.value) {
                style.text_transform = match kw {
                    "none" => TextTransform::None,
                    "uppercase" => TextTransform::Uppercase,
                    "lowercase" => TextTransform::Lowercase,
                    "capitalize" => TextTransform::Capitalize,
                    _ => style.text_transform,
                };
            }
        }

        "letter-spacing" => {
            if let Some(px) = first_length_px(&decl.value, style.font_size_px) {
                style.letter_spacing = px;
            } else if matches!(decl.value.first(), Some(CssValue::Keyword(k)) if k == "normal") {
                style.letter_spacing = 0.0;
            }
        }

        "word-spacing" => {
            if let Some(px) = first_length_px(&decl.value, style.font_size_px) {
                style.word_spacing = px;
            } else if matches!(decl.value.first(), Some(CssValue::Keyword(k)) if k == "normal") {
                style.word_spacing = 0.0;
            }
        }

        "vertical-align" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.vertical_align = match kw {
                    "baseline" => VerticalAlign::Baseline,
                    "top" => VerticalAlign::Top,
                    "middle" => VerticalAlign::Middle,
                    "bottom" => VerticalAlign::Bottom,
                    "text-top" => VerticalAlign::TextTop,
                    "text-bottom" => VerticalAlign::TextBottom,
                    "sub" => VerticalAlign::Sub,
                    "super" => VerticalAlign::Super,
                    _ => style.vertical_align,
                };
            }
        }

        "text-indent" => {
            if let Some(px) = first_length_px(&decl.value, style.font_size_px) {
                style.text_indent = px;
            }
        }

        "text-overflow" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.text_overflow = match kw {
                    "clip" => TextOverflow::Clip,
                    "ellipsis" => TextOverflow::Ellipsis,
                    _ => style.text_overflow,
                };
            }
        }

        "list-style-type" | "list-style" => {
            if let Some(kw) = first_keyword_or_none(&decl.value) {
                style.list_style_type = match kw {
                    "none" => ListStyleType::None,
                    "disc" => ListStyleType::Disc,
                    "circle" => ListStyleType::Circle,
                    "square" => ListStyleType::Square,
                    "decimal" => ListStyleType::Decimal,
                    _ => style.list_style_type,
                };
            }
        }

        "border-radius" => {
            let vals = collect_lengths(&decl.value, style.font_size_px);
            match vals.len() {
                1 => style.border_radius = [vals[0]; 4],
                2 => style.border_radius = [vals[0], vals[1], vals[0], vals[1]],
                3 => style.border_radius = [vals[0], vals[1], vals[2], vals[1]],
                4 => style.border_radius = [vals[0], vals[1], vals[2], vals[3]],
                _ => {}
            }
        }
        "border-top-left-radius" => {
            if let Some(v) = first_length_px(&decl.value, style.font_size_px) {
                style.border_radius[0] = v;
            }
        }
        "border-top-right-radius" => {
            if let Some(v) = first_length_px(&decl.value, style.font_size_px) {
                style.border_radius[1] = v;
            }
        }
        "border-bottom-right-radius" => {
            if let Some(v) = first_length_px(&decl.value, style.font_size_px) {
                style.border_radius[2] = v;
            }
        }
        "border-bottom-left-radius" => {
            if let Some(v) = first_length_px(&decl.value, style.font_size_px) {
                style.border_radius[3] = v;
            }
        }

        "top" => style.top = first_length_or_none(&decl.value, style.font_size_px),
        "right" => style.right = first_length_or_none(&decl.value, style.font_size_px),
        "bottom" => style.bottom = first_length_or_none(&decl.value, style.font_size_px),
        "left" => style.left = first_length_or_none(&decl.value, style.font_size_px),

        "border-top-width" => {
            if let Some(v) = first_length_px(&decl.value, style.font_size_px) {
                style.border.top.width = v;
            }
        }
        "border-right-width" => {
            if let Some(v) = first_length_px(&decl.value, style.font_size_px) {
                style.border.right.width = v;
            }
        }
        "border-bottom-width" => {
            if let Some(v) = first_length_px(&decl.value, style.font_size_px) {
                style.border.bottom.width = v;
            }
        }
        "border-left-width" => {
            if let Some(v) = first_length_px(&decl.value, style.font_size_px) {
                style.border.left.width = v;
            }
        }

        "border-top-style" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.border.top.style = parse_border_style(kw);
            }
        }
        "border-right-style" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.border.right.style = parse_border_style(kw);
            }
        }
        "border-bottom-style" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.border.bottom.style = parse_border_style(kw);
            }
        }
        "border-left-style" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.border.left.style = parse_border_style(kw);
            }
        }

        "border-top-color" => {
            if let Some(c) = first_color(&decl.value) {
                style.border.top.color = c;
            }
        }
        "border-right-color" => {
            if let Some(c) = first_color(&decl.value) {
                style.border.right.color = c;
            }
        }
        "border-bottom-color" => {
            if let Some(c) = first_color(&decl.value) {
                style.border.bottom.color = c;
            }
        }
        "border-left-color" => {
            if let Some(c) = first_color(&decl.value) {
                style.border.left.color = c;
            }
        }

        "border-top" => apply_border_side_shorthand(&decl.value, &mut style.border.top, style.font_size_px),
        "border-right" => apply_border_side_shorthand(&decl.value, &mut style.border.right, style.font_size_px),
        "border-bottom" => apply_border_side_shorthand(&decl.value, &mut style.border.bottom, style.font_size_px),
        "border-left" => apply_border_side_shorthand(&decl.value, &mut style.border.left, style.font_size_px),

        "align-self" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.align_self = match kw {
                    "auto" => AlignSelf::Auto,
                    "flex-start" => AlignSelf::FlexStart,
                    "flex-end" => AlignSelf::FlexEnd,
                    "center" => AlignSelf::Center,
                    "baseline" => AlignSelf::Baseline,
                    "stretch" => AlignSelf::Stretch,
                    _ => style.align_self,
                };
            }
        }

        "align-content" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.align_content = match kw {
                    "flex-start" => AlignContent::FlexStart,
                    "flex-end" => AlignContent::FlexEnd,
                    "center" => AlignContent::Center,
                    "space-between" => AlignContent::SpaceBetween,
                    "space-around" => AlignContent::SpaceAround,
                    "stretch" => AlignContent::Stretch,
                    _ => style.align_content,
                };
            }
        }

        "gap" | "grid-gap" => {
            if let Some(px) = first_length_px(&decl.value, style.font_size_px) {
                style.gap = px;
                style.grid.column_gap = px;
                style.grid.row_gap = px;
            }
        }
        "row-gap" | "grid-row-gap" => {
            if let Some(px) = first_length_px(&decl.value, style.font_size_px) {
                style.grid.row_gap = px;
            }
        }
        "column-gap" | "grid-column-gap" => {
            if let Some(px) = first_length_px(&decl.value, style.font_size_px) {
                style.grid.column_gap = px;
            }
        }

        "flex" => {
            // flex shorthand: <grow> [<shrink>] [<basis>]
            let mut nums = Vec::new();
            for v in &decl.value {
                match v {
                    CssValue::Number(n) => nums.push(*n as f32),
                    CssValue::Length(val, unit) => {
                        style.flex.basis = Some(resolve_length(*val, unit, style.font_size_px));
                    }
                    CssValue::Keyword(k) if k == "auto" => { style.flex.basis = None; }
                    CssValue::None => {
                        style.flex.grow = 0.0;
                        style.flex.shrink = 0.0;
                        style.flex.basis = None;
                    }
                    _ => {}
                }
            }
            if nums.len() >= 1 {
                style.flex.grow = nums[0];
            }
            if nums.len() >= 2 {
                style.flex.shrink = nums[1];
            }
        }

        "flex-flow" => {
            for v in &decl.value {
                if let CssValue::Keyword(kw) = v {
                    match kw.as_str() {
                        "row" => style.flex.direction = FlexDirection::Row,
                        "row-reverse" => style.flex.direction = FlexDirection::RowReverse,
                        "column" => style.flex.direction = FlexDirection::Column,
                        "column-reverse" => style.flex.direction = FlexDirection::ColumnReverse,
                        "nowrap" => style.flex.wrap = FlexWrap::NoWrap,
                        "wrap" => style.flex.wrap = FlexWrap::Wrap,
                        "wrap-reverse" => style.flex.wrap = FlexWrap::WrapReverse,
                        _ => {}
                    }
                }
            }
        }

        "font" => {
            // Simplified font shorthand: just look for size and weight
            for v in &decl.value {
                match v {
                    CssValue::Length(val, unit) => {
                        let px = resolve_length(*val, unit, style.font_size_px);
                        style.font_size_px = px;
                        style.line_height_px = px * 1.2;
                    }
                    CssValue::Number(n) => {
                        let n = *n as u16;
                        if n >= 100 && n <= 900 {
                            style.font_weight = n;
                        }
                    }
                    CssValue::Keyword(kw) => match kw.as_str() {
                        "bold" => style.font_weight = 700,
                        "normal" => style.font_weight = 400,
                        "italic" => style.font_style = FontStyle::Italic,
                        _ => {}
                    },
                    CssValue::String(s) => {
                        style.font_family = s.clone();
                    }
                    _ => {}
                }
            }
        }

        "cursor" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.cursor = match kw {
                    "auto" => Cursor::Auto,
                    "default" => Cursor::Default,
                    "pointer" => Cursor::Pointer,
                    "text" => Cursor::Text,
                    "move" => Cursor::Move,
                    "not-allowed" => Cursor::NotAllowed,
                    "crosshair" => Cursor::Crosshair,
                    "wait" => Cursor::Wait,
                    _ => style.cursor,
                };
            }
        }

        _ => {
            // Unknown property — ignore.
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Inherit / Initial helpers
// ─────────────────────────────────────────────────────────────────────────────

fn apply_inherit(style: &mut ComputedStyle, prop: &str, parent: &ComputedStyle) {
    match prop {
        "color" => style.color = parent.color,
        "font-size" => style.font_size_px = parent.font_size_px,
        "font-weight" => style.font_weight = parent.font_weight,
        "font-family" => style.font_family = parent.font_family.clone(),
        "font-style" => style.font_style = parent.font_style,
        "line-height" => style.line_height_px = parent.line_height_px,
        "text-align" => style.text_align = parent.text_align,
        "text-transform" => style.text_transform = parent.text_transform,
        "text-indent" => style.text_indent = parent.text_indent,
        "letter-spacing" => style.letter_spacing = parent.letter_spacing,
        "word-spacing" => style.word_spacing = parent.word_spacing,
        "white-space" => style.white_space = parent.white_space,
        "visibility" => style.visibility = parent.visibility,
        "cursor" => style.cursor = parent.cursor,
        "list-style-type" | "list-style" => style.list_style_type = parent.list_style_type,
        "display" => style.display = parent.display,
        "opacity" => style.opacity = parent.opacity,
        _ => {}
    }
}

fn apply_initial(style: &mut ComputedStyle, prop: &str) {
    let def = ComputedStyle::default();
    match prop {
        "display" => style.display = def.display,
        "position" => style.position = def.position,
        "float" => style.float = def.float,
        "color" => style.color = def.color,
        "background" | "background-color" => style.background_color = def.background_color,
        "font-size" => {
            style.font_size_px = def.font_size_px;
            style.line_height_px = def.line_height_px;
        }
        "font-weight" => style.font_weight = def.font_weight,
        "font-family" => style.font_family = def.font_family,
        "font-style" => style.font_style = def.font_style,
        "line-height" => style.line_height_px = def.line_height_px,
        "text-align" => style.text_align = def.text_align,
        "text-decoration" | "text-decoration-line" => style.text_decoration = def.text_decoration,
        "text-transform" => style.text_transform = def.text_transform,
        "text-indent" => style.text_indent = def.text_indent,
        "text-overflow" => style.text_overflow = def.text_overflow,
        "letter-spacing" => style.letter_spacing = def.letter_spacing,
        "word-spacing" => style.word_spacing = def.word_spacing,
        "white-space" => style.white_space = def.white_space,
        "vertical-align" => style.vertical_align = def.vertical_align,
        "visibility" => style.visibility = def.visibility,
        "box-sizing" => style.box_sizing = def.box_sizing,
        "margin" => style.margin = def.margin,
        "padding" => style.padding = def.padding,
        "border-radius" => style.border_radius = def.border_radius,
        "opacity" => style.opacity = def.opacity,
        "z-index" => style.z_index = def.z_index,
        "cursor" => style.cursor = def.cursor,
        "list-style-type" | "list-style" => style.list_style_type = def.list_style_type,
        _ => {}
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Value extraction helpers
// ─────────────────────────────────────────────────────────────────────────────

fn first_keyword(values: &[CssValue]) -> Option<&str> {
    for v in values {
        if let CssValue::Keyword(kw) = v {
            return Some(kw.as_str());
        }
    }
    None
}

fn first_keyword_or_none(values: &[CssValue]) -> Option<&str> {
    for v in values {
        match v {
            CssValue::Keyword(kw) => return Some(kw.as_str()),
            CssValue::None => return Some("none"),
            _ => {}
        }
    }
    None
}

fn first_color(values: &[CssValue]) -> Option<Color> {
    for v in values {
        if let CssValue::Color(c) = v {
            return Some(css_color_to_color(c));
        }
    }
    None
}

fn css_color_to_color(c: &CssColor) -> Color {
    Color::rgba(c.r, c.g, c.b, c.a)
}

fn first_number(values: &[CssValue]) -> Option<f32> {
    for v in values {
        match v {
            CssValue::Number(n) => return Some(*n as f32),
            CssValue::Percentage(p) => return Some(*p as f32 / 100.0),
            _ => {}
        }
    }
    None
}

fn resolve_length(value: f64, unit: &LengthUnit, parent_font_size: f32) -> f32 {
    match unit {
        LengthUnit::Px => value as f32,
        LengthUnit::Em => value as f32 * parent_font_size,
        LengthUnit::Rem => value as f32 * 16.0, // root font size default
        LengthUnit::Vw => value as f32 * 10.0,  // simplified; needs viewport
        LengthUnit::Vh => value as f32 * 10.0,
        LengthUnit::Percent => value as f32, // caller must handle percentage context
    }
}

fn first_length_px(values: &[CssValue], parent_font_size: f32) -> Option<f32> {
    for v in values {
        match v {
            CssValue::Length(val, unit) => {
                return Some(resolve_length(*val, unit, parent_font_size));
            }
            CssValue::Number(n) if *n == 0.0 => return Some(0.0),
            CssValue::Percentage(p) => return Some(*p as f32),
            _ => {}
        }
    }
    None
}

fn first_length_or_auto(values: &[CssValue], parent_font_size: f32) -> Option<f32> {
    for v in values {
        match v {
            CssValue::Auto => return Some(0.0),
            CssValue::Length(val, unit) => {
                return Some(resolve_length(*val, unit, parent_font_size));
            }
            CssValue::Number(n) if *n == 0.0 => return Some(0.0),
            CssValue::Percentage(p) => return Some(*p as f32),
            _ => {}
        }
    }
    None
}

fn first_length_or_none(values: &[CssValue], parent_font_size: f32) -> Option<f32> {
    for v in values {
        match v {
            CssValue::Auto | CssValue::None => return None,
            CssValue::Length(val, unit) => {
                return Some(resolve_length(*val, unit, parent_font_size));
            }
            CssValue::Number(n) if *n == 0.0 => return Some(0.0),
            CssValue::Percentage(p) => return Some(*p as f32),
            _ => {}
        }
    }
    None
}

fn first_string_or_keyword(values: &[CssValue]) -> Option<String> {
    for v in values {
        match v {
            CssValue::String(s) => return Some(s.clone()),
            CssValue::Keyword(kw) => return Some(kw.clone()),
            _ => {}
        }
    }
    None
}

fn collect_lengths(values: &[CssValue], parent_font_size: f32) -> Vec<f32> {
    let mut result = Vec::new();
    for v in values {
        match v {
            CssValue::Length(val, unit) => {
                result.push(resolve_length(*val, unit, parent_font_size));
            }
            CssValue::Number(n) if *n == 0.0 => result.push(0.0),
            _ => {}
        }
    }
    result
}

fn resolve_font_weight(values: &[CssValue]) -> Option<u16> {
    for v in values {
        match v {
            CssValue::Number(n) => return Some((*n as u16).clamp(1, 1000)),
            CssValue::Keyword(kw) => {
                return match kw.as_str() {
                    "normal" => Some(400),
                    "bold" => Some(700),
                    "lighter" => Some(100),
                    "bolder" => Some(900),
                    _ => None,
                };
            }
            _ => {}
        }
    }
    None
}

fn parse_border_style(kw: &str) -> BorderStyle {
    match kw {
        "none" => BorderStyle::None,
        "solid" => BorderStyle::Solid,
        "dotted" => BorderStyle::Dotted,
        "dashed" => BorderStyle::Dashed,
        "double" => BorderStyle::Double,
        "groove" => BorderStyle::Groove,
        "ridge" => BorderStyle::Ridge,
        "inset" => BorderStyle::Inset,
        "outset" => BorderStyle::Outset,
        _ => BorderStyle::None,
    }
}

fn parse_overflow(kw: &str) -> Overflow {
    match kw {
        "visible" => Overflow::Visible,
        "hidden" => Overflow::Hidden,
        "scroll" => Overflow::Scroll,
        "auto" => Overflow::Auto,
        _ => Overflow::Visible,
    }
}

/// Expand 1-4 values into (top, right, bottom, left) per CSS shorthand rules.
fn expand_shorthand_4(vals: &[f32]) -> (f32, f32, f32, f32) {
    match vals.len() {
        1 => (vals[0], vals[0], vals[0], vals[0]),
        2 => (vals[0], vals[1], vals[0], vals[1]),
        3 => (vals[0], vals[1], vals[2], vals[1]),
        4 => (vals[0], vals[1], vals[2], vals[3]),
        _ => (0.0, 0.0, 0.0, 0.0),
    }
}

fn apply_edge_shorthand(values: &[CssValue], edges: &mut Edges<f32>, parent_font_size: f32) {
    let vals = collect_lengths(values, parent_font_size);
    if !vals.is_empty() {
        let (t, r, b, l) = expand_shorthand_4(&vals);
        edges.top = t;
        edges.right = r;
        edges.bottom = b;
        edges.left = l;
    }
}

fn apply_border_side_shorthand(values: &[CssValue], side: &mut BorderSide, parent_font_size: f32) {
    for v in values {
        match v {
            CssValue::Length(val, unit) => {
                side.width = resolve_length(*val, unit, parent_font_size);
            }
            CssValue::Number(n) if *n == 0.0 => {
                side.width = 0.0;
            }
            CssValue::Keyword(kw) => {
                let bs = parse_border_style(kw);
                if bs != BorderStyle::None || kw == "none" {
                    side.style = bs;
                }
            }
            CssValue::Color(c) => {
                side.color = css_color_to_color(c);
            }
            CssValue::None => {
                side.style = BorderStyle::None;
                side.width = 0.0;
            }
            _ => {}
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use css::parse_stylesheet;
    use dom::{Attr, Namespace};

    fn build_dom_and_style(
        css_str: &str,
    ) -> (Dom, NodeId, NodeId) {
        let mut dom = Dom::new();
        let doc = dom.create_document();
        let html = dom.create_html_element("html");
        let body = dom.create_html_element("body");
        let div = dom.create_element(
            "div",
            Namespace::Html,
            vec![
                Attr { name: "id".into(), value: "main".into() },
                Attr { name: "class".into(), value: "container".into() },
            ],
        );
        let p = dom.create_html_element("p");

        dom.append_child(doc, html);
        dom.append_child(html, body);
        dom.append_child(body, div);
        dom.append_child(div, p);

        let _ = css_str; // used by callers
        (dom, div, p)
    }

    #[test]
    fn resolve_display_block() {
        let (dom, div, _) = build_dom_and_style("");
        let ss = parse_stylesheet("div { display: block; }");
        let sheets = vec![(ss, StyleOrigin::Author)];
        let matched = collect_matching_rules(&dom, div, &sheets);
        let style = resolve_style(&dom, div, &matched, None);
        assert_eq!(style.display, Display::Block);
    }

    #[test]
    fn resolve_color_and_font_size() {
        let (dom, div, _) = build_dom_and_style("");
        let ss = parse_stylesheet("div { color: red; font-size: 20px; }");
        let sheets = vec![(ss, StyleOrigin::Author)];
        let matched = collect_matching_rules(&dom, div, &sheets);
        let style = resolve_style(&dom, div, &matched, None);
        assert_eq!(style.color, Color::rgb(255, 0, 0));
        assert_eq!(style.font_size_px, 20.0);
    }

    #[test]
    fn resolve_margin_shorthand() {
        let (dom, div, _) = build_dom_and_style("");
        let ss = parse_stylesheet("div { margin: 10px 20px; }");
        let sheets = vec![(ss, StyleOrigin::Author)];
        let matched = collect_matching_rules(&dom, div, &sheets);
        let style = resolve_style(&dom, div, &matched, None);
        assert_eq!(style.margin.top, 10.0);
        assert_eq!(style.margin.right, 20.0);
        assert_eq!(style.margin.bottom, 10.0);
        assert_eq!(style.margin.left, 20.0);
    }

    #[test]
    fn inheritance_color() {
        let (dom, div, p) = build_dom_and_style("");
        let ss = parse_stylesheet("div { color: blue; }");
        let sheets = vec![(ss, StyleOrigin::Author)];

        let div_matched = collect_matching_rules(&dom, div, &sheets);
        let div_style = resolve_style(&dom, div, &div_matched, None);
        assert_eq!(div_style.color, Color::rgb(0, 0, 255));

        let p_matched = collect_matching_rules(&dom, p, &sheets);
        let p_style = resolve_style(&dom, p, &p_matched, Some(&div_style));
        // color is inherited
        assert_eq!(p_style.color, Color::rgb(0, 0, 255));
    }

    #[test]
    fn non_inherited_not_passed() {
        let (dom, div, p) = build_dom_and_style("");
        let ss = parse_stylesheet("div { margin: 10px; background-color: red; }");
        let sheets = vec![(ss, StyleOrigin::Author)];

        let div_matched = collect_matching_rules(&dom, div, &sheets);
        let div_style = resolve_style(&dom, div, &div_matched, None);

        let p_matched = collect_matching_rules(&dom, p, &sheets);
        let p_style = resolve_style(&dom, p, &p_matched, Some(&div_style));
        // margin and background-color are NOT inherited
        assert_eq!(p_style.margin.top, 0.0);
        assert_eq!(p_style.background_color, Color::TRANSPARENT);
    }

    #[test]
    fn important_overrides_normal() {
        let (dom, div, _) = build_dom_and_style("");
        let ss = parse_stylesheet(
            "div { color: red; } div { color: blue !important; } #main { color: green; }",
        );
        let sheets = vec![(ss, StyleOrigin::Author)];
        let matched = collect_matching_rules(&dom, div, &sheets);
        let style = resolve_style(&dom, div, &matched, None);
        // !important should win even though #main has higher specificity
        assert_eq!(style.color, Color::rgb(0, 0, 255));
    }

    #[test]
    fn specificity_ordering() {
        let (dom, div, _) = build_dom_and_style("");
        let ss = parse_stylesheet("div { color: red; } #main { color: blue; }");
        let sheets = vec![(ss, StyleOrigin::Author)];
        let matched = collect_matching_rules(&dom, div, &sheets);
        let style = resolve_style(&dom, div, &matched, None);
        // #main has higher specificity than div
        assert_eq!(style.color, Color::rgb(0, 0, 255));
    }

    #[test]
    fn source_order_tiebreak() {
        let (dom, div, _) = build_dom_and_style("");
        let ss = parse_stylesheet("div { color: red; } div { color: blue; }");
        let sheets = vec![(ss, StyleOrigin::Author)];
        let matched = collect_matching_rules(&dom, div, &sheets);
        let style = resolve_style(&dom, div, &matched, None);
        // Later rule wins at same specificity
        assert_eq!(style.color, Color::rgb(0, 0, 255));
    }

    #[test]
    fn resolve_opacity() {
        let (dom, div, _) = build_dom_and_style("");
        let ss = parse_stylesheet("div { opacity: 0.5; }");
        let sheets = vec![(ss, StyleOrigin::Author)];
        let matched = collect_matching_rules(&dom, div, &sheets);
        let style = resolve_style(&dom, div, &matched, None);
        assert!((style.opacity - 0.5).abs() < 0.01);
    }

    #[test]
    fn resolve_flex_properties() {
        let (dom, div, _) = build_dom_and_style("");
        let ss = parse_stylesheet(
            "div { display: flex; flex-direction: column; justify-content: center; }",
        );
        let sheets = vec![(ss, StyleOrigin::Author)];
        let matched = collect_matching_rules(&dom, div, &sheets);
        let style = resolve_style(&dom, div, &matched, None);
        assert_eq!(style.display, Display::Flex);
        assert_eq!(style.flex.direction, FlexDirection::Column);
        assert_eq!(style.flex.justify_content, JustifyContent::Center);
    }

    #[test]
    fn expand_shorthand_1() {
        assert_eq!(expand_shorthand_4(&[5.0]), (5.0, 5.0, 5.0, 5.0));
    }

    #[test]
    fn expand_shorthand_2() {
        assert_eq!(expand_shorthand_4(&[5.0, 10.0]), (5.0, 10.0, 5.0, 10.0));
    }

    #[test]
    fn expand_shorthand_3() {
        assert_eq!(
            expand_shorthand_4(&[5.0, 10.0, 15.0]),
            (5.0, 10.0, 15.0, 10.0)
        );
    }

    #[test]
    fn expand_shorthand_4_values() {
        assert_eq!(
            expand_shorthand_4(&[1.0, 2.0, 3.0, 4.0]),
            (1.0, 2.0, 3.0, 4.0)
        );
    }
}
