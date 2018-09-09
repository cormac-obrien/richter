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

use std::cell::Cell;

use failure::Error;

pub struct Menu {
    selected: Cell<usize>,
    items: Vec<NamedMenuItem>,
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
            .push(NamedMenuItem::new(name, MenuItem::Submenu(submenu)));
        self
    }

    pub fn add_action<S>(mut self, name: S, action: Box<Fn()>) -> MenuBuilder
    where
        S: AsRef<str>,
    {
        self.items
            .push(NamedMenuItem::new(name, MenuItem::Action(action)));
        self
    }

    pub fn add_toggle<S>(mut self, name: S, init: bool, on_toggle: Box<Fn(bool)>) -> MenuBuilder
    where
        S: AsRef<str>,
    {
        self.items.push(NamedMenuItem::new(
            name,
            MenuItem::Toggle(MenuItemToggle::new(init, on_toggle)),
        ));
        self
    }
}

struct NamedMenuItem {
    name: String,
    item: MenuItem,
}

impl NamedMenuItem {
    fn new<S>(name: S, item: MenuItem) -> NamedMenuItem
    where
        S: AsRef<str>,
    {
        NamedMenuItem {
            name: name.as_ref().to_string(),
            item,
        }
    }
}

enum MenuItem {
    Submenu(Menu),
    Action(Box<Fn()>),
    Toggle(MenuItemToggle),
    Enum,
}

struct MenuItemToggle {
    state: Cell<bool>,
    on_toggle: Box<Fn(bool)>,
}

impl MenuItemToggle {
    fn new(init: bool, on_toggle: Box<Fn(bool)>) -> MenuItemToggle {
        MenuItemToggle {
            state: Cell::new(init),
            on_toggle,
        }
    }

    fn toggle(&self) {
        self.state.set(!self.state.get());
        (self.on_toggle)(self.state.get());
    }
}

pub struct MenuItemEnum {
    selected: Cell<usize>,
    items: Vec<MenuItemEnumItem>,
}

impl MenuItemEnum {
    pub fn new(init: usize, items: Vec<MenuItemEnumItem>) -> Result<MenuItemEnum, Error> {
        ensure!(items.len() > 0, "Enum element must have at least one item");
        ensure!(init < items.len(), "Invalid initial item ID");

        Ok(MenuItemEnum {
            selected: Cell::new(init),
            items,
        })
    }

    pub fn selected_name(&self) -> &str {
        self.items[self.selected.get()].name.as_str()
    }

    pub fn select_next(&self) {
        let selected = match self.selected.get() + 1 {
            s if s >= self.items.len() => 0,
            s => s,
        };

        self.selected.set(selected);
        (self.items[selected].on_select)();
    }

    pub fn select_prev(&self) {
        let selected = match self.selected.get() {
            0 => self.items.len() - 1,
            s => s - 1,
        };

        self.selected.set(selected);
        (self.items[selected].on_select)();
    }
}

pub struct MenuItemEnumItem {
    name: String,
    on_select: Box<Fn()>,
}

impl MenuItemEnumItem {
    pub fn new<S>(name: S, on_select: Box<Fn()>) -> Result<MenuItemEnumItem, Error>
    where
        S: AsRef<str>,
    {
        Ok(MenuItemEnumItem {
            name: name.as_ref().to_string(),
            on_select,
        })
    }
}

pub struct MenuItemSlider {
    min: f32,
    max: f32,
    increment: f32,
    steps: usize,

    selected: Cell<usize>,
    on_select: Box<Fn(f32)>,
}

impl MenuItemSlider {
    pub fn new(
        min: f32,
        max: f32,
        steps: usize,
        init: usize,
        on_select: Box<Fn(f32)>,
    ) -> Result<MenuItemSlider, Error> {
        ensure!(steps > 1, "Slider must have at least 2 steps");
        ensure!(init < steps, "Invalid initial setting");
        ensure!(
            min < max,
            "Minimum setting must be less than maximum setting"
        );

        Ok(MenuItemSlider {
            min,
            max,
            increment: (max - min) / (steps - 1) as f32,
            steps,
            selected: Cell::new(init),
            on_select,
        })
    }

    pub fn increase(&self) {
        let old = self.selected.get();

        if old != self.steps - 1 {
            self.selected.set(old + 1);
        }

        (self.on_select)(self.min + self.selected.get() as f32 * self.increment);
    }

    pub fn decrease(&self) {
        let old = self.selected.get();

        if old != 0 {
            self.selected.set(old - 1);
        }

        (self.on_select)(self.min + self.selected.get() as f32 * self.increment);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn test_menu_item_toggle() {
        let s = Rc::new(RefCell::new("false".to_string()));

        let s2 = s.clone();
        let item = MenuItemToggle::new(
            false,
            Box::new(move |state| {
                s2.replace(format!("{}", state));
            }),
        );
        item.toggle();

        assert_eq!(*s.borrow(), "true");
    }

    #[test]
    fn test_menu_item_enum() {
        let s = Rc::new(RefCell::new("first".to_string()));
        let (s2, s3, s4) = (s.clone(), s.clone(), s.clone());

        let item = MenuItemEnum::new(
            0,
            vec![
                MenuItemEnumItem::new(
                    "first",
                    Box::new(move || {
                        s2.replace("first".to_string());
                    }),
                ).unwrap(),
                MenuItemEnumItem::new(
                    "second",
                    Box::new(move || {
                        s3.replace("second".to_string());
                    }),
                ).unwrap(),
                MenuItemEnumItem::new(
                    "third",
                    Box::new(move || {
                        s4.replace("third".to_string());
                    }),
                ).unwrap(),
            ],
        ).unwrap();

        item.select_prev();
        assert_eq!(s.borrow().as_str(), "third");

        item.select_prev();
        assert_eq!(s.borrow().as_str(), "second");

        item.select_next();
        assert_eq!(s.borrow().as_str(), "third");

        item.select_next();
        assert_eq!(s.borrow().as_str(), "first");
    }

    #[test]
    fn test_menu_item_slider() {
        let f = Rc::new(Cell::new(0.0f32));

        let f2 = f.clone();
        let item = MenuItemSlider::new(
            0.0,
            10.0,
            11,
            0,
            Box::new(move |f| {
                f2.set(f);
            }),
        ).unwrap();

        item.decrease();
        assert_eq!(f.get(), 0.0);

        for i in 0..10 {
            item.increase();
            assert_eq!(f.get(), i as f32 + 1.0);
        }

        item.increase();
        assert_eq!(f.get(), 10.0);
    }
}
