//! Reading the configuration, and what happens when that goes wrong.
//!
//! These tests exist because of a real failure. The application used to treat
//! every read error the same way: fall back to defaults. Defaults carry no API
//! token, and the application writes one out as soon as it sees an empty one,
//! so a configuration that failed to read once was replaced by defaults and
//! then saved over the original on the next launch. Somebody's layout, their
//! calibration and their devices, gone silently because a file was briefly
//! locked.
//!
//! So the interesting cases here are not the happy one.

use std::fs;
use std::path::{Path, PathBuf};

use maslight_core::profile::{read_config, ConfigLoad};
use maslight_core::AppConfig;

/// A directory of our own, removed when the test finishes.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "maslight-config-test-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("could not make a temporary directory");
        Self(dir)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn written(path: &Path) -> AppConfig {
    let mut config = AppConfig::default();
    config.sanitise();
    config.ui.api_token = String::from("a-token-somebody-saved");
    config.save_to(path).expect("could not write the config");
    config
}

#[test]
fn a_configuration_that_reads_is_the_one_that_is_used() {
    let dir = TempDir::new("loaded");
    let path = dir.path("config.json");
    let original = written(&path);

    match read_config(&path) {
        ConfigLoad::Loaded(cfg) => {
            assert_eq!(cfg.ui.api_token, original.ui.api_token);
        }
        _ => panic!("a readable file should load"),
    }
}

#[test]
fn no_file_yet_is_a_first_run_and_may_be_written() {
    let dir = TempDir::new("fresh");
    let path = dir.path("config.json");

    let load = read_config(&path);
    assert!(
        matches!(load, ConfigLoad::Fresh(_)),
        "a missing file is a first run, not a failure"
    );
    assert!(
        load.writable(),
        "there is nothing to lose, so saving defaults is correct"
    );
}

#[test]
fn a_corrupt_configuration_is_kept_rather_than_overwritten() {
    let dir = TempDir::new("corrupt");
    let path = dir.path("config.json");
    let rubbish = "{ this is not json";
    fs::write(&path, rubbish).unwrap();

    let load = read_config(&path);
    let ConfigLoad::Replaced { kept, .. } = &load else {
        panic!("invalid json should be moved aside, not silently discarded");
    };

    assert_eq!(
        fs::read_to_string(kept).unwrap(),
        rubbish,
        "the original bytes must survive exactly"
    );
    assert!(!path.exists(), "the unusable file should be out of the way");
    assert!(
        load.writable(),
        "the original is safe now, so defaults may be saved"
    );
}

#[test]
fn a_second_corrupt_configuration_does_not_overwrite_the_first() {
    let dir = TempDir::new("corrupt-twice");
    let path = dir.path("config.json");

    fs::write(&path, "first broken file").unwrap();
    let ConfigLoad::Replaced { kept: first, .. } = read_config(&path) else {
        panic!("expected the first to be moved aside");
    };

    fs::write(&path, "second broken file").unwrap();
    let ConfigLoad::Replaced { kept: second, .. } = read_config(&path) else {
        panic!("expected the second to be moved aside");
    };

    assert_ne!(first, second, "each rescue needs its own name");
    assert_eq!(fs::read_to_string(&first).unwrap(), "first broken file");
    assert_eq!(fs::read_to_string(&second).unwrap(), "second broken file");
}

#[test]
fn a_configuration_that_cannot_be_read_is_never_written_over() {
    // A directory where the file should be is a read error that is not "no
    // such file", which is the case that used to be indistinguishable from a
    // first run and cost somebody their configuration.
    let dir = TempDir::new("unreadable");
    let path = dir.path("config.json");
    fs::create_dir_all(&path).unwrap();

    let load = read_config(&path);
    assert!(
        matches!(load, ConfigLoad::Unreadable { .. }),
        "a file that exists and will not read is not a first run"
    );
    assert!(
        !load.writable(),
        "this is the whole point: do not write over what could not be read"
    );
    assert!(path.exists(), "and do not move it either");
}

#[test]
fn defaults_are_still_usable_when_reading_fails() {
    // Refusing to write must not mean refusing to run: the lights should still
    // work while somebody sorts out their file.
    let dir = TempDir::new("still-runs");
    let path = dir.path("config.json");
    fs::create_dir_all(&path).unwrap();

    let load = read_config(&path);
    let config = load.config();
    assert!(
        !config.profiles.is_empty(),
        "there should still be a profile to run"
    );
    assert!(!config.active().name.is_empty());
}
