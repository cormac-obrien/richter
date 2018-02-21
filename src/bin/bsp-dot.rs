// Copyright © 2018 Cormac O'Brien
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

extern crate richter;

use richter::common::bsp;
use richter::common::model;
use richter::common::pak;

fn main() {
    let mut pak = pak::Pak::new();
    pak.add("pak0.pak").unwrap();

    let (mut models, _) = bsp::load(pak.open("maps/e1m1.bsp").unwrap()).unwrap();
    let worldmodel = match models.remove(0) {
        model::Model {
            kind: model::ModelKind::Brush(bmodel),
            ..
        } => bmodel,
        _ => unreachable!(),
    };

    let dot_src = worldmodel.bsp_data().gen_dot_graph();
    println!("{}", dot_src);
}
