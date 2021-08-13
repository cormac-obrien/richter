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

use std::cell::{Cell, RefCell};

use crate::client::menu::Menu;

use failure::Error;

pub enum Item {
    Submenu(Menu),
    Action(Box<dyn Fn()>),
    Toggle(Toggle),
    Enum(Enum),
    Slider(Slider),
    TextField(TextField),
}

pub struct Toggle {
    state: Cell<bool>,
    on_toggle: Box<dyn Fn(bool)>,
}

impl Toggle {
    pub fn new(init: bool, on_toggle: Box<dyn Fn(bool)>) -> Toggle {
        let t = Toggle {
            state: Cell::new(init),
            on_toggle,
        };

        // initialize with default
        (t.on_toggle)(init);

        t
    }

    pub fn set_false(&self) {
        self.state.set(false);
        (self.on_toggle)(self.state.get());
    }

    pub fn set_true(&self) {
        self.state.set(true);
        (self.on_toggle)(self.state.get());
    }

    pub fn toggle(&self) {
        self.state.set(!self.state.get());
        (self.on_toggle)(self.state.get());
    }

    pub fn get(&self) -> bool {
        self.state.get()
    }
}

// TODO: add wrapping configuration to enums
// e.g. resolution enum wraps, texture filtering does not
pub struct Enum {
    selected: Cell<usize>,
    items: Vec<EnumItem>,
}

impl Enum {
    pub fn new(init: usize, items: Vec<EnumItem>) -> Result<Enum, Error> {
        ensure!(
            !items.is_empty(),
            "Enum element must have at least one item"
        );
        ensure!(init < items.len(), "Invalid initial item ID");

        let e = Enum {
            selected: Cell::new(init),
            items,
        };

        // initialize with the default choice
        (e.items[e.selected.get()].on_select)();

        Ok(e)
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

pub struct EnumItem {
    name: String,
    on_select: Box<dyn Fn()>,
}

impl EnumItem {
    pub fn new<S>(name: S, on_select: Box<dyn Fn()>) -> Result<EnumItem, Error>
    where
        S: AsRef<str>,
    {
        Ok(EnumItem {
            name: name.as_ref().to_string(),
            on_select,
        })
    }
}

pub struct Slider {
    min: f32,
    _max: f32,
    increment: f32,
    steps: usize,

    selected: Cell<usize>,
    on_select: Box<dyn Fn(f32)>,
}

impl Slider {
    pub fn new(
        min: f32,
        max: f32,
        steps: usize,
        init: usize,
        on_select: Box<dyn Fn(f32)>,
    ) -> Result<Slider, Error> {
        ensure!(steps > 1, "Slider must have at least 2 steps");
        ensure!(init < steps, "Invalid initial setting");
        ensure!(
            min < max,
            "Minimum setting must be less than maximum setting"
        );

        Ok(Slider {
            min,
            _max: max,
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

    pub fn position(&self) -> f32 {
        self.selected.get() as f32 / self.steps as f32
    }
}

pub struct TextField {
    chars: RefCell<Vec<char>>,
    max_len: Option<usize>,
    on_update: Box<dyn Fn(&str)>,
    cursor: Cell<usize>,
}

impl TextField {
    pub fn new<S>(
        default: Option<S>,
        max_len: Option<usize>,
        on_update: Box<dyn Fn(&str)>,
    ) -> Result<TextField, Error>
    where
        S: AsRef<str>,
    {
        let chars = RefCell::new(match default {
            Some(d) => d.as_ref().chars().collect(),
            None => Vec::new(),
        });

        let cursor = Cell::new(chars.borrow().len());

        Ok(TextField {
            chars,
            max_len,
            on_update,
            cursor,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn text(&self) -> String {
        self.chars.borrow().iter().collect()
    }

    pub fn len(&self) -> usize {
        self.chars.borrow().len()
    }

    pub fn set_cursor(&self, cursor: usize) -> Result<(), Error> {
        ensure!(cursor <= self.len(), "Index out of range");

        self.cursor.set(cursor);

        Ok(())
    }

    pub fn home(&self) {
        self.cursor.set(0);
    }

    pub fn end(&self) {
        self.cursor.set(self.len());
    }

    pub fn cursor_right(&self) {
        let curs = self.cursor.get();
        if curs < self.len() {
            self.cursor.set(curs + 1);
        }
    }

    pub fn cursor_left(&self) {
        let curs = self.cursor.get();
        if curs > 1 {
            self.cursor.set(curs - 1);
        }
    }

    pub fn insert(&self, c: char) {
        if let Some(l) = self.max_len {
            if self.len() == l {
                return;
            }
        }

        self.chars.borrow_mut().insert(self.cursor.get(), c);
        (self.on_update)(&self.text());
    }

    pub fn backspace(&self) {
        if self.cursor.get() > 1 {
            self.chars.borrow_mut().remove(self.cursor.get() - 1);
            (self.on_update)(&self.text());
        }
    }

    pub fn delete(&self) {
        if self.cursor.get() < self.len() {
            self.chars.borrow_mut().remove(self.cursor.get());
            (self.on_update)(&self.text());
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    #[test]
    fn test_toggle() {
        let s = Rc::new(RefCell::new("false".to_string()));

        let s2 = s.clone();
        let item = Toggle::new(
            false,
            Box::new(move |state| {
                s2.replace(format!("{}", state));
            }),
        );
        item.toggle();

        assert_eq!(*s.borrow(), "true");
    }

    #[test]
    fn test_enum() {
        let target = Rc::new(RefCell::new("null".to_string()));

        let enum_items = (0..3i32)
            .into_iter()
            .map(|i: i32| {
                let target_handle = target.clone();
                EnumItem::new(
                    format!("option_{}", i),
                    Box::new(move || {
                        target_handle.replace(format!("option_{}", i));
                    }),
                )
                .unwrap()
            })
            .collect();

        let e = Enum::new(0, enum_items).unwrap();
        assert_eq!(*target.borrow(), "option_0");

        // wrap under
        e.select_prev();
        assert_eq!(*target.borrow(), "option_2");

        e.select_next();
        e.select_next();
        e.select_next();
        assert_eq!(*target.borrow(), "option_2");

        // wrap over
        e.select_next();
        assert_eq!(*target.borrow(), "option_0");
    }

    #[test]
    fn test_slider() {
        let f = Rc::new(Cell::new(0.0f32));

        let f2 = f.clone();
        let item = Slider::new(
            0.0,
            10.0,
            11,
            0,
            Box::new(move |f| {
                f2.set(f);
            }),
        )
        .unwrap();

        // don't underflow
        item.decrease();
        assert_eq!(f.get(), 0.0);

        for i in 0..10 {
            item.increase();
            assert_eq!(f.get(), i as f32 + 1.0);
        }

        // don't overflow
        item.increase();
        assert_eq!(f.get(), 10.0);
    }

    #[test]
    fn test_textfield() {
        let max_len = 10;
        let s = Rc::new(RefCell::new("before".to_owned()));
        let s2 = s.clone();

        let tf = TextField::new(
            Some("default"),
            Some(max_len),
            Box::new(move |x| {
                s2.replace(x.to_string());
            }),
        )
        .unwrap();

        tf.cursor_left();
        tf.backspace();
        tf.backspace();
        tf.home();
        tf.delete();
        tf.delete();
        tf.delete();
        tf.cursor_right();
        tf.insert('f');
        tf.end();
        tf.insert('e');
        tf.insert('r');

        assert_eq!(tf.text(), *s.borrow());

        for _ in 0..2 * max_len {
            tf.insert('x');
        }

        assert_eq!(tf.len(), max_len);
    }
}
