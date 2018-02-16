// Copyright © 2018 Cormac O'Brien.
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
extern crate rodio;

use std::io::BufReader;
use std::io::Cursor;

use richter::common::pak;
use rodio::Source;

fn main() {
    let mut pak = richter::common::pak::Pak::new();
    pak.add("pak0.pak").unwrap();
    let comp1 = pak.open("sound/knight/sword1.wav").unwrap().to_owned();

    let endpoint = rodio::get_endpoints_list().next().unwrap();
    println!("Using endpoint {}", endpoint.get_name());

    let source = rodio::Decoder::new(BufReader::new(Cursor::new(comp1))).unwrap();
    println!("Source duration: {:?}", source.total_duration().unwrap());
    println!("Source sample rate: {:?}Hz", source.samples_rate());

    rodio::play_raw(&endpoint, source.convert_samples().amplify(64.0));

    std::thread::sleep_ms(10000);
}
