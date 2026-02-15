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
    Table,
}

impl Display {
    /// Returns `true` if this display value generates a block-level box.
    pub fn is_block_level(self) -> bool {
        matches!(self, Display::Block | Display::Flex | Display::Grid | Display::Table)
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
        ListStyleType::None
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
// WordBreak
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordBreak {
    Normal,
    BreakAll,
    KeepAll,
    BreakWord,
}

impl Default for WordBreak {
    fn default() -> Self {
        WordBreak::Normal
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// OverflowWrap
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowWrap {
    Normal,
    BreakWord,
    Anywhere,
}

impl Default for OverflowWrap {
    fn default() -> Self {
        OverflowWrap::Normal
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Direction
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Ltr,
    Rtl,
}

impl Default for Direction {
    fn default() -> Self {
        Direction::Ltr
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PointerEvents
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerEvents {
    Auto,
    None,
}

impl Default for PointerEvents {
    fn default() -> Self {
        PointerEvents::Auto
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// UserSelect
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserSelect {
    Auto,
    None,
    Text,
    All,
}

impl Default for UserSelect {
    fn default() -> Self {
        UserSelect::Auto
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ObjectFit
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectFit {
    Fill,
    Contain,
    Cover,
    None,
    ScaleDown,
}

impl Default for ObjectFit {
    fn default() -> Self {
        ObjectFit::Fill
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Resize
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resize {
    None,
    Both,
    Horizontal,
    Vertical,
}

impl Default for Resize {
    fn default() -> Self {
        Resize::None
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// BorderCollapse
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderCollapse {
    Separate,
    Collapse,
}

impl Default for BorderCollapse {
    fn default() -> Self {
        BorderCollapse::Separate
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TableLayout
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableLayout {
    Auto,
    Fixed,
}

impl Default for TableLayout {
    fn default() -> Self {
        TableLayout::Auto
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// BackgroundRepeat
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundRepeat {
    Repeat,
    NoRepeat,
    RepeatX,
    RepeatY,
    Space,
    Round,
}

impl Default for BackgroundRepeat {
    fn default() -> Self {
        BackgroundRepeat::Repeat
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// BackgroundSize
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BackgroundSize {
    Auto,
    Cover,
    Contain,
    Explicit(f32, f32),
}

impl Default for BackgroundSize {
    fn default() -> Self {
        BackgroundSize::Auto
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// WritingMode
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WritingMode {
    HorizontalTb,
    VerticalRl,
    VerticalLr,
}

impl Default for WritingMode {
    fn default() -> Self {
        WritingMode::HorizontalTb
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// UnicodeBidi
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnicodeBidi {
    Normal,
    Embed,
    Isolate,
    BidiOverride,
    IsolateOverride,
    Plaintext,
}

impl Default for UnicodeBidi {
    fn default() -> Self {
        UnicodeBidi::Normal
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// CaptionSide
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptionSide {
    Top,
    Bottom,
}

impl Default for CaptionSide {
    fn default() -> Self {
        CaptionSide::Top
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// EmptyCells
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmptyCells {
    Show,
    Hide,
}

impl Default for EmptyCells {
    fn default() -> Self {
        EmptyCells::Show
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// MixBlendMode
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixBlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl Default for MixBlendMode {
    fn default() -> Self {
        MixBlendMode::Normal
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Isolation
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Isolation {
    Auto,
    Isolate,
}

impl Default for Isolation {
    fn default() -> Self {
        Isolation::Auto
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ScrollBehavior
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollBehavior {
    Auto,
    Smooth,
}

impl Default for ScrollBehavior {
    fn default() -> Self {
        ScrollBehavior::Auto
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ColorScheme
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorScheme {
    Normal,
    Light,
    Dark,
}

impl Default for ColorScheme {
    fn default() -> Self {
        ColorScheme::Normal
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TransformFunction
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum TransformFunction {
    Translate(f32, f32),
    TranslateX(f32),
    TranslateY(f32),
    Scale(f32, f32),
    ScaleX(f32),
    ScaleY(f32),
    Rotate(f32),
    SkewX(f32),
    SkewY(f32),
    Matrix(f32, f32, f32, f32, f32, f32),
}

// ─────────────────────────────────────────────────────────────────────────────
// FilterFunction
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum FilterFunction {
    Opacity(f32),
    Blur(f32),
    Brightness(f32),
    Contrast(f32),
    Grayscale(f32),
    Saturate(f32),
    Invert(f32),
    Sepia(f32),
    HueRotate(f32),
    DropShadow(f32, f32, f32, Color),
}

// ─────────────────────────────────────────────────────────────────────────────
// BackgroundImage
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct GradientStop {
    pub position: f32,
    pub color: Color,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BackgroundImage {
    None,
    LinearGradient {
        angle_deg: f32,
        stops: Vec<GradientStop>,
    },
}

impl Default for BackgroundImage {
    fn default() -> Self {
        BackgroundImage::None
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TextShadow
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct TextShadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    pub color: Color,
}

// ─────────────────────────────────────────────────────────────────────────────
// BoxShadow
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct BoxShadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: Color,
    pub inset: bool,
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

    // -- Percentage sizing (resolved at layout time against containing block) --
    pub width_pct: Option<f32>,
    pub height_pct: Option<f32>,
    pub min_width_pct: Option<f32>,
    pub min_height_pct: Option<f32>,
    pub max_width_pct: Option<f32>,
    pub max_height_pct: Option<f32>,

    // -- Position offsets --
    pub top: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
    pub left: Option<f32>,

    // -- Percentage position offsets (resolved at layout time) --
    pub top_pct: Option<f32>,
    pub right_pct: Option<f32>,
    pub bottom_pct: Option<f32>,
    pub left_pct: Option<f32>,

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
    pub is_list_item: bool,

    // -- Box shadow --
    pub box_shadow: Vec<BoxShadow>,

    // -- Background image --
    pub background_image: BackgroundImage,

    // -- Text shadow --
    pub text_shadow: Vec<TextShadow>,

    // -- Text breaking --
    pub word_break: WordBreak,
    pub overflow_wrap: OverflowWrap,
    pub tab_size: f32,
    pub hyphens: bool,

    // -- Directionality --
    pub direction: Direction,
    pub writing_mode: WritingMode,
    pub unicode_bidi: UnicodeBidi,

    // -- Interaction --
    pub pointer_events: PointerEvents,
    pub user_select: UserSelect,
    pub resize: Resize,

    // -- Object/image --
    pub object_fit: ObjectFit,
    pub object_position_x: f32,
    pub object_position_y: f32,

    // -- Table --
    pub border_collapse: BorderCollapse,
    pub border_spacing: f32,
    pub table_layout: TableLayout,
    pub caption_side: CaptionSide,
    pub empty_cells: EmptyCells,

    // -- Background extended --
    pub background_repeat: BackgroundRepeat,
    pub background_size: BackgroundSize,
    pub background_position_x: f32,
    pub background_position_y: f32,

    // -- Outline --
    pub outline_width: f32,
    pub outline_style: BorderStyle,
    pub outline_color: Color,
    pub outline_offset: f32,

    // -- Sizing --
    pub aspect_ratio: Option<f32>,

    // -- Content --
    pub content: Option<String>,

    // -- Transform --
    pub transform: Vec<TransformFunction>,
    pub transform_origin_x: f32,
    pub transform_origin_y: f32,

    // -- Filter --
    pub filter: Vec<FilterFunction>,
    pub backdrop_filter: Vec<FilterFunction>,

    // -- Multi-column --
    pub column_count: Option<u32>,
    pub column_width: Option<f32>,
    pub column_gap_val: Option<f32>,

    // -- Containment --
    pub will_change: bool,
    pub contain_layout: bool,
    pub contain_paint: bool,

    // -- Blending --
    pub mix_blend_mode: MixBlendMode,
    pub isolation: Isolation,

    // -- Scroll --
    pub scroll_behavior: ScrollBehavior,

    // -- Colors --
    pub accent_color: Option<Color>,
    pub caret_color: Option<Color>,
    pub color_scheme: ColorScheme,

    // -- Transition/Animation (stored for future use) --
    pub transition_duration_ms: f32,
    pub transition_property: Option<String>,
    pub animation_name: Option<String>,
    pub animation_duration_ms: f32,
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

            width_pct: None,
            height_pct: None,
            min_width_pct: None,
            min_height_pct: None,
            max_width_pct: None,
            max_height_pct: None,

            top: None,
            right: None,
            bottom: None,
            left: None,

            top_pct: None,
            right_pct: None,
            bottom_pct: None,
            left_pct: None,

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

            list_style_type: ListStyleType::None,
            is_list_item: false,

            box_shadow: Vec::new(),

            background_image: BackgroundImage::None,

            text_shadow: Vec::new(),

            word_break: WordBreak::Normal,
            overflow_wrap: OverflowWrap::Normal,
            tab_size: 8.0,
            hyphens: false,

            direction: Direction::Ltr,
            writing_mode: WritingMode::HorizontalTb,
            unicode_bidi: UnicodeBidi::Normal,

            pointer_events: PointerEvents::Auto,
            user_select: UserSelect::Auto,
            resize: Resize::None,

            object_fit: ObjectFit::Fill,
            object_position_x: 50.0,
            object_position_y: 50.0,

            border_collapse: BorderCollapse::Separate,
            border_spacing: 0.0,
            table_layout: TableLayout::Auto,
            caption_side: CaptionSide::Top,
            empty_cells: EmptyCells::Show,

            background_repeat: BackgroundRepeat::Repeat,
            background_size: BackgroundSize::Auto,
            background_position_x: 0.0,
            background_position_y: 0.0,

            outline_width: 0.0,
            outline_style: BorderStyle::None,
            outline_color: Color::BLACK,
            outline_offset: 0.0,

            aspect_ratio: None,

            content: None,

            transform: Vec::new(),
            transform_origin_x: 50.0,
            transform_origin_y: 50.0,

            filter: Vec::new(),
            backdrop_filter: Vec::new(),

            column_count: None,
            column_width: None,
            column_gap_val: None,

            will_change: false,
            contain_layout: false,
            contain_paint: false,

            mix_blend_mode: MixBlendMode::Normal,
            isolation: Isolation::Auto,

            scroll_behavior: ScrollBehavior::Auto,

            accent_color: None,
            caret_color: None,
            color_scheme: ColorScheme::Normal,

            transition_duration_ms: 0.0,
            transition_property: None,
            animation_name: None,
            animation_duration_ms: 0.0,
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
            || !self.transform.is_empty()
            || !self.filter.is_empty()
            || self.will_change
            || self.mix_blend_mode != MixBlendMode::Normal
            || self.isolation == Isolation::Isolate
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
