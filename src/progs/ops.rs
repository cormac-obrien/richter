// Copyright © 2015 Cormac O'Brien.
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

#[derive(Debug, FromPrimitive)]
#[repr(u16)]
pub enum Opcode {
    Done = 0,
    MulF = 1,
    MulV = 2,
    MulFV = 3,
    MulVF = 4,
    Div = 5,
    AddF = 6,
    AddV = 7,
    SubF = 8,
    SubV = 9,
    EqF = 10,
    EqV = 11,
    EqS = 12,
    EqE = 13,
    EqFnc = 14,
    NeF = 15,
    NeV = 16,
    NeS = 17,
    NeE = 18,
    NeFnc = 19,
    Le = 20,
    Ge = 21,
    Lt = 22,
    Gt = 23,
    Indirect0 = 24,
    Indirect1 = 25,
    Indirect2 = 26,
    Indirect3 = 27,
    Indirect4 = 28,
    Indirect5 = 29,
    Address = 30,
    StoreF = 31,
    StoreV = 32,
    StoreS = 33,
    StoreEnt = 34,
    StoreFld = 35,
    StoreFnc = 36,
    StorePF = 37,
    StorePV = 38,
    StorePS = 39,
    StorePEnt = 40,
    StorePFld = 41,
    StorePFnc = 42,
    Return = 43,
    NotF = 44,
    NotV = 45,
    NotS = 46,
    NotEnt = 47,
    NotFnc = 48,
    If = 49,
    IfNot = 50,
    Call0 = 51,
    Call1 = 52,
    Call2 = 53,
    Call3 = 54,
    Call4 = 55,
    Call5 = 56,
    Call6 = 57,
    Call7 = 58,
    Call8 = 59,
    State = 60,
    Goto = 61,
    And = 62,
    Or = 63,
    BitAnd = 64,
    BitOr = 65,
}
