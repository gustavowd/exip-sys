#[test]
fn roundtrip() {
    let path = std::env::var("EXI_SAMPLE").expect("defina EXI_SAMPLE=/caminho/response.exi");
    let original = std::fs::read(&path).unwrap();
    let xml1 = exip_sys::exi_to_xml(&original).unwrap();
    let exi = exip_sys::xml_to_exi(&xml1).expect("encode falhou");
    let xml2 = exip_sys::exi_to_xml(&exi).expect("decode do re-encode falhou");
    assert_eq!(xml1, xml2);
    println!(
        "original: {} bytes, recodificado: {} bytes, idênticos: {}",
        original.len(),
        exi.len(),
        original == exi
    );
    std::fs::write("target/roundtrip.exi", &exi).unwrap();
}
