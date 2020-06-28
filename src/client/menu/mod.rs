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

pub use self::item::{Enum, EnumItem, Item, Slider, TextField, Toggle};

#[derive(Clone, Copy, Debug)]
pub enum MenuState {
    /// Menu is inactive.
    Inactive,

    /// Menu is active. `index` indicates the currently selected element.
    Active { index: usize },

    /// A submenu of this menu is active. `index` indicates the active submenu.
    InSubMenu { index: usize },
}

/// Specifies how the menu body should be rendered.
pub enum MenuBodyView {
    /// The menu body is rendered using a predefined bitmap.
    Predefined {
        /// The path to the bitmap.
        path: String,
    },
    /// The menu body is rendered dynamically based on its contents.
    Dynamic,
}

pub struct MenuView {
    pub draw_plaque: bool,
    pub title_path: String,
    pub body: MenuBodyView,
}

impl MenuView {
    /// Returns true if the Quake plaque should be drawn to the left of the menu.
    pub fn draw_plaque(&self) -> bool {
        self.draw_plaque
    }

    /// Returns the path to the menu title bitmap.
    pub fn title_path(&self) -> &str {
        &self.title_path
    }

    /// Returns a MenuBodyView which specifies how to render the menu body.
    pub fn body(&self) -> &MenuBodyView {
        &self.body
    }
}

pub struct Menu {
    items: Vec<NamedMenuItem>,
    state: Cell<MenuState>,
    view: MenuView,
}

impl Menu {
    /// Returns a reference to the active submenu of this menu and its parent.
    fn active_submenu_and_parent(&self) -> Result<(&Menu, Option<&Menu>), Error> {
        let mut m = self;
        let mut m_parent = None;

        while let MenuState::InSubMenu { index } = m.state.get() {
            match m.items[index].item {
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
    pub fn active_submenu(&self) -> Result<&Menu, Error> {
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

        let s = m.state.get().clone();
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

        let s = m.state.get().clone();
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

        if let MenuState::Active { index } = m.state.get() {
            return Ok(&m.items[index].item);
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
    ///
    /// Otherwise, this has no effect.
    pub fn activate(&self) -> Result<(), Error> {
        let m = self.active_submenu()?;

        if let MenuState::Active { index } = m.state.get() {
            match m.items[index].item {
                Item::Submenu(ref submenu) => {
                    m.state.replace(MenuState::InSubMenu { index });
                    submenu.state.replace(MenuState::Active { index: 0 });
                }

                Item::Action(ref action) => (action)(),

                _ => (),
            }
        }

        Ok(())
    }

    pub fn left(&self) -> Result<(), Error> {
        let m = self.active_submenu()?;

        if let MenuState::Active {index}= m.state.get() {
            match m.items[index].item {
                Item::Enum(ref e) => e.select_prev(),
                Item::Slider(ref slider) => slider.decrease(),
                Item::TextField(ref text) => text.cursor_left(),
                Item::Toggle(ref toggle) => toggle.set_false(),
                _ => (),
            }
        }

        Ok(())
    }

    pub fn right(&self) -> Result<(), Error> {
        let m = self.active_submenu()?;

        if let MenuState::Active {index}= m.state.get() {
            match m.items[index].item {
                Item::Enum(ref e) => e.select_next(),
                Item::Slider(ref slider) => slider.increase(),
                Item::TextField(ref text) => text.cursor_right(),
                Item::Toggle(ref toggle) => toggle.set_true(),
                _ => (),
            }
        }

        Ok(())
    }

    /// Return `true` if the root menu is active, `false` otherwise.
    pub fn at_root(&self) -> bool {
        match self.state.get() {
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
                let s = mp.state.get().clone();
                match s {
                    MenuState::InSubMenu { index } => mp.state.replace(MenuState::Active { index }),
                    _ => unreachable!(),
                };
            }

            None => unreachable!(),
        }

        Ok(())
    }

    pub fn items(&self) -> &[NamedMenuItem] {
        &self.items
    }

    pub fn state(&self) -> MenuState {
        self.state.get()
    }

    pub fn view(&self) -> &MenuView {
        &self.view
    }
}

pub struct MenuBuilder {
    gfx_name: Option<String>,
    items: Vec<NamedMenuItem>,
}

impl MenuBuilder {
    pub fn new() -> MenuBuilder {
        MenuBuilder {
            gfx_name: None,
            items: Vec::new(),
        }
    }

    pub fn build(self, view: MenuView) -> Menu {
        // deactivate all child menus
        for item in self.items.iter() {
            if let Item::Submenu(ref m) = item.item {
                m.state.replace(MenuState::Inactive);
            }
        }

        Menu {
            items: self.items,
            state: Cell::new(MenuState::Active { index: 0 }),
            view,
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

    pub fn add_action<S>(mut self, name: S, action: Box<dyn Fn()>) -> MenuBuilder
    where
        S: AsRef<str>,
    {
        self.items
            .push(NamedMenuItem::new(name, Item::Action(action)));
        self
    }

    pub fn add_toggle<S>(mut self, name: S, init: bool, on_toggle: Box<dyn Fn(bool)>) -> MenuBuilder
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
        on_select: Box<dyn Fn(f32)>,
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
        on_update: Box<dyn Fn(&str)>,
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

pub struct NamedMenuItem {
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

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn item(&self) -> &Item {
        &self.item
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    fn view() -> MenuView {
        MenuView {
            draw_plaque: false,
            title_path: "path".to_string(),
            body: MenuBodyView::Dynamic,
        }
    }

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

        let _m = MenuBuilder::new()
            .add_action("action", Box::new(move || action_target_handle.set(true)))
            .build(view());

        // TODO
    }

    #[test]
    fn test_menu_active_submenu() {
        let menu = MenuBuilder::new()
            .add_submenu(
                "menu_1",
                MenuBuilder::new()
                    .add_action("action_1", Box::new(|| ()))
                    .build(view()),
            )
            .add_submenu(
                "menu_2",
                MenuBuilder::new()
                    .add_action("action_2", Box::new(|| ()))
                    .build(view()),
            )
            .build(view());

        let m = &menu;
        let m1 = match m.items[0].item {
            Item::Submenu(ref m1i) => m1i,
            _ => unreachable!(),
        };
        let m2 = match m.items[1].item {
            Item::Submenu(ref m2i) => m2i,
            _ => unreachable!(),
        };

        assert!(is_active(&m.state.get()));
        assert!(is_inactive(&m1.state.get()));
        assert!(is_inactive(&m2.state.get()));

        // enter m1
        m.activate().unwrap();
        assert!(is_insubmenu(&m.state.get()));
        assert!(is_active(&m1.state.get()));
        assert!(is_inactive(&m2.state.get()));

        // exit m1
        m.back().unwrap();
        assert!(is_active(&m.state.get()));
        assert!(is_inactive(&m1.state.get()));
        assert!(is_inactive(&m2.state.get()));

        // enter m2
        m.next().unwrap();
        m.activate().unwrap();
        assert!(is_insubmenu(&m.state.get()));
        assert!(is_inactive(&m1.state.get()));
        assert!(is_active(&m2.state.get()));
    }
}
