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

use std::cell::RefCell;

use failure::Error;

use self::item::{Enum, EnumItem, Item, Slider, TextField, Toggle};

#[derive(Clone)]
pub enum MenuState {
    /// Menu is inactive.
    Inactive,

    /// Menu is active. `index` indicates the currently selected element.
    Active { index: usize },

    /// A submenu of this menu is active. `index` indicates the active submenu.
    InSubMenu { index: usize },
}

struct Layout {
    _gfx_name: String,
    items: Vec<NamedMenuItem>,
    _cursor_pos: Vec<(u32, u32)>,
}

enum Items {
    List(Vec<NamedMenuItem>),
    Layout(Layout),
}

impl Items {
    pub fn item(&self, i: usize) -> &Item {
        match self {
            Items::List(ref list) => &list[i].item,
            Items::Layout(ref layout) => &layout.items[i].item,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Items::List(ref list) => list.len(),
            Items::Layout(ref layout) => layout.items.len(),
        }
    }
}

pub struct Menu {
    items: Items,
    state: RefCell<MenuState>,
}

impl Menu {
    /// Returns a reference to the active submenu of this menu and its parent.
    fn active_submenu_and_parent(&self) -> Result<(&Menu, Option<&Menu>), Error> {
        let mut m = self;
        let mut m_parent = None;

        while let MenuState::InSubMenu { index } = *m.state.borrow() {
            match m.items.item(index) {
                Item::Submenu(ref s) => {
                    m_parent = Some(m);
                    m = s;
                }
                _ => bail!("Menu state points to invalid submenu"),
            }
        }

        Ok((m, m_parent))
    }

    /// Return a reference to the active submenu of this menu
    fn active_submenu(&self) -> Result<&Menu, Error> {
        let (m, _) = self.active_submenu_and_parent()?;
        Ok(m)
    }

    /// Return a reference to the parent of the active submenu of this menu.
    ///
    /// If this is the root menu, returns None.
    fn active_submenu_parent(&self) -> Result<Option<&Menu>, Error> {
        let (_, m_parent) = self.active_submenu_and_parent()?;
        Ok(m_parent)
    }

    /// Select the next element of this Menu.
    pub fn next(&self) -> Result<(), Error> {
        let m = self.active_submenu()?;

        let s = m.state.borrow().clone();
        if let MenuState::Active { index } = s {
            m.state.replace(MenuState::Active {
                index: (index + 1) % m.items.len(),
            });
        } else {
            bail!("Selected menu is inactive (invariant violation)");
        }

        Ok(())
    }

    /// Select the previous element of this Menu.
    pub fn prev(&self) -> Result<(), Error> {
        let m = self.active_submenu()?;

        let s = m.state.borrow().clone();
        if let MenuState::Active { index } = s {
            m.state.replace(MenuState::Active {
                index: (index - 1) % m.items.len(),
            });
        } else {
            bail!("Selected menu is inactive (invariant violation)");
        }

        Ok(())
    }

    /// Return a reference to the currently selected menu item.
    pub fn selected(&self) -> Result<&Item, Error> {
        let m = self.active_submenu()?;

        if let MenuState::Active { index } = *m.state.borrow() {
            return Ok(&m.items.item(index));
        } else {
            bail!("Active menu in invalid state (invariant violation)")
        }
    }

    /// Activate the currently selected menu item.
    ///
    /// If this item is a `Menu`, sets the active (sub)menu's state to
    /// `MenuState::InSubMenu` and the selected submenu's state to
    /// `MenuState::Active`.
    ///
    /// If this item is an `Action`, executes the function contained in the
    /// `Action`.
    pub fn activate(&self) -> Result<(), Error> {
        let m = self.active_submenu()?;

        let s = m.state.borrow().clone();
        if let MenuState::Active { index } = s {
            match m.items.item(index) {
                Item::Submenu(ref submenu) => {
                    m.state.replace(MenuState::InSubMenu { index });
                    submenu.state.replace(MenuState::Active { index: 0 });
                }

                _ => unimplemented!(),
            }
        }

        Ok(())
    }

    /// Return `true` if the root menu is active, `false` otherwise.
    pub fn at_root(&self) -> bool {
        match *self.state.borrow() {
            MenuState::Active { .. } => true,
            _ => false,
        }
    }

    /// Deactivate the active menu and activate its parent
    pub fn back(&self) -> Result<(), Error> {
        if self.at_root() {
            bail!("Cannot back out of root menu!");
        }

        let (m, m_parent) = self.active_submenu_and_parent()?;
        m.state.replace(MenuState::Inactive);

        match m_parent {
            Some(mp) => {
                let s = mp.state.borrow().clone();
                match s {
                    MenuState::InSubMenu { index } => mp.state.replace(MenuState::Active { index }),
                    _ => unreachable!(),
                };
            }

            None => unreachable!(),
        }

        Ok(())
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
        // deactivate all child menus
        for item in self.items.iter() {
            if let Item::Submenu(ref m) = item.item {
                m.state.replace(MenuState::Inactive);
            }
        }

        Menu {
            items: Items::List(self.items),
            state: RefCell::new(MenuState::Active { index: 0 }),
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
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    fn is_inactive(state: &MenuState) -> bool {
        match state {
            MenuState::Inactive => true,
            _ => false,
        }
    }

    fn is_active(state: &MenuState) -> bool {
        match state {
            MenuState::Active { .. } => true,
            _ => false,
        }
    }

    fn is_insubmenu(state: &MenuState) -> bool {
        match state {
            MenuState::InSubMenu { .. } => true,
            _ => false,
        }
    }

    #[test]
    fn test_menu_builder() {
        let action_target = Rc::new(Cell::new(false));
        let action_target_handle = action_target.clone();

        let m = MenuBuilder::new()
            .add_action("action", Box::new(move || action_target_handle.set(true)))
            .build();

        // TODO
    }

    #[test]
    fn test_menu_active_submenu() {
        let menu = MenuBuilder::new()
            .add_submenu(
                "menu_1",
                MenuBuilder::new()
                    .add_action("action_1", Box::new(|| ()))
                    .build(),
            )
            .add_submenu(
                "menu_2",
                MenuBuilder::new()
                    .add_action("action_2", Box::new(|| ()))
                    .build(),
            )
            .build();

        let m = &menu;
        let m1 = match m.items.item(0) {
            Item::Submenu(ref m1i) => m1i,
            _ => unreachable!(),
        };
        let m2 = match m.items.item(1) {
            Item::Submenu(ref m2i) => m2i,
            _ => unreachable!(),
        };

        assert!(is_active(&m.state.borrow()));
        assert!(is_inactive(&m1.state.borrow()));
        assert!(is_inactive(&m2.state.borrow()));

        // enter m1
        m.activate().unwrap();
        assert!(is_insubmenu(&m.state.borrow()));
        assert!(is_active(&m1.state.borrow()));
        assert!(is_inactive(&m2.state.borrow()));

        // exit m1
        m.back().unwrap();
        assert!(is_active(&m.state.borrow()));
        assert!(is_inactive(&m1.state.borrow()));
        assert!(is_inactive(&m2.state.borrow()));

        // enter m2
        m.next().unwrap();
        m.activate().unwrap();
        assert!(is_insubmenu(&m.state.borrow()));
        assert!(is_inactive(&m1.state.borrow()));
        assert!(is_active(&m2.state.borrow()));
    }
}
