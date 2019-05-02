// Copyright © 2019 Cormac O'Brien
//
// Permission is hereby granted, free of charge, to any person obtaining a copy of this software
// and associated documentation files (the "Software"), to deal in the Software without
// restriction, including without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all copies or
// substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING
// BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
// NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
// DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

pub enum Position {
    Absolute(u32),
    CenterRelative(i32),
}

pub enum LayoutElement {
    Bitmap {
        name: String,
        x: Position,
        y: Position,
    },
}

/// Defines the position of graphical elements that make up a Menu and the location of the cursor
/// for each selected element.
pub struct Layout {
    elements: Vec<LayoutElement>,
    cursor_pos: Vec<(u32, u32)>,
}

impl Layout {
    pub fn predefined<S>(gfx_name: S) -> Option<Layout>
    where
        S: AsRef<str>,
    {
        let name = gfx_name.as_ref();
        let layout = match name {
            "gfx/mainmenu.lmp" => Layout::bigmenu("gfx/ttl_main.lmp", "gfx/mainmenu.lmp", 5),
            "gfx/sp_menu.lmp" => Layout::bigmenu("gfx/ttl_sgl.lmp", "gfx/sp_menu.lmp", 3),
            "gfx/mp_menu.lmp" => Layout::bigmenu("gfx/p_multi.lmp", "gfx/mp_menu.lmp", 3),
            _ => return None,
        };

        Some(layout)
    }

    fn bigmenu<S>(title: S, menu: S, items: u32) -> Layout
    where
        S: AsRef<str>,
    {
        Layout {
            elements: vec![
                element_qplaque(),
                element_title(title),
                element_bigmenu(menu),
            ],

            cursor_pos: cursor_bigmenu(items),
        }
    }

    pub fn elements(&self) -> &[LayoutElement] {
        self.elements.as_slice()
    }
}

use self::Position::*;

// Quake "plaque" element
fn element_qplaque() -> LayoutElement {
    LayoutElement::Bitmap {
        name: "gfx/qplaque.lmp".to_string(),
        x: Absolute(16),
        y: Absolute(4),
    }
}

// menu title element
fn element_title<S>(name: S) -> LayoutElement
where
    S: AsRef<str>,
{
    LayoutElement::Bitmap {
        name: name.as_ref().to_owned(),
        x: CenterRelative(0),
        y: Absolute(4),
    }
}

// element for big-text menu items (Main, Single Player and Multiplayer)
fn element_bigmenu<S>(name: S) -> LayoutElement
where
    S: AsRef<str>,
{
    LayoutElement::Bitmap {
        name: name.as_ref().to_owned(),
        x: Absolute(54),
        y: Absolute(32),
    }
}

// generate the cursor positions for a bigmenu with n elements
fn cursor_bigmenu(n: u32) -> Vec<(u32, u32)> {
    (0..n).into_iter().map(|x| (54, 32 + 20 * x)).collect()
}
