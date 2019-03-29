// Copyright © 2018 Cormac O'Brien
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

mod item;

use std::cell::Cell;

use failure::Error;

use self::item::{Enum, EnumItem, Item, Slider, TextField, Toggle};

pub enum MenuState {
    /// Menu is inactive.
    Inactive,

    /// Menu is active. `index` indicates the currently selected element.
    Active { index: usize },

    /// A submenu of this menu is active. `index` indicates the active submenu.
    InSubMenu { index: usize },
}

pub struct Menu {
    selected: Cell<usize>,
    items: Vec<NamedMenuItem>,
    state: MenuState,
}

impl Menu {
    /// Select the next element of this Menu.
    pub fn next(&self) {
        let s = self.selected.get();
        self.selected.set((s + 1) % self.items.len());
    }

    /// Select the previous element of this Menu.
    pub fn prev(&self) {
        let s = self.selected.get();
        self.selected.get((s - 1) % self.items.len())
    }

    pub fn selected(&self) {
        &self.items[self.selected.get()]
    }
}

pub struct MenuBuilder {
    items: Vec<NamedMenuItem>,
}

impl MenuBuilder {
    pub fn new() -> MenuBuilder {
        MenuBuilder { items: Vec::new() }
    }

    pub fn build(self) -> Menu {
        Menu {
            selected: Cell::new(0),
            items: self.items,
        }
    }

    pub fn add_submenu<S>(mut self, name: S, submenu: Menu) -> MenuBuilder
    where
        S: AsRef<str>,
    {
        self.items
            .push(NamedMenuItem::new(name, Item::Submenu(submenu)));
        self
    }

    pub fn add_action<S>(mut self, name: S, action: Box<Fn()>) -> MenuBuilder
    where
        S: AsRef<str>,
    {
        self.items
            .push(NamedMenuItem::new(name, Item::Action(action)));
        self
    }

    pub fn add_toggle<S>(mut self, name: S, init: bool, on_toggle: Box<Fn(bool)>) -> MenuBuilder
    where
        S: AsRef<str>,
    {
        self.items.push(NamedMenuItem::new(
            name,
            Item::Toggle(Toggle::new(init, on_toggle)),
        ));
        self
    }

    pub fn add_enum<S, E>(mut self, name: S, items: E, init: usize) -> Result<MenuBuilder, Error>
    where
        S: AsRef<str>,
        E: Into<Vec<EnumItem>>,
    {
        self.items.push(NamedMenuItem::new(
            name,
            Item::Enum(Enum::new(init, items.into())?),
        ));
        Ok(self)
    }

    pub fn add_slider<S>(
        mut self,
        name: S,
        min: f32,
        max: f32,
        steps: usize,
        init: usize,
        on_select: Box<Fn(f32)>,
    ) -> Result<MenuBuilder, Error>
    where
        S: AsRef<str>,
    {
        self.items.push(NamedMenuItem::new(
            name,
            Item::Slider(Slider::new(min, max, steps, init, on_select)?),
        ));
        Ok(self)
    }

    pub fn add_text_field<S>(
        mut self,
        name: S,
        default: Option<S>,
        max_len: Option<usize>,
        on_update: Box<Fn(&str)>,
    ) -> Result<MenuBuilder, Error>
    where
        S: AsRef<str>,
    {
        self.items.push(NamedMenuItem::new(
            name,
            Item::TextField(TextField::new(default, max_len, on_update)?),
        ));
        Ok(self)
    }
}

struct NamedMenuItem {
    name: String,
    item: Item,
}

impl NamedMenuItem {
    fn new<S>(name: S, item: Item) -> NamedMenuItem
    where
        S: AsRef<str>,
    {
        NamedMenuItem {
            name: name.as_ref().to_string(),
            item,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn test_menu_builder() {
        let action_target = Rc::new(Cell::new(false));
        let action_target_handle = action_target.clone();

        let m = MenuBuilder::new()
            .add_action("action", Box::new(move || action_target_handle.set(true)))
            .build();

        // TODO
    }

}
