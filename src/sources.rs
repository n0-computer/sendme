//! File selection happens before any selected content is imported or shared.

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use anyhow::Context;
use ignore::{
    gitignore::{Gitignore, GitignoreBuilder},
    Match,
};
use walkdir::WalkDir;

pub(crate) fn collect_sources(
    input: &Path,
    respect_ignore: bool,
) -> anyhow::Result<Vec<(String, PathBuf)>> {
    let input = input.canonicalize()?;
    let name_root = input.parent().context("cannot get input parent")?;
    let filtering = respect_ignore && input.is_dir();
    // One pair per ancestor: .ignore takes precedence over every .gitignore.
    let mut rules: Vec<(Gitignore, Gitignore)> = Vec::new();
    let mut files = Vec::new();
    let mut walker = WalkDir::new(&input).into_iter();
    while let Some(entry) = walker.next() {
        let entry = entry?;
        let path = entry.path();
        let is_dir = entry.file_type().is_dir();
        if filtering {
            rules.truncate(entry.depth());
            if entry.depth() > 0 {
                let matched = rules
                    .iter()
                    .rev()
                    .map(|(_, generic)| generic)
                    .chain(rules.iter().rev().map(|(git, _)| git))
                    .map(|matcher| matcher.matched(path, is_dir))
                    .find(|matched| !matches!(matched, Match::None));
                if entry.file_name() == ".git" || matched.is_some_and(|m| m.is_ignore()) {
                    if is_dir {
                        walker.skip_current_dir();
                    }
                    continue;
                }
            }
            if is_dir {
                rules.push((
                    load_git_rules(path, ".gitignore")?,
                    load_git_rules(path, ".ignore")?,
                ));
            }
        }
        // Preserve the original policy: do not follow descendant symlinks.
        if !entry.file_type().is_file() {
            continue;
        }
        let name = crate::canonicalized_path_to_string(path.strip_prefix(name_root)?, true)?;
        files.push((name, path.to_owned()));
    }
    anyhow::ensure!(
        !filtering || !files.is_empty(),
        "no files to send after applying ignore rules"
    );
    Ok(files)
}

fn load_git_rules(root: &Path, filename: &str) -> anyhow::Result<Gitignore> {
    let path = root.join(filename);
    let mut builder = GitignoreBuilder::new(root);
    if let Some(contents) = read_rules(&path)? {
        let contents = contents.strip_prefix('\u{feff}').unwrap_or(&contents);
        for (line, text) in contents.lines().enumerate() {
            builder
                .add_line(Some(path.clone()), text)
                .with_context(|| format!("{}:{}", path.display(), line + 1))?;
        }
    }
    builder
        .build()
        .with_context(|| format!("invalid rules in {}", path.display()))
}

fn read_rules(path: &Path) -> anyhow::Result<Option<String>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("reading {}", path.display())),
    };
    anyhow::ensure!(
        metadata.is_file(),
        "ignore file must be a regular file: {}",
        path.display()
    );
    fs::read_to_string(path)
        .map(Some)
        .with_context(|| format!("reading {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, name: &str, contents: &str) {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn names(root: &Path, enabled: bool) -> Vec<String> {
        let mut names: Vec<_> = collect_sources(root, enabled)
            .unwrap()
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        names.sort();
        names
    }

    #[test]
    fn selection_combines_rules_and_preserves_unfiltered_and_explicit_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        write(temp.path(), ".gitignore", "*\n"); // Outside the selected tree.
        let fixtures = [
            (".gitignore", "\u{feff}*.log\nblocked/\nrescued.txt\n"),
            (".ignore", "!rescued.txt\n"),
            (".dockerignore", "*\n[z-a]\n"), // Not a rule source, even if invalid.
            ("nested/.gitignore", "!keep.log\nrescued.txt\n"),
            ("nested/.dockerignore", "*\n"), // Not a rule source.
            ("nested/.ignore", "!rescued.txt\n"),
            ("blocked/.ignore", "!keep.txt\n"), // Cannot rescue a pruned parent.
            ("blocked/keep.txt", "blocked"),
            (".git/config", "metadata"),
            (".hidden", "hidden"),
            ("private.log", "private"),
            ("rescued.txt", "generic overrides git"),
            ("nested/keep.log", "keep"),
            ("nested/drop.log", "drop"),
            ("nested/readme.md", "keep"),
            ("nested/rescued.txt", "generic overrides git"),
        ];
        for (name, contents) in fixtures {
            write(&root, name, contents);
        }
        assert_eq!(names(&root, false).len(), fixtures.len());
        assert_eq!(
            names(&root, true),
            [
                "project/.dockerignore",
                "project/.gitignore",
                "project/.hidden",
                "project/.ignore",
                "project/nested/.dockerignore",
                "project/nested/.gitignore",
                "project/nested/.ignore",
                "project/nested/keep.log",
                "project/nested/readme.md",
                "project/nested/rescued.txt",
                "project/rescued.txt",
            ]
        );
        assert_eq!(names(&root.join("private.log"), true), ["private.log"]);
    }

    #[test]
    fn missing_rules_are_allowed_but_invalid_rules_and_empty_selection_fail() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        write(&root, "keep", "data");
        assert_eq!(names(&root, true), ["data/keep"]);
        for filename in [".gitignore", ".ignore"] {
            write(&root, filename, "[z-a]\n");
            let error = collect_sources(&root, true).unwrap_err();
            assert!(format!("{error:#}").contains(filename));
            assert_eq!(names(&root, false).len(), 2);
            fs::remove_file(root.join(filename)).unwrap();
        }
        write(&root, ".ignore", "*\n");
        assert!(collect_sources(&root, true)
            .unwrap_err()
            .to_string()
            .contains("no files to send"));
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_followed_and_symlink_rules_are_rejected() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        write(&root, "keep", "data");
        write(temp.path(), "outside", "*\n");
        symlink(temp.path().join("outside"), root.join("link")).unwrap();
        assert_eq!(names(&root, true), ["data/keep"]);
        symlink(temp.path().join("outside"), root.join(".ignore")).unwrap();
        assert!(collect_sources(&root, true)
            .unwrap_err()
            .to_string()
            .contains("regular file"));
    }
}
