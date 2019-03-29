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

// XXX: most of this code is placeholder and will not compile

use richter::client::menu::{Menu, MenuBuilder};

pub fn build_main_menu() -> Menu {
    MenuBuilder::new()
        .add_submenu("Single Player", build_menu_sp())
        .add_submenu("Multiplayer", build_menu_mp())
        .add_submenu("Options", build_menu_options())
        .add_action("Help/Ordering")
        .add_action("Quit")
        .build()
}

fn build_menu_sp() -> Menu {
    MenuBuilder::new()
        .add_action("New Game")
        // .add_submenu("Load", unimplemented!())
        // .add_submenu("Save", unimplemented!())
        .build()
}

fn build_menu_mp() -> Menu {
    MenuBuilder::new()
        .add_submenu("Join a Game", build_menu_mp_join())
        // .add_submenu("New Game", unimplemented!())
        // .add_submenu("Setup", unimplemented!())
        .build()
}

fn build_menu_mp_join() -> Menu {
    MenuBuilder::new()
        // .add_submenu("IPX", unimplemented!()) // this is always disabled -- remove?
        .add_submenu("TCP", build_menu_mp_join_tcp())
        // .add_textbox // description
        .build()
}

fn build_menu_mp_join_tcp() -> Menu {
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
    MenuBuilder::new()
        // .add
        .build()
}

fn build_menu_options() -> Menu {
    MenuBuilder::new()
        // .add_submenu("Customize controls", unimplemented!())
        .add_action("Go to console")
        .add_action("Reset to defaults")
        .add_slider("Render scale")
        .add_slider("Screen Size")
        .add_slider("Brightness")
        .add_slider("Mouse Speed")
        .add_slider("CD music volume")
        .add_slider("Sound volume")
        .add_toggle("Always run")
        .add_toggle("Invert mouse")
        .add_toggle("Lookspring")
        .add_toggle("Lookstrafe")
        // .add_submenu("Video options", unimplemented!())
        .build()
}
