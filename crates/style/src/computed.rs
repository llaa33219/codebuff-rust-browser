//! Computed style values — the final resolved CSS properties for a node.

use common::{Color, Edges};

// ─────────────────────────────────────────────────────────────────────────────
// Display
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    None,
    Block,
    Inline,
    InlineBlock,
    Flex,
    InlineFlex,
    Grid,
    InlineGrid,
}

impl Display {
    /// Returns `true` if this display value generates a block-level box.
    pub fn is_block_level(self) -> bool {
        matches!(self, Display::Block | Display::Flex | Display::Grid)
    }

    /// Returns `true` if this display value generates an inline-level box.
    pub fn is_inline_level(self) -> bool {
        matches!(
            self,
            Display::Inline | Display::InlineBlock | Display::InlineFlex | Display::InlineGrid
        )
    }
}

impl Default for Display {
    fn default() -> Self {
        Display::Inline
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Position
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

impl Default for Position {
    fn default() -> Self {
        Position::Static
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Float
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Float {
    None,
    Left,
    Right,
}

impl Default for Float {
    fn default() -> Self {
        Float::None
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TextAlign
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Right,
    Center,
    Justify,
}

impl Default for TextAlign {
    fn default() -> Self {
        TextAlign::Left
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// BorderStyle
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderStyle {
    None,
    Solid,
    Dotted,
    Dashed,
    Double,
    Groove,
    Ridge,
    Inset,
    Outset,
}

impl Default for BorderStyle {
    fn default() -> Self {
        BorderStyle::None
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Overflow
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    Visible,
    Hidden,
    Scroll,
    Auto,
}

impl Default for Overflow {
    fn default() -> Self {
        Overflow::Visible
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Flex properties
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

impl Default for FlexDirection {
    fn default() -> Self {
        FlexDirection::Row
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexWrap {
    NoWrap,
    Wrap,
    WrapReverse,
}

impl Default for FlexWrap {
    fn default() -> Self {
        FlexWrap::NoWrap
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JustifyContent {
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

impl Default for JustifyContent {
    fn default() -> Self {
        JustifyContent::FlexStart
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignItems {
    Stretch,
    FlexStart,
    FlexEnd,
    Center,
    Baseline,
}

impl Default for AlignItems {
    fn default() -> Self {
        AlignItems::Stretch
    }
}

/// Flexbox-related computed style properties.
#[derive(Debug, Clone, PartialEq)]
pub struct FlexStyle {
    pub direction: FlexDirection,
    pub wrap: FlexWrap,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    pub grow: f32,
    pub shrink: f32,
    pub basis: Option<f32>,
}

impl Default for FlexStyle {
    fn default() -> Self {
        Self {
            direction: FlexDirection::default(),
            wrap: FlexWrap::default(),
            justify_content: JustifyContent::default(),
            align_items: AlignItems::default(),
            grow: 0.0,
            shrink: 1.0,
            basis: None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Grid properties
// ─────────────────────────────────────────────────────────────────────────────

/// A breadth value used in grid track sizing (e.g. inside `minmax()`).
#[derive(Debug, Clone, PartialEq)]
pub enum GridBreadth {
    Fixed(f32),
    Fr(f32),
    Auto,
    MinContent,
    MaxContent,
}

impl Default for GridBreadth {
    fn default() -> Self {
        GridBreadth::Auto
    }
}

/// A grid track size.
#[derive(Debug, Clone, PartialEq)]
pub enum GridTrackSize {
    Fixed(f32),
    Fr(f32),
    Auto,
    MinMax(GridBreadth, GridBreadth),
}

impl Default for GridTrackSize {
    fn default() -> Self {
        GridTrackSize::Auto
    }
}

/// Controls the auto-placement algorithm direction and density.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridAutoFlow {
    Row,
    Column,
    RowDense,
    ColumnDense,
}

impl Default for GridAutoFlow {
    fn default() -> Self {
        GridAutoFlow::Row
    }
}

/// Grid-related computed style properties.
#[derive(Debug, Clone, PartialEq)]
pub struct GridStyle {
    pub template_columns: Vec<GridTrackSize>,
    pub template_rows: Vec<GridTrackSize>,
    pub auto_flow: GridAutoFlow,
    pub column_gap: f32,
    pub row_gap: f32,
}

impl Default for GridStyle {
    fn default() -> Self {
        Self {
            template_columns: Vec::new(),
            template_rows: Vec::new(),
            auto_flow: GridAutoFlow::Row,
            column_gap: 0.0,
            row_gap: 0.0,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TextDecoration
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDecoration {
    None,
    Underline,
    Overline,
    LineThrough,
}

impl Default for TextDecoration {
    fn default() -> Self {
        TextDecoration::None
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// FontStyle
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

impl Default for FontStyle {
    fn default() -> Self {
        FontStyle::Normal
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Visibility
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Visible,
    Hidden,
    Collapse,
}

impl Default for Visibility {
    fn default() -> Self {
        Visibility::Visible
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// BoxSizing
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxSizing {
    ContentBox,
    BorderBox,
}

impl Default for BoxSizing {
    fn default() -> Self {
        BoxSizing::ContentBox
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// WhiteSpace
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhiteSpace {
    Normal,
    NoWrap,
    Pre,
    PreWrap,
    PreLine,
}

impl Default for WhiteSpace {
    fn default() -> Self {
        WhiteSpace::Normal
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TextTransform
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextTransform {
    None,
    Uppercase,
    Lowercase,
    Capitalize,
}

impl Default for TextTransform {
    fn default() -> Self {
        TextTransform::None
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// VerticalAlign
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalAlign {
    Baseline,
    Top,
    Middle,
    Bottom,
    TextTop,
    TextBottom,
    Sub,
    Super,
}

impl Default for VerticalAlign {
    fn default() -> Self {
        VerticalAlign::Baseline
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TextOverflow
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextOverflow {
    Clip,
    Ellipsis,
}

impl Default for TextOverflow {
    fn default() -> Self {
        TextOverflow::Clip
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ListStyleType
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListStyleType {
    None,
    Disc,
    Circle,
    Square,
    Decimal,
}

impl Default for ListStyleType {
    fn default() -> Self {
        ListStyleType::Disc
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AlignSelf
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignSelf {
    Auto,
    FlexStart,
    FlexEnd,
    Center,
    Baseline,
    Stretch,
}

impl Default for AlignSelf {
    fn default() -> Self {
        AlignSelf::Auto
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AlignContent
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignContent {
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    Stretch,
}

impl Default for AlignContent {
    fn default() -> Self {
        AlignContent::Stretch
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Cursor
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cursor {
    Auto,
    Default,
    Pointer,
    Text,
    Move,
    NotAllowed,
    Crosshair,
    Wait,
}

impl Default for Cursor {
    fn default() -> Self {
        Cursor::Auto
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// BorderSide
// ─────────────────────────────────────────────────────────────────────────────

/// A single border side (width + style + color).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderSide {
    pub width: f32,
    pub style: BorderStyle,
    pub color: Color,
}

impl Default for BorderSide {
    fn default() -> Self {
        Self {
            width: 0.0,
            style: BorderStyle::None,
            color: Color::BLACK,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ComputedStyle
// ─────────────────────────────────────────────────────────────────────────────

/// The full set of computed CSS properties for a single node.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    // -- Box model --
    pub display: Display,
    pub position: Position,
    pub float: Float,
    pub box_sizing: BoxSizing,

    // -- Color --
    pub color: Color,
    pub background_color: Color,

    // -- Typography --
    pub font_size_px: f32,
    pub font_weight: u16,
    pub font_family: String,
    pub font_style: FontStyle,
    pub line_height_px: f32,
    pub text_align: TextAlign,
    pub text_decoration: TextDecoration,
    pub text_transform: TextTransform,
    pub text_indent: f32,
    pub text_overflow: TextOverflow,
    pub letter_spacing: f32,
    pub word_spacing: f32,
    pub white_space: WhiteSpace,
    pub vertical_align: VerticalAlign,

    // -- Box dimensions --
    pub margin: Edges<f32>,
    pub padding: Edges<f32>,
    pub border: Edges<BorderSide>,
    pub border_radius: [f32; 4],

    // -- Sizing --
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub min_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,

    // -- Position offsets --
    pub top: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
    pub left: Option<f32>,

    // -- Flexbox --
    pub flex: FlexStyle,
    pub align_self: AlignSelf,
    pub align_content: AlignContent,
    pub gap: f32,

    // -- Grid --
    pub grid: GridStyle,

    // -- Stacking --
    pub z_index: Option<i32>,

    // -- Overflow --
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,

    // -- Visual --
    pub opacity: f32,
    pub visibility: Visibility,
    pub cursor: Cursor,

    // -- List --
    pub list_style_type: ListStyleType,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self {
            display: Display::Inline,
            position: Position::Static,
            float: Float::None,
            box_sizing: BoxSizing::ContentBox,

            color: Color::BLACK,
            background_color: Color::TRANSPARENT,

            font_size_px: 16.0,
            font_weight: 400,
            font_family: String::from("serif"),
            font_style: FontStyle::Normal,
            line_height_px: 19.2, // 1.2 * 16
            text_align: TextAlign::Left,
            text_decoration: TextDecoration::None,
            text_transform: TextTransform::None,
            text_indent: 0.0,
            text_overflow: TextOverflow::Clip,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            white_space: WhiteSpace::Normal,
            vertical_align: VerticalAlign::Baseline,

            margin: Edges::zero(),
            padding: Edges::zero(),
            border: Edges {
                top: BorderSide::default(),
                right: BorderSide::default(),
                bottom: BorderSide::default(),
                left: BorderSide::default(),
            },
            border_radius: [0.0; 4],

            width: None,
            height: None,
            min_width: None,
            min_height: None,
            max_width: None,
            max_height: None,

            top: None,
            right: None,
            bottom: None,
            left: None,

            flex: FlexStyle::default(),
            align_self: AlignSelf::Auto,
            align_content: AlignContent::Stretch,
            gap: 0.0,

            grid: GridStyle::default(),

            z_index: None,

            overflow_x: Overflow::Visible,
            overflow_y: Overflow::Visible,

            opacity: 1.0,
            visibility: Visibility::Visible,
            cursor: Cursor::Auto,

            list_style_type: ListStyleType::Disc,
        }
    }
}

impl ComputedStyle {
    /// Create a default style appropriate for the root element (display: block).
    pub fn root_default() -> Self {
        Self {
            display: Display::Block,
            ..Self::default()
        }
    }

    /// Get border widths as `Edges<f32>`.
    pub fn border_widths(&self) -> Edges<f32> {
        Edges {
            top: self.border.top.width,
            right: self.border.right.width,
            bottom: self.border.bottom.width,
            left: self.border.left.width,
        }
    }

    /// Get border colors as `Edges<Color>`.
    pub fn border_colors(&self) -> Edges<Color> {
        Edges {
            top: self.border.top.color,
            right: self.border.right.color,
            bottom: self.border.bottom.color,
            left: self.border.left.color,
        }
    }

    /// Whether this element creates a new stacking context.
    pub fn creates_stacking_context(&self) -> bool {
        self.z_index.is_some()
            || self.opacity < 1.0
            || self.position == Position::Fixed
            || self.position == Position::Sticky
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_computed_style() {
        let s = ComputedStyle::default();
        assert_eq!(s.display, Display::Inline);
        assert_eq!(s.position, Position::Static);
        assert_eq!(s.float, Float::None);
        assert_eq!(s.color, Color::BLACK);
        assert_eq!(s.background_color, Color::TRANSPARENT);
        assert_eq!(s.font_size_px, 16.0);
        assert_eq!(s.font_weight, 400);
        assert_eq!(s.line_height_px, 19.2);
        assert_eq!(s.text_align, TextAlign::Left);
        assert_eq!(s.opacity, 1.0);
        assert_eq!(s.overflow_x, Overflow::Visible);
        assert_eq!(s.z_index, None);
        assert_eq!(s.width, None);
        assert_eq!(s.height, None);
    }

    #[test]
    fn root_default_is_block() {
        let s = ComputedStyle::root_default();
        assert_eq!(s.display, Display::Block);
    }

    #[test]
    fn border_widths() {
        let mut s = ComputedStyle::default();
        s.border.top.width = 1.0;
        s.border.right.width = 2.0;
        s.border.bottom.width = 3.0;
        s.border.left.width = 4.0;
        let w = s.border_widths();
        assert_eq!(w.top, 1.0);
        assert_eq!(w.right, 2.0);
        assert_eq!(w.bottom, 3.0);
        assert_eq!(w.left, 4.0);
    }

    #[test]
    fn display_block_level() {
        assert!(Display::Block.is_block_level());
        assert!(Display::Flex.is_block_level());
        assert!(!Display::Inline.is_block_level());
        assert!(!Display::InlineBlock.is_block_level());
    }

    #[test]
    fn display_inline_level() {
        assert!(Display::Inline.is_inline_level());
        assert!(Display::InlineBlock.is_inline_level());
        assert!(!Display::Block.is_inline_level());
    }

    #[test]
    fn stacking_context_creation() {
        let mut s = ComputedStyle::default();
        assert!(!s.creates_stacking_context());

        s.z_index = Some(1);
        assert!(s.creates_stacking_context());

        s.z_index = None;
        s.opacity = 0.5;
        assert!(s.creates_stacking_context());

        s.opacity = 1.0;
        s.position = Position::Fixed;
        assert!(s.creates_stacking_context());
    }

    #[test]
    fn flex_style_defaults() {
        let f = FlexStyle::default();
        assert_eq!(f.direction, FlexDirection::Row);
        assert_eq!(f.wrap, FlexWrap::NoWrap);
        assert_eq!(f.justify_content, JustifyContent::FlexStart);
        assert_eq!(f.align_items, AlignItems::Stretch);
        assert_eq!(f.grow, 0.0);
        assert_eq!(f.shrink, 1.0);
        assert_eq!(f.basis, None);
    }
}
