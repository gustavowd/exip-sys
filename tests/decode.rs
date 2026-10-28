
#[test]
fn decode_sample() {
    let path = std::env::var("EXI_SAMPLE").expect("defina EXI_SAMPLE=/caminho/completo/response.exi");
    let exi = std::fs::read(&path).unwrap_or_else(|e| panic!("não consegui ler {path}: {e}"));
    let xml = exip_sys::exi_to_xml(&exi).expect("decode falhou");
    println!("{xml}");
    assert!(xml.starts_with('<'));
}

#[test]
fn decode_repeatable() {
    let path = std::env::var("EXI_SAMPLE").unwrap();
    let exi = std::fs::read(&path).unwrap();
    let first = exip_sys::exi_to_xml(&exi).unwrap();
    for _ in 0..2000 {
        assert_eq!(exip_sys::exi_to_xml(&exi).unwrap(), first);
    }
}
