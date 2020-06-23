/// An anchor coordinate.
#[derive(Clone, Copy, Debug)]
pub enum AnchorCoord {
    /// A value of zero in this dimension.
    Zero,

    /// The center of the quad in this dimension.
    Center,

    /// The maximum extent of the quad in this dimension.
    Max,

    /// An absolute anchor coordinate, in pixels.
    Absolute(i32),

    /// A proportion of the maximum extent of the quad in this dimension.
    Proportion(f32),
}

impl AnchorCoord {
    pub fn to_value(&self, max: u32) -> i32 {
        match *self {
            AnchorCoord::Zero => 0,
            AnchorCoord::Center => max as i32 / 2,
            AnchorCoord::Max => max as i32,
            AnchorCoord::Absolute(v) => v,
            AnchorCoord::Proportion(p) => (p * max as f32) as i32,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Anchor {
    pub x: AnchorCoord,
    pub y: AnchorCoord,
}

impl Anchor {
    pub const BOTTOM_LEFT: Anchor = Anchor {
        x: AnchorCoord::Zero,
        y: AnchorCoord::Zero,
    };
    pub const CENTER_LEFT: Anchor = Anchor {
        x: AnchorCoord::Zero,
        y: AnchorCoord::Center,
    };
    pub const TOP_LEFT: Anchor = Anchor {
        x: AnchorCoord::Zero,
        y: AnchorCoord::Max,
    };
    pub const BOTTOM_CENTER: Anchor = Anchor {
        x: AnchorCoord::Center,
        y: AnchorCoord::Zero,
    };
    pub const CENTER: Anchor = Anchor {
        x: AnchorCoord::Center,
        y: AnchorCoord::Center,
    };
    pub const TOP_CENTER: Anchor = Anchor {
        x: AnchorCoord::Center,
        y: AnchorCoord::Max,
    };
    pub const BOTTOM_RIGHT: Anchor = Anchor {
        x: AnchorCoord::Max,
        y: AnchorCoord::Zero,
    };
    pub const CENTER_RIGHT: Anchor = Anchor {
        x: AnchorCoord::Max,
        y: AnchorCoord::Center,
    };
    pub const TOP_RIGHT: Anchor = Anchor {
        x: AnchorCoord::Max,
        y: AnchorCoord::Max,
    };

    pub fn absolute_xy(x: i32, y: i32) -> Anchor {
        Anchor {
            x: AnchorCoord::Absolute(x),
            y: AnchorCoord::Absolute(y),
        }
    }

    pub fn to_xy(&self, width: u32, height: u32) -> (i32, i32) {
        (self.x.to_value(width), self.y.to_value(height))
    }
}

/// The position of a quad rendered on the screen.
#[derive(Clone, Copy, Debug)]
pub enum ScreenPosition {
    /// The quad is positioned at the exact coordinates provided.
    Absolute(Anchor),

    /// The quad is positioned relative to a reference point.
    Relative {
        anchor: Anchor,

        /// The offset along the x-axis from `reference_x`.
        x_ofs: i32,

        /// The offset along the y-axis from `reference_y`.
        y_ofs: i32,
    },
}

impl ScreenPosition {
    pub fn to_xy(&self, display_width: u32, display_height: u32) -> (i32, i32) {
        match *self {
            ScreenPosition::Absolute(Anchor {
                x: anchor_x,
                y: anchor_y,
            }) => (
                anchor_x.to_value(display_width),
                anchor_y.to_value(display_height),
            ),
            ScreenPosition::Relative {
                anchor:
                    Anchor {
                        x: anchor_x,
                        y: anchor_y,
                    },
                x_ofs,
                y_ofs,
            } => (
                anchor_x.to_value(display_width) + x_ofs,
                anchor_y.to_value(display_height) + y_ofs,
            ),
        }
    }
}
