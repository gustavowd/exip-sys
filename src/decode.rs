#![allow(unsafe_op_in_unsafe_fn)]

use std::borrow::Cow;
use std::os::raw::{c_char, c_int, c_void};

#[repr(C)]
struct SepSink {
    user: *mut c_void,
    start_element: unsafe extern "C" fn(*mut c_void, *const c_char, usize, *const c_char, usize) -> c_int,
    end_element: unsafe extern "C" fn(*mut c_void) -> c_int,
    attribute: unsafe extern "C" fn(*mut c_void, *const c_char, usize, *const c_char, usize) -> c_int,
    text: unsafe extern "C" fn(*mut c_void, *const c_char, usize) -> c_int,
    binary: unsafe extern "C" fn(*mut c_void, *const u8, usize) -> c_int,
}

unsafe extern "C" {
    fn sep_exi_decode(data: *mut u8, len: usize, sink: *const SepSink, out_of_band: c_int) -> c_int;
}

/// Código de erro da EXIP (veja errorHandle.h; 7 = header inválido, 15 = opções divergentes).
#[derive(Debug)]
pub struct ExiError(pub i32);

struct XmlOut {
    out: String,
    open_tag: bool,
    in_attr: bool,
    stack: Vec<(String, String)>, // (nome, namespace padrão em escopo)
    tag_prefixes: Vec<String>,
}

impl XmlOut {
    fn close_open_tag(&mut self) {
        if self.open_tag {
            self.out.push('>');
            self.open_tag = false;
        }
    }

    fn push_escaped(&mut self, s: &str, attr: bool) {
        for c in s.chars() {
            match c {
                '&' => self.out.push_str("&amp;"),
                '<' => self.out.push_str("&lt;"),
                '>' => self.out.push_str("&gt;"),
                '"' if attr => self.out.push_str("&quot;"),
                _ => self.out.push(c),
            }
        }
    }

    fn value(&mut self, t: &str) {
        if self.in_attr {
            self.push_escaped(t, true);
            self.out.push('"');
            self.in_attr = false;
        } else {
            self.close_open_tag();
            self.push_escaped(t, false);
        }
    }
}

unsafe fn lossy<'a>(p: *const c_char, n: usize) -> Cow<'a, str> {
    if p.is_null() || n == 0 {
        return Cow::Borrowed("");
    }
    String::from_utf8_lossy(std::slice::from_raw_parts(p as *const u8, n))
}

unsafe extern "C" fn cb_start(
    user: *mut c_void,
    uri: *const c_char,
    ulen: usize,
    name: *const c_char,
    nlen: usize,
) -> c_int {
    let x = &mut *(user as *mut XmlOut);
    let uri = lossy(uri, ulen).into_owned();
    let name = lossy(name, nlen).into_owned();
    x.close_open_tag();
    let parent_ns = x.stack.last().map(|(_, ns)| ns.clone()).unwrap_or_default();
    x.out.push('<');
    x.out.push_str(&name);
    if uri != parent_ns {
        x.out.push_str(" xmlns=\"");
        x.push_escaped(&uri, true);
        x.out.push('"');
    }
    x.stack.push((name, uri));
    x.open_tag = true;
    x.tag_prefixes.clear();
    0
}

unsafe extern "C" fn cb_end(user: *mut c_void) -> c_int {
    let x = &mut *(user as *mut XmlOut);
    let Some((name, _)) = x.stack.pop() else { return 1 };
    if x.open_tag {
        x.out.push_str("/>");
        x.open_tag = false;
    } else {
        x.out.push_str("</");
        x.out.push_str(&name);
        x.out.push('>');
    }
    0
}

unsafe extern "C" fn cb_attr(
    user: *mut c_void,
    uri: *const c_char,
    ulen: usize,
    name: *const c_char,
    nlen: usize,
) -> c_int {
    let x = &mut *(user as *mut XmlOut);
    let uri = lossy(uri, ulen).into_owned();
    let name = lossy(name, nlen).into_owned();
    if !x.open_tag {
        return 1;
    }
    x.out.push(' ');
    if !uri.is_empty() {
        let idx = match x.tag_prefixes.iter().position(|u| *u == uri) {
            Some(i) => i,
            None => {
                x.tag_prefixes.push(uri.clone());
                let i = x.tag_prefixes.len() - 1;
                x.out.push_str(&format!("xmlns:p{i}=\""));
                x.push_escaped(&uri, true);
                x.out.push_str("\" ");
                i
            }
        };
        x.out.push_str(&format!("p{idx}:"));
    }
    x.out.push_str(&name);
    x.out.push_str("=\"");
    x.in_attr = true;
    0
}

unsafe extern "C" fn cb_text(user: *mut c_void, s: *const c_char, n: usize) -> c_int {
    let x = &mut *(user as *mut XmlOut);
    let t = lossy(s, n).into_owned();
    x.value(&t);
    0
}

// Assume xs:hexBinary (tipo binário usado pelo 2030.5); base64Binary não é distinguido aqui.
unsafe extern "C" fn cb_binary(user: *mut c_void, b: *const u8, n: usize) -> c_int {
    let x = &mut *(user as *mut XmlOut);
    let bytes: &[u8] = if b.is_null() || n == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(b, n)
    };
    let hex: String = bytes.iter().map(|v| format!("{v:02X}")).collect();
    x.value(&hex);
    0
}

pub(crate) static EXIP_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Decodifica um stream EXI do 2030.5 (opções EXI padrão, fora de banda) para texto XML.
pub fn exi_to_xml(exi: &[u8]) -> Result<String, ExiError> {
    let _g = EXIP_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut data = exi.to_vec();
    let mut x = XmlOut {
        out: String::new(),
        open_tag: false,
        in_attr: false,
        stack: Vec::new(),
        tag_prefixes: Vec::new(),
    };
    let sink = SepSink {
        user: &mut x as *mut XmlOut as *mut c_void,
        start_element: cb_start,
        end_element: cb_end,
        attribute: cb_attr,
        text: cb_text,
        binary: cb_binary,
    };
    let rc = unsafe { sep_exi_decode(data.as_mut_ptr(), data.len(), &sink, 1) };
    if rc == 0 { Ok(x.out) } else { Err(ExiError(rc)) }
}
