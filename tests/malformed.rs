//! Malformed EXI must return an error, never crash the process.
//!
//! Inputs are valid EXI produced by `xml_to_exi`, then corrupted with every
//! single-bit flip and every truncation. A segfault fails the whole test binary.

const NS: &str = "urn:ieee:std:2030.5:ns";

fn samples() -> Vec<Vec<u8>> {
    [
        format!(r#"<Time xmlns="{NS}" href="/tm"><currentTime>1700000000</currentTime><dstEndTime>0</dstEndTime><dstOffset>3600</dstOffset><dstStartTime>0</dstStartTime><quality>7</quality><tzOffset>-10800</tzOffset></Time>"#),
        format!(r#"<EndDevice xmlns="{NS}" href="/edev/3" subscribable="0"><ConfigurationLink href="/edev/3/cfg"/><DeviceInformationLink href="/edev/3/di"/><sFDI>987654321005</sFDI><changedTime>1379905200</changedTime><FunctionSetAssignmentsListLink all="3" href="/edev/3/fsal"/><RegistrationLink href="/edev/3/reg"/></EndDevice>"#),
        format!(r#"<Subscription xmlns="{NS}"><subscribedResource>/upt/0/mr/4/r</subscribedResource><encoding>0</encoding><level>+S2</level><limit>1</limit><notificationURI>/note</notificationURI></Subscription>"#),
    ]
    .iter()
    .map(|xml| exip_sys::xml_to_exi(xml).expect("valid sample must encode"))
    .collect()
}

#[test]
fn valid_samples_round_trip() {
    for exi in samples() {
        exip_sys::exi_to_xml(&exi).expect("valid EXI must decode");
    }
}

#[test]
fn bit_flips_do_not_crash() {
    for exi in samples() {
        for i in 0..exi.len() * 8 {
            let mut bad = exi.clone();
            bad[i / 8] ^= 0x80 >> (i % 8);
            let _ = exip_sys::exi_to_xml(&bad);
        }
    }
}

#[test]
fn truncations_do_not_crash() {
    for exi in samples() {
        for len in 0..exi.len() {
            let _ = exip_sys::exi_to_xml(&exi[..len]);
        }
    }
}

#[test]
fn garbage_does_not_crash() {
    // Deterministic pseudo-random inputs (xorshift), with and without a valid header byte.
    let mut s: u32 = 0x2030_5005;
    for n in 0..2000 {
        let len = 1 + (n % 64) as usize;
        let mut buf: Vec<u8> = (0..len)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                s as u8
            })
            .collect();
        if n % 2 == 0 {
            buf[0] = 0x80;
        }
        let _ = exip_sys::exi_to_xml(&buf);
    }
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

/// Inputs found by fuzzing that crashed EXIP. Each one is decoded in order,
/// in the same process, because two of them only crash through state left
/// behind in the static grammar by a previous decode.
#[test]
fn regressions_do_not_crash() {
    let cases = [
        // Header options document adds a URI to the static ops_schema string
        // table: realloc() of a static array ("pointer being freed was not allocated").
        "bb8029ff8e5509afa0f99aed424eb83343d7d8a58ee366889d749e595d77b3844bb70e72c46f79196f398dcc71ef0850ac75ff48026a70",
        // First input links a static local name to a built-in grammar; the
        // second follows that grammar after it was freed (use-after-free).
        "809700a5ee8da901c534546b0001203800724335ea80",
        "805b2050600006cc800f476173204d6972726f72696e67201130020040080040a0e0000d990561f4810482079c000013fbccee20",
        // CH event at document level: value decoded with an empty QName (out-of-bounds read).
        "bb2af6198d84df1ebe49bf68aa35a1000ee0314b3bf9261094a962c4ffcab7945fcb7cd4e1502c2c12e6633b3bbb98f75a76b8abbe81baadd8f5f44de398591999ea39176ffd40ee7d6f0d6705d3242ce4",
    ];
    for case in cases {
        let _ = exip_sys::exi_to_xml(&hex(case));
    }
    for exi in samples() {
        exip_sys::exi_to_xml(&exi).expect("valid EXI must still decode after malformed input");
    }
}
