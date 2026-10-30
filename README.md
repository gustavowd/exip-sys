# exip-sys

Rust bindings for the EXIP C library, providing functions to decode and encode
EXI messages using the IEEE 2030.5 (SEP) schema.

## Schema and compatibility

The schema compiled into this library is the **IEEE 2030.5** standard schema,
in the `urn:ieee:std:2030.5:ns` namespace. Its corresponding schema-informed
grammar is in `grammar/sep_grammar.c` and is linked into the code by
`csrc/sep_schema_shim.c`.

This grammar is C code generated from the XSDs; it is not loaded dynamically.
Therefore, if the IEEE 2030.5 XSDs used by the project are changed or updated
to another revision, the grammar must be regenerated with an EXIP-compatible
tool and `grammar/sep_grammar.c` must be updated. Changing an XSD file alone
does not change the schema used by the library. The new grammar must match the
schema used by the other participants in the communication and must be
recompiled with the library. This repository does not include the XSDs or a
script for generating the grammar.

## Requirements

- Rust and Cargo;
- a C compiler compatible with the `cc` crate;
- libclang, required by `bindgen`;
- Git, to download the pinned EXIP source on the first build when `EXIP_DIR` is
  not set.

By default, `build.rs` clones
[`ekrich/exip`](https://github.com/ekrich/exip) and checks out commit
`7bdf82198f396ddc48a2b16e40eba6650c38c0c2`. The source is cached under Cargo's
build output directory, so subsequent builds reuse it. The initial build
requires network access.

To use an existing local EXIP checkout instead (for example, to build offline),
set `EXIP_DIR`. By default, `exipConfig.h` is expected in
`build/gcc/pc` under the EXIP source directory; set `EXIP_TARGET_DIR` if it is
located elsewhere:

```sh
export EXIP_DIR=/path/to/exip
export EXIP_TARGET_DIR=/path/to/exip/build/gcc/pc
cargo build
```

Both variables are optional when using the default download and configuration
location.

## Adding as a dependency

In another Rust project, add the local dependency to `Cargo.toml` (adjust the
path to match your directory layout):

```toml
[dependencies]
exip-sys = { path = "../exip-sys" }
```

The environment building the consuming project must also meet the build
requirements listed above.

## Usage

The public functions accept EXI bytes or XML text and return the result in a
`Result`:

- `exip_sys::exi_to_xml(&[u8]) -> Result<String, ExiError>`
- `exip_sys::xml_to_exi(&str) -> Result<Vec<u8>, EncodeError>`

Example converting in both directions:

```rust
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let exi = fs::read("message.exi")?;
    let xml = exip_sys::exi_to_xml(&exi).expect("failed to decode EXI");
    fs::write("message.xml", &xml)?;

    // The XML must be valid against the IEEE 2030.5 schema in use.
    let xml = fs::read_to_string("message.xml")?;
    let exi = exip_sys::xml_to_exi(&xml).expect("failed to encode XML");
    fs::write("encoded-message.exi", exi)?;

    Ok(())
}
```

The XML passed to `xml_to_exi` must conform to the compiled IEEE 2030.5 schema.
Encoding uses the default EXI options and does not write options into the
header; decoding also interprets the default options out of band. The other
side of the communication must use compatible options and the same schema.

## Current limitations

- Encoding does not support `xsi:*` attributes (`xsi:type` and `xsi:nil`).
- Encoding supports string, boolean, integer, and `hexBinary` values; other
  types may return an error.
- During decoding, binary values are written as hexadecimal. The routine does
  not distinguish `hexBinary` from `base64Binary`.
- Decoding XML Schema `xs:dateTime` values is not yet supported. IEEE 2030.5
  timestamp fields named `dateTime` use `TimeType` (integer seconds since the
  Unix epoch), not `xs:dateTime`.
- Encoding discards text nodes containing only whitespace, which are commonly
  used for indentation.

Check the returned `EncodeError` and `ExiError` values to handle XML errors,
unsupported values, and errors reported by EXIP.

## Tests

The encode/decode integration tests require an EXI sample file, specified by
the `EXI_SAMPLE` environment variable:

```sh
EXI_SAMPLE=/path/to/message.exi cargo test
```

To run only the library unit test, which checks that the schema was loaded:

```sh
cargo test --lib
```
