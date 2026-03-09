use std::collections::BTreeMap;

use repo_walker::{detect_language_for_file, walk_repository, WalkerOptions};

mod common;
use common::FixtureRepo;

#[test]
fn mixed_language_fixture_detection_is_deterministic() {
    let fixture = FixtureRepo::new().expect("create fixture repo");
    fixture
        .write("src/main.rs", "fn main() {}\n")
        .expect("write rust");
    fixture
        .write("web/app.ts", "export const n = 1;\n")
        .expect("write typescript");
    fixture
        .write("scripts/bootstrap", "#!/usr/bin/env python\nprint('ok')\n")
        .expect("write python shebang");
    fixture
        .write("php/snippet", "<?php echo 'ok';\n")
        .expect("write php snippet");
    fixture
        .write("data/blob", "{\"ok\":true}\n")
        .expect("write json blob");
    fixture
        .write("docs/readme.unknown", "plain text\n")
        .expect("write unknown");

    let result = walk_repository(fixture.path(), &WalkerOptions::default()).expect("walk repo");
    let files = result.files;
    let mut detected = BTreeMap::new();

    for file in files {
        let language = detect_language_for_file(&file.absolute_path).expect("detect language");
        detected.insert(
            file.relative_path.to_string_lossy().replace('\\', "/"),
            language.as_str().to_string(),
        );
    }

    let expected = BTreeMap::from([
        ("data/blob".to_string(), "json".to_string()),
        ("docs/readme.unknown".to_string(), "unknown".to_string()),
        ("php/snippet".to_string(), "php".to_string()),
        ("scripts/bootstrap".to_string(), "python".to_string()),
        ("src/main.rs".to_string(), "rust".to_string()),
        ("web/app.ts".to_string(), "typescript".to_string()),
    ]);

    assert_eq!(detected, expected);
}
