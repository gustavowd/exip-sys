#![allow(unsafe_op_in_unsafe_fn)]

use crate::decode::EXIP_LOCK;
use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;
use quick_xml::reader::NsReader;
use quick_xml::XmlVersion;
use std::os::raw::{c_char, c_int};

const XSI_NS: &str = "http://www.w3.org/2001/XMLSchema-instance";

#[repr(C)]
struct SepEnc {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn sep_enc_open() -> *mut SepEnc;
    fn sep_enc_start_element(e: *mut SepEnc, uri: *const c_char, ulen: usize, name: *const c_char, nlen: usize) -> c_int;
    fn sep_enc_attribute(
        e: *mut SepEnc,
        uri: *const c_char,
        ulen: usize,
        name: *const c_char,
        nlen: usize,
        val: *const c_char,
        vlen: usize,
    ) -> c_int;
    fn sep_enc_text(e: *mut SepEnc, t: *const c_char, n: usize) -> c_int;
    fn sep_enc_end_element(e: *mut SepEnc) -> c_int;
    fn sep_enc_finish(e: *mut SepEnc, out: *mut *mut u8, out_len: *mut usize) -> c_int;
    fn sep_enc_abort(e: *mut SepEnc);
    fn sep_enc_free_buf(p: *mut u8);
}

#[derive(Debug)]
pub enum EncodeError {
    /// XML inválido ou construção não suportada pelo transcodificador.
    Xml(String),
    /// Erro da EXIP (>0) ou do glue (-1000 tipo não suportado, -1001 valor inválido,
    /// -1002 memória, -1003 texto em elemento sem conteúdo simples), com o ponto onde ocorreu.
    Exip { code: i32, at: String },
}

struct Enc(*mut SepEnc);

impl Drop for Enc {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { sep_enc_abort(self.0) }
        }
    }
}

fn xml_err<E: std::fmt::Display>(e: E) -> EncodeError {
    EncodeError::Xml(e.to_string())
}

fn check(rc: c_int, at: &str) -> Result<(), EncodeError> {
    if rc == 0 {
        Ok(())
    } else {
        Err(EncodeError::Exip { code: rc, at: at.to_string() })
    }
}

fn ns_str<'a>(r: &'a ResolveResult<'_>) -> Result<&'a str, EncodeError> {
    match r {
        ResolveResult::Bound(n) => Ok(n.as_ref()),
        ResolveResult::Unbound => Ok(""),
        ResolveResult::Unknown(p) => Err(EncodeError::Xml(format!("prefixo de namespace desconhecido: {p}"))),
    }
}

/// Resolve referências numéricas (&#38; e &#x26;); o nome chega sem '&' e sem ';'.
fn char_ref(name: &str) -> Option<char> {
    let n = name.strip_prefix('#')?;
    let code = match n.strip_prefix('x') {
        Some(h) => u32::from_str_radix(h, 16).ok()?,
        None => n.parse().ok()?,
    };
    char::from_u32(code)
}

fn start(reader: &NsReader<&[u8]>, enc: &Enc, uri: &str, e: &BytesStart) -> Result<(), EncodeError> {
    let local = e.local_name();
    let name: &str = local.as_ref();
    let at = format!("<{name}>");
    check(
        unsafe {
            sep_enc_start_element(enc.0, uri.as_ptr() as *const c_char, uri.len(), name.as_ptr() as *const c_char, name.len())
        },
        &at,
    )?;

    // (namespace, nome local, valor): a EXIP exige atributos ordenados por nome local e depois namespace
    let mut attrs: Vec<(String, String, String)> = Vec::new();
    for a in e.attributes() {
        let a = a.map_err(xml_err)?;
        let raw: &str = a.key.as_ref();
        if raw == "xmlns" || raw.starts_with("xmlns:") {
            continue;
        }
        let (r, l) = reader.resolver().resolve_attribute(a.key);
        let auri = ns_str(&r)?.to_owned();
        if auri == XSI_NS {
            return Err(EncodeError::Xml("atributos xsi:* (xsi:type, xsi:nil) ainda não são suportados".into()));
        }
        let lname: &str = l.as_ref();
        let val = a.normalized_value(XmlVersion::Implicit1_0).map_err(xml_err)?.into_owned();
        attrs.push((auri, lname.to_owned(), val));
    }
    attrs.sort_by(|a, b| (&a.1, &a.0).cmp(&(&b.1, &b.0)));

    for (auri, aname, val) in &attrs {
        let at = format!("{at}@{aname}");
        check(
            unsafe {
                sep_enc_attribute(
                    enc.0,
                    auri.as_ptr() as *const c_char,
                    auri.len(),
                    aname.as_ptr() as *const c_char,
                    aname.len(),
                    val.as_ptr() as *const c_char,
                    val.len(),
                )
            },
            &at,
        )?;
    }
    Ok(())
}

/// Entrega o texto acumulado do elemento; texto só de espaços (indentação) é descartado.
fn flush(enc: &Enc, text: &mut String) -> Result<(), EncodeError> {
    if !text.trim().is_empty() {
        check(unsafe { sep_enc_text(enc.0, text.as_ptr() as *const c_char, text.len()) }, "texto")?;
    }
    text.clear();
    Ok(())
}

/// Codifica XML do 2030.5 em EXI (schema-informed, opções EXI padrão, sem options no header).
pub fn xml_to_exi(xml: &str) -> Result<Vec<u8>, EncodeError> {
    let _g = EXIP_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let p = unsafe { sep_enc_open() };
    if p.is_null() {
        return Err(EncodeError::Exip { code: -1, at: "sep_enc_open".into() });
    }
    let mut enc = Enc(p);

    let mut reader = NsReader::from_str(xml);
    let mut text = String::new();

    loop {
        let (ns, ev) = reader.read_resolved_event().map_err(xml_err)?;
        let uri = ns_str(&ns)?.to_owned(); // copia: encerra o empréstimo de `reader`
        match ev {
            Event::Start(e) => {
                flush(&enc, &mut text)?;
                start(&reader, &enc, &uri, &e)?;
            }
            Event::Empty(e) => {
                flush(&enc, &mut text)?;
                start(&reader, &enc, &uri, &e)?;
                check(unsafe { sep_enc_end_element(enc.0) }, "end")?;
            }
            Event::End(_) => {
                flush(&enc, &mut text)?;
                check(unsafe { sep_enc_end_element(enc.0) }, "end")?;
            }
            Event::Text(t) => text.push_str(&t),
            Event::CData(c) => text.push_str(&c),
            Event::GeneralRef(r) => {
                let name: &str = &r;
                if let Some(c) = char_ref(name) {
                    text.push(c);
                } else if let Some(s) = resolve_predefined_entity(name) {
                    text.push_str(s);
                } else {
                    return Err(EncodeError::Xml(format!("entidade desconhecida: &{name};")));
                }
            }
            Event::Eof => break,
            _ => {} // declaração XML, comentários, PI, DOCTYPE
        }
    }

    // sep_enc_finish libera o encoder em qualquer caso
    let p = std::mem::replace(&mut enc.0, std::ptr::null_mut());
    let mut out: *mut u8 = std::ptr::null_mut();
    let mut len: usize = 0;
    check(unsafe { sep_enc_finish(p, &mut out, &mut len) }, "finish")?;
    let v = if out.is_null() { Vec::new() } else { unsafe { std::slice::from_raw_parts(out, len) }.to_vec() };
    if !out.is_null() {
        unsafe { sep_enc_free_buf(out) };
    }
    Ok(v)
}
