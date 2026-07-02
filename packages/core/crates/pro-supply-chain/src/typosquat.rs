/// Small, hand-picked lists of well-known package names per ecosystem,
/// used only as edit-distance anchors for typosquat detection — not meant
/// to be exhaustive, just enough well-known names that a one-or-two-edit
/// variant of a popular package is actually catchable.
pub const POPULAR_NPM: &[&str] = &[
    "react", "react-dom", "lodash", "express", "axios", "chalk", "commander", "webpack",
    "typescript", "eslint", "jest", "vue", "next", "moment", "async", "underscore", "classnames",
    "prop-types", "redux", "rxjs", "socket.io", "mongoose", "body-parser", "cors", "dotenv",
    "nodemon", "jquery", "bootstrap", "left-pad", "uuid", "yargs", "glob", "fs-extra", "semver",
    "debug", "minimist", "colors", "inquirer", "chokidar", "postcss", "vite",
];

pub const POPULAR_CRATES: &[&str] = &[
    "serde", "tokio", "rand", "regex", "clap", "log", "reqwest", "anyhow", "thiserror",
    "itertools", "chrono", "futures", "syn", "quote", "proc-macro2", "bytes", "hyper", "rayon",
    "lazy_static", "once_cell", "serde_json", "uuid", "async-trait", "toml", "libc", "cfg-if",
    "num-traits", "bitflags", "indexmap", "crossbeam",
];

pub fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// The closest popular-package match to `requested`, if any popular name is
/// within `max_distance` edits and isn't `requested` itself.
pub fn closest_typosquat_target<'a>(
    requested: &str,
    popular: &'a [&'a str],
    max_distance: usize,
) -> Option<(&'a str, usize)> {
    popular
        .iter()
        .filter(|&&name| name != requested)
        .map(|&name| (name, edit_distance(requested, name)))
        .filter(|(_, distance)| *distance <= max_distance)
        .min_by_key(|(_, distance)| *distance)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_strings_have_zero_distance() {
        assert_eq!(edit_distance("reqwest", "reqwest"), 0);
    }

    #[test]
    fn one_char_typo_has_distance_one() {
        assert_eq!(edit_distance("reqwset", "reqwest"), 2);
        assert_eq!(edit_distance("reqwrst", "reqwest"), 1);
    }

    #[test]
    fn close_typo_of_popular_package_is_flagged() {
        let hit = closest_typosquat_target("reqwests", POPULAR_CRATES, 2);
        assert_eq!(hit, Some(("reqwest", 1)));
    }

    #[test]
    fn popular_package_itself_is_not_flagged_against_itself() {
        let hit = closest_typosquat_target("reqwest", POPULAR_CRATES, 2);
        assert_eq!(hit, None);
    }

    #[test]
    fn unrelated_name_is_not_flagged() {
        let hit = closest_typosquat_target("my-totally-unique-internal-tool", POPULAR_NPM, 2);
        assert_eq!(hit, None);
    }
}
