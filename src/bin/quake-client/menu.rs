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

use richter::client::menu::{Menu, MenuBodyView, MenuBuilder, MenuView};

use failure::Error;

pub fn build_main_menu() -> Result<Menu, Error> {
    Ok(MenuBuilder::new()
        .add_submenu("Single Player", build_menu_sp()?)
        .add_submenu("Multiplayer", build_menu_mp()?)
        .add_submenu("Options", build_menu_options()?)
        .add_action("Help/Ordering", Box::new(|| ()))
        .add_action("Quit", Box::new(|| ()))
        .build(MenuView {
            draw_plaque: true,
            title_path: "gfx/ttl_main.lmp".to_string(),
            body: MenuBodyView::Predefined {
                path: "gfx/mainmenu.lmp".to_string(),
            },
        }))
}

fn build_menu_sp() -> Result<Menu, Error> {
    Ok(MenuBuilder::new()
        .add_action("New Game", Box::new(|| ()))
        // .add_submenu("Load", unimplemented!())
        // .add_submenu("Save", unimplemented!())
        .build(MenuView {
            draw_plaque: true,
            title_path: "gfx/ttl_sgl.lmp".to_string(),
            body: MenuBodyView::Predefined {
                path: "gfx/sp_menu.lmp".to_string(),
            },
        }))
}

fn build_menu_mp() -> Result<Menu, Error> {
    Ok(MenuBuilder::new()
        .add_submenu("Join a Game", build_menu_mp_join()?)
        // .add_submenu("New Game", unimplemented!())
        // .add_submenu("Setup", unimplemented!())
        .build(MenuView {
            draw_plaque: true,
            title_path: "gfx/p_multi.lmp".to_string(),
            body: MenuBodyView::Predefined {
                path: "gfx/mp_menu.lmp".to_string(),
            },
        }))
}

fn build_menu_mp_join() -> Result<Menu, Error> {
    Ok(MenuBuilder::new()
        .add_submenu("TCP", build_menu_mp_join_tcp()?)
        // .add_textbox // description
        .build(MenuView {
            draw_plaque: true,
            title_path: "gfx/p_multi.lmp".to_string(),
            body: MenuBodyView::Predefined {
                path: "gfx/mp_menu.lmp".to_string(),
            },
        }))
}

fn build_menu_mp_join_tcp() -> Result<Menu, Error> {
    // Join Game - TCP/IP          // title
    //
    //  Address: 127.0.0.1         // label
    //
    //  Port     [26000]           // text field
    //
    //  Search for local games...  // menu
    //
    //  Join game at:              // label
    //  [                        ] // text field
    Ok(MenuBuilder::new()
        // .add
        .add_toggle("placeholder", false, Box::new(|_| ()))
        .build(MenuView {
            draw_plaque: true,
            title_path: "gfx/p_multi.lmp".to_string(),
            body: MenuBodyView::Dynamic,
        }))
}

fn build_menu_options() -> Result<Menu, Error> {
    Ok(MenuBuilder::new()
        // .add_submenu("Customize controls", unimplemented!())
        .add_action("Go to console", Box::new(|| ()))
        .add_action("Reset to defaults", Box::new(|| ()))
        .add_slider("Render scale", 0.25, 1.0, 2, 0, Box::new(|_| ()))?
        .add_slider("Screen Size", 0.0, 1.0, 10, 9, Box::new(|_| ()))?
        .add_slider("Brightness", 0.0, 1.0, 10, 9, Box::new(|_| ()))?
        .add_slider("Mouse Speed", 0.0, 1.0, 10, 9, Box::new(|_| ()))?
        .add_slider("CD music volume", 0.0, 1.0, 10, 9, Box::new(|_| ()))?
        .add_slider("Sound volume", 0.0, 1.0, 10, 9, Box::new(|_| ()))?
        .add_toggle("Always run", true, Box::new(|_| ()))
        .add_toggle("Invert mouse", false, Box::new(|_| ()))
        .add_toggle("Lookspring", false, Box::new(|_| ()))
        .add_toggle("Lookstrafe", false, Box::new(|_| ()))
        // .add_submenu("Video options", unimplemented!())
        .build(MenuView {
            draw_plaque: true,
            title_path: "gfx/p_option.lmp".to_string(),
            body: MenuBodyView::Dynamic,
        }))
}
