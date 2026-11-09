//! Compiled-artifact fingerprinting: ELF, PE and Mach-O.
//!
//! c2r does not decompile. It recovers what a re-implementation needs to know:
//! format, architecture, the toolchain that produced it, linked libraries,
//! imported/exported symbols and the interesting strings (URLs, paths, SQL,
//! format strings) that reveal behaviour.

use crate::model::BinaryInfo;
use goblin::Object;

const MAX_LIST: usize = 60;

pub fn looks_like_binary(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x7fELF")
        || bytes.starts_with(b"MZ")
        || bytes.starts_with(&[0xfe, 0xed, 0xfa, 0xce])
        || bytes.starts_with(&[0xfe, 0xed, 0xfa, 0xcf])
        || bytes.starts_with(&[0xce, 0xfa, 0xed, 0xfe])
        || bytes.starts_with(&[0xcf, 0xfa, 0xed, 0xfe])
        || bytes.starts_with(&[0xca, 0xfe, 0xba, 0xbe]) && bytes.len() > 8 && bytes[7] < 20
    // fat Mach-O, not a .class file
}

pub fn analyze(rel: &str, bytes: &[u8]) -> BinaryInfo {
    let mut info = BinaryInfo {
        path: rel.to_string(),
        size: bytes.len() as u64,
        ..Default::default()
    };
    match Object::parse(bytes) {
        Ok(Object::Elf(elf)) => {
            info.format = "ELF".into();
            info.bits = if elf.is_64 { 64 } else { 32 };
            info.arch = goblin::elf::header::machine_to_str(elf.header.e_machine)
                .trim_start_matches("EM_")
                .to_string();
            info.libraries = elf.libraries.iter().map(|s| s.to_string()).collect();
            for sym in elf.dynsyms.iter() {
                let Some(name) = elf.dynstrtab.get_at(sym.st_name) else {
                    continue;
                };
                if name.is_empty() {
                    continue;
                }
                if sym.is_import() {
                    info.imported_symbols.push(name.to_string());
                } else if sym.st_bind() == goblin::elf::sym::STB_GLOBAL && sym.st_value != 0 {
                    info.exported_symbols.push(name.to_string());
                }
            }
        }
        Ok(Object::PE(pe)) => {
            info.format = if pe.is_lib {
                "PE (DLL)".into()
            } else {
                "PE (EXE)".into()
            };
            info.bits = if pe.is_64 { 64 } else { 32 };
            info.arch = match pe.header.coff_header.machine {
                0x8664 => "x86_64".into(),
                0x14c => "x86".into(),
                0xaa64 => "ARM64".into(),
                m => format!("0x{m:x}"),
            };
            info.libraries = pe.libraries.iter().map(|s| s.to_string()).collect();
            info.imported_symbols = pe
                .imports
                .iter()
                .map(|i| format!("{}!{}", i.dll, i.name))
                .collect();
            info.exported_symbols = pe
                .exports
                .iter()
                .filter_map(|e| e.name.map(str::to_string))
                .collect();
            let clr = pe
                .header
                .optional_header
                .and_then(|o| o.data_directories.get_clr_runtime_header().copied())
                .is_some_and(|d| d.virtual_address != 0);
            if clr {
                info.toolchain
                    .push(".NET (CLR assembly) - decompile with ILSpy/dnSpy for source".into());
            }
        }
        Ok(Object::Mach(mach)) => {
            info.format = "Mach-O".into();
            let single = match mach {
                goblin::mach::Mach::Binary(m) => Some(m),
                goblin::mach::Mach::Fat(fat) => {
                    info.format = "Mach-O (universal)".into();
                    fat.into_iter().find_map(|a| match a {
                        Ok(goblin::mach::SingleArch::MachO(m)) => Some(m),
                        _ => None,
                    })
                }
            };
            if let Some(m) = single {
                info.bits = if m.is_64 { 64 } else { 32 };
                info.arch = match m.header.cputype() {
                    goblin::mach::cputype::CPU_TYPE_X86_64 => "x86_64".into(),
                    goblin::mach::cputype::CPU_TYPE_ARM64 => "ARM64".into(),
                    c => format!("cpu {c}"),
                };
                info.libraries = m
                    .libs
                    .iter()
                    .filter(|l| **l != "self")
                    .map(|s| s.to_string())
                    .collect();
                if let Ok(imports) = m.imports() {
                    info.imported_symbols = imports.iter().map(|i| i.name.to_string()).collect();
                }
                if let Ok(exports) = m.exports() {
                    info.exported_symbols = exports.iter().map(|e| e.name.clone()).collect();
                }
            }
        }
        _ => {
            info.format = "unknown".into();
        }
    }

    // Toolchain fingerprints from raw bytes.
    let has = |needle: &[u8]| bytes.windows(needle.len()).any(|w| w == needle);
    let probes: &[(&[u8], &str)] = &[
        (b"Go build ID:", "Go"),
        (b"runtime.gopanic", "Go"),
        (b"/rustc/", "Rust"),
        (b"rust_panic", "Rust"),
        (b"GCC: (", "GCC (C/C++)"),
        (b"clang version", "Clang/LLVM (C/C++/ObjC/Swift)"),
        (b"libstdc++", "C++ (libstdc++)"),
        (b"MSVCP", "C++ (MSVC runtime)"),
        (b"VCRUNTIME", "MSVC (C/C++)"),
        (
            b"_MEIPASS",
            "Python (PyInstaller bundle) - extract with pyinstxtractor",
        ),
        (
            b"PyInstaller",
            "Python (PyInstaller bundle) - extract with pyinstxtractor",
        ),
        (b"Py_Initialize", "Embeds CPython"),
        (b"JNI_OnLoad", "JNI native library (Java/Android)"),
        (b"Qt5Core", "Qt 5"),
        (b"Qt6Core", "Qt 6"),
        (
            b"electron.asar",
            "Electron app - unpack app.asar for JS source",
        ),
        (b"node::Start", "Node.js runtime (pkg/nexe bundle)"),
        (b"swift_retain", "Swift"),
        (b"_objc_msgSend", "Objective-C"),
        (b"Delphi", "Delphi"),
        (b"mono_", "Mono (.NET)"),
    ];
    for (needle, label) in probes {
        if has(needle) && !info.toolchain.iter().any(|t| t == label) {
            info.toolchain.push(label.to_string());
        }
    }

    info.interesting_strings = interesting_strings(bytes);
    info.libraries.sort();
    info.libraries.dedup();
    info.imported_symbols.sort();
    info.imported_symbols.dedup();
    info.imported_symbols.truncate(MAX_LIST * 3);
    info.exported_symbols.sort();
    info.exported_symbols.dedup();
    info.exported_symbols.truncate(MAX_LIST * 3);
    info
}

/// Printable ASCII runs that look meaningful: URLs, paths, SQL, format
/// strings, error messages.
fn interesting_strings(bytes: &[u8]) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let mut cur: Vec<u8> = vec![];
    let flush = |cur: &mut Vec<u8>, out: &mut Vec<String>| {
        if cur.len() >= 8 {
            let s = String::from_utf8_lossy(cur).to_string();
            let lower = s.to_ascii_lowercase();
            let interesting = lower.contains("http://")
                || lower.contains("https://")
                || lower.starts_with("select ")
                || lower.starts_with("insert into")
                || lower.starts_with("update ")
                || lower.contains("create table")
                || lower.contains("error")
                || lower.contains("failed")
                || lower.contains("usage:")
                || lower.contains("--help")
                || (s.contains('%') && s.contains(' ') && s.len() < 120)
                || lower.starts_with("/api/")
                || lower.contains(".json")
                || lower.contains("password")
                || lower.contains("token");
            if interesting && out.len() < MAX_LIST && !out.contains(&s) {
                out.push(s.chars().take(160).collect());
            }
        }
        cur.clear();
    };
    for &b in bytes {
        if (0x20..0x7f).contains(&b) {
            cur.push(b);
        } else {
            flush(&mut cur, &mut out);
        }
    }
    flush(&mut cur, &mut out);
    out
}
