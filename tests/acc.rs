use clap::Parser;
use diary::{Cmd, Env};
use dircpy::copy_dir;
use glob::glob;
use pretty_assertions as pa;
use std::path::Path;
use std::{env, fs};
use test_case::test_case;

#[test_case("add-complementary-video-1")]
#[test_case("add-complementary-video-2")]
#[test_case("add-dry-run")]
#[test_case("add-filter-from")]
#[test_case("add-filter-from-to")]
#[test_case("add-filter-on")]
#[test_case("add-remove")]
#[test_case("add-remove-and-dry-run")]
#[test_case("add-screenshot")]
#[test_case("add-smoke")]
#[test_case("add-verbose")]
#[test_case("add-video")]
#[test_case("add-where-date-is-in-file-name")]
fn test(case: &str) {
    colored::control::set_override(false);
    env::set_var("TZ", "UTC");

    // ---

    let dir = Path::new("tests").join("acc").join(case);

    let given = dir.join("given");
    let given_cmd = given.join("cmd");
    let given_diary = given.join("diary");
    let given_source = given.join("source");

    let expected = dir.join("expected");
    let expected_diary = expected.join("diary");
    let expected_source = expected.join("source");

    // ---

    let tmp = dir.join(".tmp");
    let tmp_diary = tmp.join("diary");
    let tmp_source = tmp.join("source");

    if tmp.exists() {
        fs::remove_dir_all(&tmp).unwrap();
    }

    fs::create_dir(&tmp).unwrap();

    copy_dir(given_diary, &tmp_diary).unwrap();
    copy_dir(given_source, &tmp_source).unwrap();

    // ---

    let mut stdout = Vec::new();

    let mut env = Env {
        stdout: &mut stdout,
    };

    let cmd = {
        let cmd = fs::read_to_string(given_cmd)
            .unwrap()
            .trim()
            .replace("$diary", tmp_diary.to_str().unwrap())
            .replace("$source", tmp_source.to_str().unwrap());

        Cmd::parse_from(cmd.split(' '))
    };

    cmd.run(&mut env).unwrap();

    // ---

    let stdout = String::from_utf8_lossy(&stdout);
    let stdout = stdout.replace(&format!("tests/acc/{}/.tmp/", case), "");

    assert_file_eq(expected.join("stdout"), stdout);
    assert_fs_eq(expected_diary, tmp_diary);
    assert_fs_eq(expected_source, tmp_source);

    // ---

    fs::remove_dir_all(&tmp).unwrap();
}

fn assert_file_eq(path: impl AsRef<Path>, actual: impl AsRef<str>) {
    let path = path.as_ref();
    let actual = actual.as_ref();

    let expected = fs::read_to_string(path).unwrap_or_default();
    let path_new = format!("{}.new", path.display());

    if expected == actual {
        _ = fs::remove_file(path_new);
    } else {
        fs::write(path_new, actual).unwrap();

        pa::assert_eq!(expected, actual);
    }
}

fn assert_fs_eq(expected: impl AsRef<Path>, actual: impl AsRef<Path>) {
    let expected = expected.as_ref();
    let actual = actual.as_ref();

    // ---

    let tree = |dir: &Path| -> String {
        let mut paths: Vec<_> = glob(&format!("{}/**/*", dir.display()))
            .unwrap()
            .map(|entry| {
                entry
                    .unwrap()
                    .strip_prefix(dir)
                    .unwrap()
                    .display()
                    .to_string()
            })
            .filter(|entry| entry != ".gitkeep")
            .collect();

        paths.sort();
        paths.join("\n")
    };

    pa::assert_eq!(tree(expected), tree(actual));

    // ---

    for file in tree(expected).split('\n') {
        let expected = expected.join(file);
        let actual = actual.join(file);

        assert_eq!(expected.is_file(), actual.is_file());

        if expected.is_file() {
            let expected_data = fs::read(&expected).unwrap();
            let actual_data = fs::read(&actual).unwrap();

            if expected_data != actual_data {
                panic!(
                    "assertion failed: fixtures are different:\n{} vs {}",
                    expected.display(),
                    actual.display()
                );
            }
        }
    }
}
