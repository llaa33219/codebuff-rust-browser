//! Cascade resolution — collect matching rules, sort, and resolve computed values.
//!
//! Implements the CSS cascade: importance → origin → specificity → source order.
//! Handles inheritance of inheritable properties (color, font-*, text-align, line-height).

use std::collections::HashMap;

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
// ResolveContext — viewport dimensions + custom properties for var() resolution
// ─────────────────────────────────────────────────────────────────────────────

/// Context for resolving CSS functions like `var()` and viewport units.
pub struct ResolveContext {
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub custom_properties: HashMap<String, Vec<CssValue>>,
}

impl ResolveContext {
    pub fn new(vw: f32, vh: f32) -> Self {
        Self {
            viewport_width: vw,
            viewport_height: vh,
            custom_properties: HashMap::new(),
        }
    }
}

/// Pre-resolve CSS functions (`var()`, viewport units, `calc()`) in a value list.
pub fn resolve_css_values(values: &[CssValue], ctx: &ResolveContext) -> Vec<CssValue> {
    let mut result = Vec::new();
    for v in values {
        match v {
            CssValue::Function { name, args } => {
                let lower = name.to_ascii_lowercase();
                if lower == "var" {
                    let var_name = args.iter().find_map(|a| {
                        if let CssValue::Keyword(k) = a {
                            if k.starts_with("--") { return Some(k.clone()); }
                        }
                        None
                    });
                    if let Some(ref var_name) = var_name {
                        if let Some(val) = ctx.custom_properties.get(var_name) {
                            result.extend(resolve_css_values(val, ctx));
                            continue;
                        }
                    }
                    // Fallback: everything after the --name keyword
                    let mut found_name = false;
                    let mut fallback = Vec::new();
                    for a in args {
                        if found_name {
                            fallback.push(a.clone());
                        } else if let CssValue::Keyword(k) = a {
                            if k.starts_with("--") {
                                found_name = true;
                            }
                        }
                    }
                    if !fallback.is_empty() {
                        result.extend(resolve_css_values(&fallback, ctx));
                    }
                } else if lower == "calc" || lower == "-webkit-calc" {
                    // Keep calc() as a Function with resolved inner args
                    // (var/viewport resolved). Actual evaluation happens later
                    // in resolve_property_percentages / resolve_remaining_calcs
                    // so that percentages can be resolved against the correct base.
                    let resolved_args = resolve_css_values(args, ctx);
                    result.push(CssValue::Function {
                        name: name.clone(),
                        args: resolved_args,
                    });
                } else {
                    // Other functions (rgb, etc.) — keep as-is
                    result.push(v.clone());
                }
            }
            CssValue::Length(val, unit) => match unit {
                LengthUnit::Vw => result.push(CssValue::Length(
                    *val * ctx.viewport_width as f64 / 100.0,
                    LengthUnit::Px,
                )),
                LengthUnit::Vh => result.push(CssValue::Length(
                    *val * ctx.viewport_height as f64 / 100.0,
                    LengthUnit::Px,
                )),
                LengthUnit::Vmin => result.push(CssValue::Length(
                    *val * (ctx.viewport_width.min(ctx.viewport_height)) as f64 / 100.0,
                    LengthUnit::Px,
                )),
                LengthUnit::Vmax => result.push(CssValue::Length(
                    *val * (ctx.viewport_width.max(ctx.viewport_height)) as f64 / 100.0,
                    LengthUnit::Px,
                )),
                _ => result.push(v.clone()),
            },
            _ => result.push(v.clone()),
        }
    }
    result
}

fn eval_simple_calc(args: &[CssValue], ctx: &ResolveContext, percent_base: f32) -> Option<f32> {
    let resolved = resolve_css_values(args, ctx);
    if resolved.is_empty() {
        return None;
    }
    let mut total: f32 = 0.0;
    let mut op = '+';
    for arg in &resolved {
        // Detect arithmetic operator keywords (+, -, *, /) produced by the
        // CSS tokenizer for delimiter characters inside calc().
        if let CssValue::Keyword(k) = arg {
            match k.as_str() {
                "+" => { op = '+'; }
                "-" => { op = '-'; }
                "*" => { op = '*'; }
                "/" => { op = '/'; }
                _ => {}
            }
            continue;
        }
        let val = match arg {
            CssValue::Length(v, unit) => Some(match unit {
                LengthUnit::Px => *v as f32,
                LengthUnit::Em => *v as f32 * 16.0,
                LengthUnit::Rem => *v as f32 * 16.0,
                _ => *v as f32,
            }),
            CssValue::Number(n) => Some(*n as f32),
            CssValue::Percentage(p) => Some((*p as f32 / 100.0) * percent_base),
            _ => None,
        };
        if let Some(v) = val {
            total = match op {
                '+' => total + v,
                '-' => total - v,
                '*' => total * v,
                '/' => {
                    if v != 0.0 { total / v } else { total }
                }
                _ => total,
            };
            op = '+';
        }
    }
    Some(total)
}

/// Pre-resolve CSS percentage values for properties where the percentage base
/// is known (viewport-relative).  Must be called after `resolve_css_values`
/// but before `apply_declaration`.
pub fn resolve_property_percentages(
    name: &str,
    values: &[CssValue],
    ctx: &ResolveContext,
) -> Vec<CssValue> {
    let prop = strip_vendor_prefix(name);
    // Per CSS spec, margin/padding percentages are always relative to the
    // containing block's *width* (even for top/bottom).
    let percent_base = match prop.as_str() {
        "left" | "right"
        | "margin" | "margin-left" | "margin-right" | "margin-top" | "margin-bottom"
        | "padding" | "padding-left" | "padding-right" | "padding-top" | "padding-bottom" => {
            ctx.viewport_width
        }
        "top" | "bottom" => ctx.viewport_height,
        _ => return values.to_vec(),
    };

    values
        .iter()
        .map(|v| match v {
            CssValue::Percentage(p) => {
                CssValue::Length((*p / 100.0) * percent_base as f64, LengthUnit::Px)
            }
            CssValue::Function { name, args }
                if name.eq_ignore_ascii_case("calc")
                    || name.eq_ignore_ascii_case("-webkit-calc") =>
            {
                if let Some(px) = eval_simple_calc(args, ctx, percent_base) {
                    CssValue::Length(px as f64, LengthUnit::Px)
                } else {
                    v.clone()
                }
            }
            _ => v.clone(),
        })
        .collect()
}

/// Evaluate any remaining `calc()` functions that were not resolved by
/// `resolve_property_percentages` (i.e. for properties without a known
/// percentage base).  Uses `viewport_width` as the fallback percent base.
pub fn resolve_remaining_calcs(values: &[CssValue], ctx: &ResolveContext) -> Vec<CssValue> {
    values
        .iter()
        .map(|v| match v {
            CssValue::Function { name, args }
                if name.eq_ignore_ascii_case("calc")
                    || name.eq_ignore_ascii_case("-webkit-calc") =>
            {
                if let Some(px) = eval_simple_calc(args, ctx, ctx.viewport_width) {
                    CssValue::Length(px as f64, LengthUnit::Px)
                } else {
                    v.clone()
                }
            }
            _ => v.clone(),
        })
        .collect()
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
    ctx: &mut ResolveContext,
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

    // First pass: collect custom properties (--*) from all declarations.
    for (decl, _, _, _) in normal_decls.iter().chain(important_decls.iter()) {
        if decl.name.starts_with("--") {
            let resolved = resolve_css_values(&decl.value, ctx);
            ctx.custom_properties.insert(decl.name.clone(), resolved);
        }
    }

    // Second pass: apply declarations with var()/viewport units resolved.
    for (decl, _, _, _) in &normal_decls {
        if decl.name.starts_with("--") {
            continue;
        }
        let resolved_values = resolve_css_values(&decl.value, ctx);
        let resolved_values = resolve_property_percentages(&decl.name, &resolved_values, ctx);
        let resolved_values = resolve_remaining_calcs(&resolved_values, ctx);
        let resolved_decl = Declaration {
            name: decl.name.clone(),
            value: resolved_values,
            important: decl.important,
        };
        apply_declaration(&mut style, &resolved_decl, parent_style);
    }
    for (decl, _, _, _) in &important_decls {
        if decl.name.starts_with("--") {
            continue;
        }
        let resolved_values = resolve_css_values(&decl.value, ctx);
        let resolved_values = resolve_property_percentages(&decl.name, &resolved_values, ctx);
        let resolved_values = resolve_remaining_calcs(&resolved_values, ctx);
        let resolved_decl = Declaration {
            name: decl.name.clone(),
            value: resolved_values,
            important: decl.important,
        };
        apply_declaration(&mut style, &resolved_decl, parent_style);
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
    let prop_name = strip_vendor_prefix(&decl.name);

    // Handle `inherit` / `initial` / `unset` for any property.
    if decl.value.len() == 1 {
        match &decl.value[0] {
            CssValue::Inherit => {
                if let Some(ps) = parent_style {
                    apply_inherit(style, &prop_name, ps);
                }
                return;
            }
            CssValue::Initial => {
                apply_initial(style, &prop_name);
                return;
            }
            CssValue::Unset => {
                if is_inherited_property(&prop_name) {
                    if let Some(ps) = parent_style {
                        apply_inherit(style, &prop_name, ps);
                    }
                } else {
                    apply_initial(style, &prop_name);
                }
                return;
            }
            _ => {}
        }
    }

    match prop_name.as_str() {
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
                    "list-item" => Display::Block,
                    "table" | "table-row-group" | "table-header-group"
                    | "table-footer-group" | "table-column"
                    | "table-column-group" | "table-caption" => Display::Block,
                    "table-row" => Display::Block,
                    "table-cell" => Display::Block,
                    "contents" => Display::Block,
                    "flow-root" => Display::Block,
                    "-webkit-box" | "-moz-box" => Display::Flex,
                    "-webkit-flex" | "-moz-flex" => Display::Flex,
                    "-webkit-inline-box" | "-moz-inline-box" => Display::InlineFlex,
                    "-webkit-inline-flex" | "-moz-inline-flex" => Display::InlineFlex,
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
            if let Some(c) = first_color_or_current(&decl.value, style.color) {
                style.background_color = c;
            }
        }

        "background" => {
            for v in &decl.value {
                match v {
                    CssValue::Color(c) => style.background_color = css_color_to_color(c),
                    CssValue::Function { name, args } => {
                        let lower = name.to_ascii_lowercase();
                        if lower == "rgba" || lower == "rgb" {
                            if let Some(color) = parse_function_color(&lower, args) {
                                style.background_color = color;
                            }
                        }
                        // Silently skip gradient, url, etc.
                    }
                    CssValue::Keyword(kw) if kw == "transparent" => {
                        style.background_color = Color::TRANSPARENT;
                    }
                    CssValue::Keyword(kw) if kw == "currentcolor" => {
                        style.background_color = style.color;
                    }
                    CssValue::Keyword(kw) => {
                        if let Some(c) = resolve_system_color(kw) {
                            style.background_color = c;
                        }
                    }
                    CssValue::None => {
                        style.background_color = Color::TRANSPARENT;
                    }
                    CssValue::Url(_) => {
                        // background-image URL — silently accept
                    }
                    _ => {}
                }
            }
        }

        "font-size" => {
            let mut handled = false;
            if let Some(kw) = first_keyword(&decl.value) {
                let px = match kw {
                    "xx-small" => Some(9.0),
                    "x-small" => Some(10.0),
                    "small" => Some(13.0),
                    "medium" => Some(16.0),
                    "large" => Some(18.0),
                    "x-large" => Some(24.0),
                    "xx-large" => Some(32.0),
                    "xxx-large" => Some(48.0),
                    "smaller" => Some(style.font_size_px * 0.833),
                    "larger" => Some(style.font_size_px * 1.2),
                    _ => None,
                };
                if let Some(px) = px {
                    style.font_size_px = px;
                    style.line_height_px = px * 1.2;
                    handled = true;
                }
            }
            if !handled {
                if let Some(CssValue::Percentage(p)) = decl.value.first() {
                    let px = (*p as f32 / 100.0) * style.font_size_px;
                    style.font_size_px = px;
                    style.line_height_px = px * 1.2;
                } else if let Some(px) = first_length_px(&decl.value, style.font_size_px) {
                    style.font_size_px = px;
                    style.line_height_px = px * 1.2;
                }
            }
        }

        "font-weight" => {
            if let Some(w) = resolve_font_weight(&decl.value) {
                style.font_weight = w;
            }
        }

        "font-family" => {
            let families = collect_font_families(&decl.value);
            if !families.is_empty() {
                style.font_family = families;
            }
        }

        "line-height" => {
            if let Some(v) = &decl.value.first() {
                match v {
                    CssValue::Keyword(kw) if kw == "normal" => {
                        style.line_height_px = style.font_size_px * 1.2;
                    }
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
                    "left" | "start" => TextAlign::Left,
                    "right" | "end" => TextAlign::Right,
                    "center" => TextAlign::Center,
                    "justify" => TextAlign::Justify,
                    _ => style.text_align,
                };
            }
        }

        "margin" => {
            let vals = collect_edge_values_with_auto(&decl.value, style.font_size_px);
            if !vals.is_empty() {
                let (t, r, b, l) = expand_shorthand_4(&vals);
                style.margin.top = t;
                style.margin.right = r;
                style.margin.bottom = b;
                style.margin.left = l;
            }
        }
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
            if let Some(c) = first_color_or_current(&decl.value, style.color) {
                style.border.top.color = c;
                style.border.right.color = c;
                style.border.bottom.color = c;
                style.border.left.color = c;
            }
        }

        "border" => {
            // Per CSS spec, shorthand resets omitted values to initial.
            // Initial border-color is currentColor, initial border-width is
            // medium (3px), initial border-style is none.
            let reset = BorderSide { width: 0.0, style: BorderStyle::None, color: style.color };
            style.border.top = reset;
            style.border.right = reset;
            style.border.bottom = reset;
            style.border.left = reset;
            apply_border_side_shorthand(&decl.value, &mut style.border.top, style.font_size_px, style.color);
            apply_border_side_shorthand(&decl.value, &mut style.border.right, style.font_size_px, style.color);
            apply_border_side_shorthand(&decl.value, &mut style.border.bottom, style.font_size_px, style.color);
            apply_border_side_shorthand(&decl.value, &mut style.border.left, style.font_size_px, style.color);
        }

        "width" => {
            if let Some(CssValue::Percentage(p)) = decl.value.first() {
                style.width_pct = Some(*p as f32);
                style.width = None;
            } else {
                style.width = first_length_or_none(&decl.value, style.font_size_px);
                style.width_pct = None;
            }
        }
        "height" => {
            if let Some(CssValue::Percentage(p)) = decl.value.first() {
                style.height_pct = Some(*p as f32);
                style.height = None;
            } else {
                style.height = first_length_or_none(&decl.value, style.font_size_px);
                style.height_pct = None;
            }
        }
        "min-width" => {
            if let Some(CssValue::Percentage(p)) = decl.value.first() {
                style.min_width_pct = Some(*p as f32);
                style.min_width = None;
            } else {
                style.min_width = first_length_or_none(&decl.value, style.font_size_px);
                style.min_width_pct = None;
            }
        }
        "min-height" => {
            if let Some(CssValue::Percentage(p)) = decl.value.first() {
                style.min_height_pct = Some(*p as f32);
                style.min_height = None;
            } else {
                style.min_height = first_length_or_none(&decl.value, style.font_size_px);
                style.min_height_pct = None;
            }
        }
        "max-width" => {
            if let Some(CssValue::Percentage(p)) = decl.value.first() {
                style.max_width_pct = Some(*p as f32);
                style.max_width = None;
            } else {
                style.max_width = first_length_or_none(&decl.value, style.font_size_px);
                style.max_width_pct = None;
            }
        }
        "max-height" => {
            if let Some(CssValue::Percentage(p)) = decl.value.first() {
                style.max_height_pct = Some(*p as f32);
                style.max_height = None;
            } else {
                style.max_height = first_length_or_none(&decl.value, style.font_size_px);
                style.max_height_pct = None;
            }
        }

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

        "top" => {
            if let Some(CssValue::Percentage(p)) = decl.value.first() {
                style.top_pct = Some(*p as f32);
                style.top = None;
            } else {
                style.top = first_length_or_none(&decl.value, style.font_size_px);
                style.top_pct = None;
            }
        }
        "right" => {
            if let Some(CssValue::Percentage(p)) = decl.value.first() {
                style.right_pct = Some(*p as f32);
                style.right = None;
            } else {
                style.right = first_length_or_none(&decl.value, style.font_size_px);
                style.right_pct = None;
            }
        }
        "bottom" => {
            if let Some(CssValue::Percentage(p)) = decl.value.first() {
                style.bottom_pct = Some(*p as f32);
                style.bottom = None;
            } else {
                style.bottom = first_length_or_none(&decl.value, style.font_size_px);
                style.bottom_pct = None;
            }
        }
        "left" => {
            if let Some(CssValue::Percentage(p)) = decl.value.first() {
                style.left_pct = Some(*p as f32);
                style.left = None;
            } else {
                style.left = first_length_or_none(&decl.value, style.font_size_px);
                style.left_pct = None;
            }
        }

        "inset" => {
            let vals = collect_edge_values_with_auto(&decl.value, style.font_size_px);
            if !vals.is_empty() {
                let (t, r, b, l) = expand_shorthand_4(&vals);
                style.top = if t.is_infinite() { None } else { Some(t) };
                style.right = if r.is_infinite() { None } else { Some(r) };
                style.bottom = if b.is_infinite() { None } else { Some(b) };
                style.left = if l.is_infinite() { None } else { Some(l) };
                style.top_pct = None;
                style.right_pct = None;
                style.bottom_pct = None;
                style.left_pct = None;
            }
        }

        "block-size" => {
            if let Some(CssValue::Percentage(p)) = decl.value.first() {
                style.height_pct = Some(*p as f32);
                style.height = None;
            } else {
                style.height = first_length_or_none(&decl.value, style.font_size_px);
                style.height_pct = None;
            }
        }
        "inline-size" => {
            if let Some(CssValue::Percentage(p)) = decl.value.first() {
                style.width_pct = Some(*p as f32);
                style.width = None;
            } else {
                style.width = first_length_or_none(&decl.value, style.font_size_px);
                style.width_pct = None;
            }
        }
        "min-block-size" => {
            if let Some(CssValue::Percentage(p)) = decl.value.first() {
                style.min_height_pct = Some(*p as f32);
                style.min_height = None;
            } else {
                style.min_height = first_length_or_none(&decl.value, style.font_size_px);
                style.min_height_pct = None;
            }
        }
        "min-inline-size" => {
            if let Some(CssValue::Percentage(p)) = decl.value.first() {
                style.min_width_pct = Some(*p as f32);
                style.min_width = None;
            } else {
                style.min_width = first_length_or_none(&decl.value, style.font_size_px);
                style.min_width_pct = None;
            }
        }
        "max-block-size" => {
            if let Some(CssValue::Percentage(p)) = decl.value.first() {
                style.max_height_pct = Some(*p as f32);
                style.max_height = None;
            } else {
                style.max_height = first_length_or_none(&decl.value, style.font_size_px);
                style.max_height_pct = None;
            }
        }
        "max-inline-size" => {
            if let Some(CssValue::Percentage(p)) = decl.value.first() {
                style.max_width_pct = Some(*p as f32);
                style.max_width = None;
            } else {
                style.max_width = first_length_or_none(&decl.value, style.font_size_px);
                style.max_width_pct = None;
            }
        }

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
            if let Some(c) = first_color_or_current(&decl.value, style.color) {
                style.border.top.color = c;
            }
        }
        "border-right-color" => {
            if let Some(c) = first_color_or_current(&decl.value, style.color) {
                style.border.right.color = c;
            }
        }
        "border-bottom-color" => {
            if let Some(c) = first_color_or_current(&decl.value, style.color) {
                style.border.bottom.color = c;
            }
        }
        "border-left-color" => {
            if let Some(c) = first_color_or_current(&decl.value, style.color) {
                style.border.left.color = c;
            }
        }

        "border-top" => apply_border_side_shorthand(&decl.value, &mut style.border.top, style.font_size_px, style.color),
        "border-right" => apply_border_side_shorthand(&decl.value, &mut style.border.right, style.font_size_px, style.color),
        "border-bottom" => apply_border_side_shorthand(&decl.value, &mut style.border.bottom, style.font_size_px, style.color),
        "border-left" => apply_border_side_shorthand(&decl.value, &mut style.border.left, style.font_size_px, style.color),

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

        "place-items" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.flex.align_items = match kw {
                    "stretch" => AlignItems::Stretch,
                    "flex-start" | "start" => AlignItems::FlexStart,
                    "flex-end" | "end" => AlignItems::FlexEnd,
                    "center" => AlignItems::Center,
                    "baseline" => AlignItems::Baseline,
                    _ => style.flex.align_items,
                };
            }
        }

        "place-content" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.flex.justify_content = match kw {
                    "flex-start" | "start" => JustifyContent::FlexStart,
                    "flex-end" | "end" => JustifyContent::FlexEnd,
                    "center" => JustifyContent::Center,
                    "space-between" => JustifyContent::SpaceBetween,
                    "space-around" => JustifyContent::SpaceAround,
                    "space-evenly" => JustifyContent::SpaceEvenly,
                    _ => style.flex.justify_content,
                };
            }
        }

        "margin-inline" => {
            let vals = collect_edge_values_with_auto(&decl.value, style.font_size_px);
            match vals.len() {
                1 => { style.margin.left = vals[0]; style.margin.right = vals[0]; }
                2 => { style.margin.left = vals[0]; style.margin.right = vals[1]; }
                _ => {}
            }
        }
        "margin-block" => {
            let vals = collect_edge_values_with_auto(&decl.value, style.font_size_px);
            match vals.len() {
                1 => { style.margin.top = vals[0]; style.margin.bottom = vals[0]; }
                2 => { style.margin.top = vals[0]; style.margin.bottom = vals[1]; }
                _ => {}
            }
        }
        "margin-inline-start" => {
            if let Some(v) = first_length_or_auto(&decl.value, style.font_size_px) {
                style.margin.left = v;
            }
        }
        "margin-inline-end" => {
            if let Some(v) = first_length_or_auto(&decl.value, style.font_size_px) {
                style.margin.right = v;
            }
        }
        "margin-block-start" => {
            if let Some(v) = first_length_or_auto(&decl.value, style.font_size_px) {
                style.margin.top = v;
            }
        }
        "margin-block-end" => {
            if let Some(v) = first_length_or_auto(&decl.value, style.font_size_px) {
                style.margin.bottom = v;
            }
        }

        "padding-inline" => {
            let vals = collect_lengths(&decl.value, style.font_size_px);
            match vals.len() {
                1 => { style.padding.left = vals[0]; style.padding.right = vals[0]; }
                2 => { style.padding.left = vals[0]; style.padding.right = vals[1]; }
                _ => {}
            }
        }
        "padding-block" => {
            let vals = collect_lengths(&decl.value, style.font_size_px);
            match vals.len() {
                1 => { style.padding.top = vals[0]; style.padding.bottom = vals[0]; }
                2 => { style.padding.top = vals[0]; style.padding.bottom = vals[1]; }
                _ => {}
            }
        }
        "padding-inline-start" => {
            if let Some(v) = first_length_px(&decl.value, style.font_size_px) {
                style.padding.left = v;
            }
        }
        "padding-inline-end" => {
            if let Some(v) = first_length_px(&decl.value, style.font_size_px) {
                style.padding.right = v;
            }
        }
        "padding-block-start" => {
            if let Some(v) = first_length_px(&decl.value, style.font_size_px) {
                style.padding.top = v;
            }
        }
        "padding-block-end" => {
            if let Some(v) = first_length_px(&decl.value, style.font_size_px) {
                style.padding.bottom = v;
            }
        }

        "border-inline-width" => {
            if let Some(v) = first_length_px(&decl.value, style.font_size_px) {
                style.border.left.width = v;
                style.border.right.width = v;
            }
        }
        "border-block-width" => {
            if let Some(v) = first_length_px(&decl.value, style.font_size_px) {
                style.border.top.width = v;
                style.border.bottom.width = v;
            }
        }

        "order" => {}

        // Legacy -webkit-box-* flexbox properties (mapped to modern flexbox).
        "box-pack" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.flex.justify_content = match kw {
                    "start" => JustifyContent::FlexStart,
                    "end" => JustifyContent::FlexEnd,
                    "center" => JustifyContent::Center,
                    "justify" => JustifyContent::SpaceBetween,
                    _ => style.flex.justify_content,
                };
            }
        }

        "box-align" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.flex.align_items = match kw {
                    "start" => AlignItems::FlexStart,
                    "end" => AlignItems::FlexEnd,
                    "center" => AlignItems::Center,
                    "baseline" => AlignItems::Baseline,
                    "stretch" => AlignItems::Stretch,
                    _ => style.flex.align_items,
                };
            }
        }

        "box-orient" => {
            if let Some(kw) = first_keyword(&decl.value) {
                style.flex.direction = match kw {
                    "horizontal" | "inline-axis" => FlexDirection::Row,
                    "vertical" | "block-axis" => FlexDirection::Column,
                    _ => style.flex.direction,
                };
            }
        }

        "box-direction" => {
            if let Some(kw) = first_keyword(&decl.value) {
                if kw == "reverse" {
                    style.flex.direction = match style.flex.direction {
                        FlexDirection::Row => FlexDirection::RowReverse,
                        FlexDirection::Column => FlexDirection::ColumnReverse,
                        other => other,
                    };
                }
            }
        }

        "box-flex" => {
            if let Some(n) = first_number(&decl.value) {
                style.flex.grow = n;
            }
        }

        "box-ordinal-group" | "box-lines" => {}

        "box-shadow" => {
            if matches!(decl.value.first(), Some(CssValue::None))
                || matches!(decl.value.first(), Some(CssValue::Keyword(k)) if k == "none")
            {
                style.box_shadow.clear();
            } else if let Some(shadow) = parse_box_shadow(&decl.value, style.color) {
                style.box_shadow = vec![shadow];
            }
        }

        // Silently accept properties we parse but don't render yet.
        "outline" | "outline-width" | "outline-style" | "outline-color" | "outline-offset"
        | "transition" | "transition-property" | "transition-duration"
        | "transition-timing-function" | "transition-delay"
        | "animation" | "animation-name" | "animation-duration"
        | "animation-timing-function" | "animation-delay" | "animation-iteration-count"
        | "animation-direction" | "animation-fill-mode" | "animation-play-state"
        | "transform" | "transform-origin" | "transform-style"
        | "perspective" | "perspective-origin"
        | "will-change" | "contain" | "content"
        | "filter" | "backdrop-filter"
        | "mix-blend-mode" | "isolation"
        | "background-image" | "background-position" | "background-repeat"
        | "background-size" | "background-attachment" | "background-origin"
        | "background-clip" | "background-blend-mode"
        | "text-shadow"
        | "clip" | "clip-path" | "mask" | "mask-image"
        | "pointer-events" | "touch-action" | "user-select"
        | "resize" | "appearance"
        | "scroll-behavior" | "scroll-snap-type" | "scroll-snap-align"
        | "overscroll-behavior" | "overscroll-behavior-x" | "overscroll-behavior-y"
        | "counter-reset" | "counter-increment" | "counter-set"
        | "quotes" | "hyphens" | "tab-size" | "word-break" | "overflow-wrap"
        | "writing-mode" | "direction" | "unicode-bidi"
        | "accent-color" | "caret-color" | "color-scheme"
        | "forced-color-adjust" | "print-color-adjust"
        | "page" | "orphans" | "widows"
        | "table-layout" | "border-collapse" | "border-spacing"
        | "caption-side" | "empty-cells"
        | "column-count" | "column-width" | "columns" | "column-rule"
        | "aspect-ratio"
        | "object-fit" | "object-position"
        | "all" => {}

        _ => {
            // Unknown property — ignore.
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Inherit / Initial helpers
// ─────────────────────────────────────────────────────────────────────────────

fn strip_vendor_prefix(name: &str) -> String {
    for prefix in &["-webkit-", "-moz-", "-ms-", "-o-"] {
        if let Some(stripped) = name.strip_prefix(prefix) {
            return stripped.to_string();
        }
    }
    name.to_string()
}

fn is_inherited_property(name: &str) -> bool {
    matches!(
        name,
        "color"
            | "font-size"
            | "font-weight"
            | "font-family"
            | "font-style"
            | "line-height"
            | "text-align"
            | "text-transform"
            | "text-indent"
            | "letter-spacing"
            | "word-spacing"
            | "white-space"
            | "visibility"
            | "cursor"
            | "list-style-type"
            | "list-style"
    )
}

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
        match v {
            CssValue::Color(c) => return Some(css_color_to_color(c)),
            CssValue::Function { name, args } => {
                let lower = name.to_ascii_lowercase();
                if lower == "rgba" || lower == "rgb" {
                    if let Some(color) = parse_function_color(&lower, args) {
                        return Some(color);
                    }
                }
            }
            CssValue::Keyword(kw) => {
                if let Some(c) = resolve_system_color(kw) {
                    return Some(c);
                }
            }
            _ => {}
        }
    }
    None
}

fn first_color_or_current(values: &[CssValue], current_color: Color) -> Option<Color> {
    for v in values {
        match v {
            CssValue::Color(c) => return Some(css_color_to_color(c)),
            CssValue::Function { name, args } => {
                let lower = name.to_ascii_lowercase();
                if lower == "rgba" || lower == "rgb" {
                    if let Some(color) = parse_function_color(&lower, args) {
                        return Some(color);
                    }
                }
            }
            CssValue::Keyword(kw) if kw == "currentcolor" => return Some(current_color),
            CssValue::Keyword(kw) if kw == "transparent" => return Some(Color::TRANSPARENT),
            CssValue::Keyword(kw) => {
                if let Some(c) = resolve_system_color(kw) {
                    return Some(c);
                }
            }
            _ => {}
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
        LengthUnit::Vw => value as f32 * 12.80,  // fallback ~1280px viewport
        LengthUnit::Vh => value as f32 * 8.00,    // fallback ~800px viewport
        LengthUnit::Vmin => value as f32 * 8.00,  // min(vw, vh) fallback
        LengthUnit::Vmax => value as f32 * 12.80, // max(vw, vh) fallback
        LengthUnit::Pt => value as f32 * 1.333,   // 1pt = 4/3 px
        LengthUnit::Ch => value as f32 * parent_font_size * 0.5,
        LengthUnit::Ex => value as f32 * parent_font_size * 0.5,
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
            CssValue::Keyword(kw) => match kw.as_str() {
                "thin" => return Some(1.0),
                "medium" => return Some(3.0),
                "thick" => return Some(5.0),
                _ => {}
            },
            _ => {}
        }
    }
    None
}

fn first_length_or_auto(values: &[CssValue], parent_font_size: f32) -> Option<f32> {
    for v in values {
        match v {
            CssValue::Auto => return Some(f32::INFINITY),
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

fn collect_font_families(values: &[CssValue]) -> String {
    let mut families: Vec<String> = Vec::new();
    let mut current = String::new();
    for v in values {
        match v {
            CssValue::String(s) => {
                if !current.is_empty() {
                    families.push(current.clone());
                    current.clear();
                }
                families.push(s.clone());
            }
            CssValue::Keyword(kw) => {
                if current.is_empty() {
                    current = kw.clone();
                } else {
                    current.push(' ');
                    current.push_str(kw);
                }
            }
            _ => {
                if !current.is_empty() {
                    families.push(current.clone());
                    current.clear();
                }
            }
        }
    }
    if !current.is_empty() {
        families.push(current);
    }
    if families.is_empty() {
        String::new()
    } else {
        families[0].clone()
    }
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

fn collect_edge_values_with_auto(values: &[CssValue], parent_font_size: f32) -> Vec<f32> {
    let mut result = Vec::new();
    for v in values {
        match v {
            CssValue::Auto => result.push(f32::INFINITY),
            CssValue::Length(val, unit) => {
                result.push(resolve_length(*val, unit, parent_font_size));
            }
            CssValue::Number(n) if *n == 0.0 => result.push(0.0),
            CssValue::Percentage(p) => result.push(*p as f32),
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

fn resolve_system_color(name: &str) -> Option<Color> {
    match name {
        "canvas" | "Canvas" => Some(Color::WHITE),
        "canvastext" | "CanvasText" => Some(Color::BLACK),
        "linktext" | "LinkText" => Some(Color::rgb(0, 0, 238)),
        "visitedtext" | "VisitedText" => Some(Color::rgb(85, 26, 139)),
        "activetext" | "ActiveText" => Some(Color::rgb(255, 0, 0)),
        "buttonface" | "ButtonFace" => Some(Color::rgb(240, 240, 240)),
        "buttontext" | "ButtonText" => Some(Color::BLACK),
        "buttonborder" | "ButtonBorder" => Some(Color::rgb(118, 118, 118)),
        "field" | "Field" => Some(Color::WHITE),
        "fieldtext" | "FieldText" => Some(Color::BLACK),
        "highlight" | "Highlight" | "selecteditem" | "SelectedItem" => Some(Color::rgb(0, 120, 215)),
        "highlighttext" | "HighlightText" => Some(Color::WHITE),
        "graytext" | "GrayText" => Some(Color::rgb(109, 109, 109)),
        "mark" | "Mark" => Some(Color::rgb(255, 255, 0)),
        "marktext" | "MarkText" => Some(Color::BLACK),
        _ => None,
    }
}

fn parse_function_color(_name: &str, args: &[CssValue]) -> Option<Color> {
    let mut nums = Vec::new();
    for v in args {
        match v {
            CssValue::Number(n) => nums.push(*n),
            CssValue::Percentage(p) => nums.push(*p * 255.0 / 100.0),
            _ => {}
        }
    }
    if nums.len() >= 3 {
        let r = nums[0].clamp(0.0, 255.0) as u8;
        let g = nums[1].clamp(0.0, 255.0) as u8;
        let b = nums[2].clamp(0.0, 255.0) as u8;
        let a = if nums.len() >= 4 {
            if nums[3] <= 1.0 {
                (nums[3] * 255.0) as u8
            } else {
                nums[3].clamp(0.0, 255.0) as u8
            }
        } else {
            255
        };
        Some(Color::rgba(r, g, b, a))
    } else {
        None
    }
}

fn parse_box_shadow(values: &[CssValue], current_color: Color) -> Option<BoxShadow> {
    let mut lengths = Vec::new();
    let mut color = None;
    let mut inset = false;
    for v in values {
        match v {
            CssValue::Length(val, unit) => lengths.push(resolve_length(*val, unit, 16.0)),
            CssValue::Number(n) if *n == 0.0 => lengths.push(0.0),
            CssValue::Color(c) => color = Some(css_color_to_color(c)),
            CssValue::Keyword(k) if k == "inset" => inset = true,
            CssValue::Keyword(k) if k == "currentcolor" => color = Some(current_color),
            _ => {}
        }
    }
    if lengths.len() >= 2 {
        Some(BoxShadow {
            offset_x: lengths[0],
            offset_y: lengths[1],
            blur: lengths.get(2).copied().unwrap_or(0.0),
            spread: lengths.get(3).copied().unwrap_or(0.0),
            color: color.unwrap_or(current_color),
            inset,
        })
    } else {
        None
    }
}

fn apply_border_side_shorthand(values: &[CssValue], side: &mut BorderSide, parent_font_size: f32, current_color: Color) {
    for v in values {
        match v {
            CssValue::Length(val, unit) => {
                side.width = resolve_length(*val, unit, parent_font_size);
            }
            CssValue::Number(n) if *n == 0.0 => {
                side.width = 0.0;
            }
            CssValue::Keyword(kw) => {
                match kw.as_str() {
                    "thin" => side.width = 1.0,
                    "medium" => side.width = 3.0,
                    "thick" => side.width = 5.0,
                    "currentcolor" => side.color = current_color,
                    _ => {
                        let bs = parse_border_style(kw);
                        if bs != BorderStyle::None || kw == "none" {
                            side.style = bs;
                        }
                    }
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

    fn default_ctx() -> ResolveContext {
        ResolveContext::new(1280.0, 800.0)
    }

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
        let style = resolve_style(&dom, div, &matched, None, &mut default_ctx());
        assert_eq!(style.display, Display::Block);
    }

    #[test]
    fn resolve_color_and_font_size() {
        let (dom, div, _) = build_dom_and_style("");
        let ss = parse_stylesheet("div { color: red; font-size: 20px; }");
        let sheets = vec![(ss, StyleOrigin::Author)];
        let matched = collect_matching_rules(&dom, div, &sheets);
        let style = resolve_style(&dom, div, &matched, None, &mut default_ctx());
        assert_eq!(style.color, Color::rgb(255, 0, 0));
        assert_eq!(style.font_size_px, 20.0);
    }

    #[test]
    fn resolve_margin_shorthand() {
        let (dom, div, _) = build_dom_and_style("");
        let ss = parse_stylesheet("div { margin: 10px 20px; }");
        let sheets = vec![(ss, StyleOrigin::Author)];
        let matched = collect_matching_rules(&dom, div, &sheets);
        let style = resolve_style(&dom, div, &matched, None, &mut default_ctx());
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
        let div_style = resolve_style(&dom, div, &div_matched, None, &mut default_ctx());
        assert_eq!(div_style.color, Color::rgb(0, 0, 255));

        let p_matched = collect_matching_rules(&dom, p, &sheets);
        let p_style = resolve_style(&dom, p, &p_matched, Some(&div_style), &mut default_ctx());
        // color is inherited
        assert_eq!(p_style.color, Color::rgb(0, 0, 255));
    }

    #[test]
    fn non_inherited_not_passed() {
        let (dom, div, p) = build_dom_and_style("");
        let ss = parse_stylesheet("div { margin: 10px; background-color: red; }");
        let sheets = vec![(ss, StyleOrigin::Author)];

        let div_matched = collect_matching_rules(&dom, div, &sheets);
        let div_style = resolve_style(&dom, div, &div_matched, None, &mut default_ctx());

        let p_matched = collect_matching_rules(&dom, p, &sheets);
        let p_style = resolve_style(&dom, p, &p_matched, Some(&div_style), &mut default_ctx());
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
        let style = resolve_style(&dom, div, &matched, None, &mut default_ctx());
        // !important should win even though #main has higher specificity
        assert_eq!(style.color, Color::rgb(0, 0, 255));
    }

    #[test]
    fn specificity_ordering() {
        let (dom, div, _) = build_dom_and_style("");
        let ss = parse_stylesheet("div { color: red; } #main { color: blue; }");
        let sheets = vec![(ss, StyleOrigin::Author)];
        let matched = collect_matching_rules(&dom, div, &sheets);
        let style = resolve_style(&dom, div, &matched, None, &mut default_ctx());
        // #main has higher specificity than div
        assert_eq!(style.color, Color::rgb(0, 0, 255));
    }

    #[test]
    fn source_order_tiebreak() {
        let (dom, div, _) = build_dom_and_style("");
        let ss = parse_stylesheet("div { color: red; } div { color: blue; }");
        let sheets = vec![(ss, StyleOrigin::Author)];
        let matched = collect_matching_rules(&dom, div, &sheets);
        let style = resolve_style(&dom, div, &matched, None, &mut default_ctx());
        // Later rule wins at same specificity
        assert_eq!(style.color, Color::rgb(0, 0, 255));
    }

    #[test]
    fn resolve_opacity() {
        let (dom, div, _) = build_dom_and_style("");
        let ss = parse_stylesheet("div { opacity: 0.5; }");
        let sheets = vec![(ss, StyleOrigin::Author)];
        let matched = collect_matching_rules(&dom, div, &sheets);
        let style = resolve_style(&dom, div, &matched, None, &mut default_ctx());
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
        let style = resolve_style(&dom, div, &matched, None, &mut default_ctx());
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

    #[test]
    fn margin_auto_produces_infinity_sentinel() {
        let (dom, div, _) = build_dom_and_style("");
        let ss = parse_stylesheet("div { margin: 0 auto; }");
        let sheets = vec![(ss, StyleOrigin::Author)];
        let matched = collect_matching_rules(&dom, div, &sheets);
        let style = resolve_style(&dom, div, &matched, None, &mut default_ctx());
        assert_eq!(style.margin.top, 0.0);
        assert!(style.margin.right.is_infinite());
        assert_eq!(style.margin.bottom, 0.0);
        assert!(style.margin.left.is_infinite());
    }

    #[test]
    fn webkit_prefix_display_flex() {
        let (dom, div, _) = build_dom_and_style("");
        let ss = parse_stylesheet("div { display: flex; -webkit-box-sizing: border-box; }");
        let sheets = vec![(ss, StyleOrigin::Author)];
        let matched = collect_matching_rules(&dom, div, &sheets);
        let style = resolve_style(&dom, div, &matched, None, &mut default_ctx());
        assert_eq!(style.display, Display::Flex);
        assert_eq!(style.box_sizing, BoxSizing::BorderBox);
    }

    #[test]
    fn percentage_width_resolved() {
        let (dom, div, _) = build_dom_and_style("");
        let ss = parse_stylesheet("div { width: 50%; }");
        let sheets = vec![(ss, StyleOrigin::Author)];
        let matched = collect_matching_rules(&dom, div, &sheets);
        let style = resolve_style(&dom, div, &matched, None, &mut default_ctx());
        // Percentage stored in width_pct, resolved at layout time
        assert_eq!(style.width, None);
        assert_eq!(style.width_pct, Some(50.0));
    }

    #[test]
    fn percentage_height_resolved() {
        let (dom, div, _) = build_dom_and_style("");
        let ss = parse_stylesheet("div { height: 100%; }");
        let sheets = vec![(ss, StyleOrigin::Author)];
        let matched = collect_matching_rules(&dom, div, &sheets);
        let style = resolve_style(&dom, div, &matched, None, &mut default_ctx());
        // Percentage stored in height_pct, resolved at layout time
        assert_eq!(style.height, None);
        assert_eq!(style.height_pct, Some(100.0));
    }

    #[test]
    fn font_size_percentage() {
        let (dom, _, p) = build_dom_and_style("");
        let ss = parse_stylesheet("p { font-size: 200%; }");
        let sheets = vec![(ss, StyleOrigin::Author)];
        // Parent has default 16px font-size
        let parent_style = ComputedStyle::default();
        let matched = collect_matching_rules(&dom, p, &sheets);
        let style = resolve_style(&dom, p, &matched, Some(&parent_style), &mut default_ctx());
        // 200% of inherited 16px = 32px
        assert_eq!(style.font_size_px, 32.0);
    }
}
