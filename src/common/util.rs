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

use std::mem::size_of;

/// A plain-old-data type.
pub trait Pod: 'static + Copy + Sized + Send + Sync {}
impl<T: 'static + Copy + Sized + Send + Sync> Pod for T {}

/// Read a null-terminated sequence of bytes and convert it into a `String`.
///
/// The zero byte is consumed.
///
/// ## Panics
/// - If the end of the input is reached before a zero byte is found.
pub fn read_cstring<R>(src: &mut R) -> Result<String, std::string::FromUtf8Error>
where
    R: std::io::BufRead,
{
    let mut bytes: Vec<u8> = Vec::new();
    src.read_until(0, &mut bytes).unwrap();
    bytes.pop();
    String::from_utf8(bytes)
}

pub unsafe fn any_as_bytes<T>(t: &T) -> &[u8] where T: Pod {
    std::slice::from_raw_parts((t as *const T) as *const u8, size_of::<T>())
}

pub unsafe fn any_slice_as_bytes<T>(t: &[T]) -> &[u8] where T: Pod {
    std::slice::from_raw_parts(t.as_ptr() as *const u8, size_of::<T>() * t.len())
}

pub unsafe fn bytes_as_any<T>(bytes: &[u8]) -> T where T: Pod {
    assert_eq!(bytes.len(), size_of::<T>());
    std::ptr::read_unaligned(bytes.as_ptr() as *const T)
}
