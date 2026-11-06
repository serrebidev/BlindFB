//! Build script, via shipfitter: embed the commit in the binary, and on
//! Windows embed the application manifest and the Explorer version resource.

fn main() {
    // wxWidgets debug symbols exceed the default MSVC PDB page limit.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg=/PDBPAGESIZE:8192");
    }
    shipfitter::build::embed_commit_info("BLINDFB");
    // The updater verifies every download with this key; its secret half signs releases.
    println!("cargo:rerun-if-changed=assets/minisign.pub");
    let public =
        std::fs::read_to_string("assets/minisign.pub").expect("assets/minisign.pub is missing");
    println!(
        "cargo:rustc-env=BLINDFB_MINISIGN_PUBLIC_KEY={}",
        public.lines().nth(1).unwrap_or_default().trim()
    );
    compile_catalogs();
    shipfitter::windows::embed_manifest("BlindFB").expect("embed manifest");
    shipfitter::windows::VersionInfo {
        product_name: "BlindFB",
        company: "Brandon Bracey",
        copyright: "Copyright (c) 2026 Brandon Bracey",
        original_filename: "blindfb.exe",
        product_version: None,
        icon: None,
    }
    .embed()
    .expect("embed version info");
}

/// Compile each `locale/<lang>/LC_MESSAGES/blindfb.po` into the `.mo` file that
/// `patois::embed_domain!` bakes into the binary, so building needs no gettext tools.
/// Untranslated and fuzzy entries are left out; patois then shows the English text.
fn compile_catalogs() {
    println!("cargo:rerun-if-changed=locale");
    let Ok(languages) = std::fs::read_dir("locale") else {
        return;
    };
    for language in languages.flatten() {
        let dir = language.path().join("LC_MESSAGES");
        let po = dir.join("blindfb.po");
        println!("cargo:rerun-if-changed={}", po.display());
        let Ok(text) = std::fs::read_to_string(&po) else {
            continue;
        };
        let entries = parse_po(&text);
        std::fs::write(dir.join("blindfb.mo"), write_mo(entries))
            .unwrap_or_else(|e| panic!("cannot write the catalog for {}: {e}", po.display()));
    }
}

/// `(msgid, msgstr)` pairs as they go into a `.mo` file: plural ids are
/// `singular\0plural`, plural translations are joined by `\0`.
fn parse_po(text: &str) -> Vec<(String, String)> {
    #[derive(Default)]
    struct Entry {
        fuzzy: bool,
        id: Option<String>,
        plural: Option<String>,
        strs: Vec<(usize, String)>,
    }
    fn unquote(line: &str) -> String {
        let inner = line.trim();
        let inner = inner.strip_prefix('"').and_then(|s| s.strip_suffix('"')).unwrap_or("");
        let mut out = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some(other) => out.push(other),
                None => {}
            }
        }
        out
    }
    fn finish(entry: Entry, out: &mut Vec<(String, String)>) {
        let Some(id) = entry.id else { return };
        if entry.strs.iter().all(|(_, s)| s.is_empty()) || (entry.fuzzy && !id.is_empty()) {
            return;
        }
        let mut strs = entry.strs;
        strs.sort_by_key(|(index, _)| *index);
        let translated: Vec<String> = strs.into_iter().map(|(_, s)| s).collect();
        let key = match entry.plural {
            Some(plural) => format!("{id}\0{plural}"),
            None => id,
        };
        out.push((key, translated.join("\0")));
    }
    let mut out = Vec::new();
    let mut entry = Entry::default();
    // Which string a continuation line ("...") extends.
    enum Target {
        None,
        Id,
        Plural,
        Str(usize),
    }
    let mut target = Target::None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            finish(std::mem::take(&mut entry), &mut out);
            target = Target::None;
        } else if let Some(flags) = line.strip_prefix("#,") {
            entry.fuzzy |= flags.split(',').any(|f| f.trim() == "fuzzy");
        } else if line.starts_with('#') {
        } else if let Some(rest) = line.strip_prefix("msgid_plural ") {
            entry.plural = Some(unquote(rest));
            target = Target::Plural;
        } else if let Some(rest) = line.strip_prefix("msgid ") {
            if entry.id.is_some() {
                finish(std::mem::take(&mut entry), &mut out);
            }
            entry.id = Some(unquote(rest));
            target = Target::Id;
        } else if let Some(rest) = line.strip_prefix("msgstr[") {
            let (index, rest) = rest.split_once(']').unwrap_or(("0", rest));
            let index = index.parse().unwrap_or(0);
            entry.strs.push((index, unquote(rest.trim())));
            target = Target::Str(entry.strs.len() - 1);
        } else if let Some(rest) = line.strip_prefix("msgstr ") {
            entry.strs.push((0, unquote(rest)));
            target = Target::Str(entry.strs.len() - 1);
        } else if line.starts_with('"') {
            let more = unquote(line);
            match target {
                Target::Id => entry.id.get_or_insert_with(String::new).push_str(&more),
                Target::Plural => entry.plural.get_or_insert_with(String::new).push_str(&more),
                Target::Str(i) => entry.strs[i].1.push_str(&more),
                Target::None => {}
            }
        }
    }
    finish(entry, &mut out);
    out
}

/// A GNU `.mo` file (little endian, no hash table), entries sorted by msgid.
fn write_mo(mut entries: Vec<(String, String)>) -> Vec<u8> {
    entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    entries.dedup_by(|a, b| a.0 == b.0);
    let count = entries.len() as u32;
    let ids_table = 28u32;
    let strs_table = ids_table + 8 * count;
    let mut data_offset = strs_table + 8 * count;
    let mut head = Vec::new();
    let mut ids = Vec::new();
    let mut strs = Vec::new();
    let mut data = Vec::new();
    for (id, _) in &entries {
        ids.extend((id.len() as u32).to_le_bytes());
        ids.extend(data_offset.to_le_bytes());
        data.extend(id.as_bytes());
        data.push(0);
        data_offset += id.len() as u32 + 1;
    }
    for (_, translation) in &entries {
        strs.extend((translation.len() as u32).to_le_bytes());
        strs.extend(data_offset.to_le_bytes());
        data.extend(translation.as_bytes());
        data.push(0);
        data_offset += translation.len() as u32 + 1;
    }
    for word in [0x9504_12de_u32, 0, count, ids_table, strs_table, 0, data_offset] {
        head.extend(word.to_le_bytes());
    }
    [head, ids, strs, data].concat()
}
