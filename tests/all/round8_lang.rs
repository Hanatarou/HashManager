//! The language is global: these tests change it, so they live in a binary of their own.
use hashcore::*;
use std::path::Path;

#[test]
fn reasons_from_the_engine_follow_the_language_of_the_interface() {
    let d = std::env::temp_dir().join(format!("hm_r8l_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let junk = d.join("x.sig");
    std::fs::write(&junk, b"nothing like a signature").unwrap();
    let file = d.join("x");
    std::fs::write(&file, b"x").unwrap();
    let reason = |lang: &str| {
        assert!(i18n::set_language(lang));
        match sig::verify_file(&file, &junk, "-----BEGIN PGP PUBLIC KEY BLOCK-----\nnot a key\n") {
            sig::Outcome::Unreadable { reason } | sig::Outcome::Invalid { reason } => reason,
            o => panic!("{o:?}"),
        }
    };
    let en = reason("en-US");
    let pt = reason("pt-BR");
    let ja = reason("ja");
    assert!(en.contains("OpenPGP") && pt.contains("OpenPGP"));
    assert!(en != pt && pt != ja && en != ja, "{en} | {pt} | {ja}");
    assert!(pt.contains("não") || pt.contains("assinatura"), "{pt}");
    // A disk image error too.
    let bad = d.join("bad.vhd");
    let mut v = vec![0u8; 1024];
    v[..8].copy_from_slice(b"conectix");
    std::fs::write(&bad, &v).unwrap();
    i18n::set_language("pt-BR");
    let e = diskimage::open(&bad).err().expect("damaged").to_string();
    assert!(e.contains("VHD") && (e.contains("soma") || e.contains("danificado")), "{e}");
    i18n::set_language("en-US");
    let e = diskimage::open(&bad).err().expect("damaged").to_string();
    assert!(e.contains("checksum"), "{e}");
    let _ = Path::new("");
    std::fs::remove_dir_all(&d).unwrap();
}
