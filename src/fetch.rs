use crate::{Callback, CommandFailed, Error};

use std::env::{self, current_dir};
use std::ffi::OsStr;
use std::fs::{create_dir_all, File};
use std::io::{self, Write};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam::channel;
use url::Url;

static SEEN: &str = "AUR_SEEN";

/// Result type for this crate;
pub type Result<T> = std::result::Result<T, Error>;

/// Represents a git repository.
pub struct Repo {
    /// The url to the git repo.
    pub url: Url,
    /// The name of the git repo.
    pub name: String,
}

/// Handle to the current configuration.
///
/// This handle is used to configure parts of the fetching process. All the features of this crate
/// must be done through this handle.
#[derive(Clone, Debug)]
pub struct Fetch {
    /// The directory to place AUR packages in.
    pub clone_dir: PathBuf,
    /// The directory to place diffs in.
    pub diff_dir: PathBuf,
    /// The git command to run.
    pub git: PathBuf,
    /// Flags passed to git.
    pub git_flags: Vec<String>,
    /// The AUR URL.
    pub aur_url: Url,
}

fn command_err(cmd: &Command, stderr: Option<String>) -> Error {
    Error::CommandFailed(CommandFailed {
        dir: cmd.get_current_dir().unwrap().to_owned(),
        command: cmd.get_program().to_owned().into(),
        args: cmd
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect(),
        stderr,
    })
}

impl Fetch {
    /// Create a new Handle with working defaults.
    ///
    /// This Inializes the clone and diff dir to the current dirrectory. If you want to configure
    /// a cache directory you will need to do that yourself.
    pub fn new() -> Result<Self> {
        Ok(Self {
            clone_dir: env::current_dir()?,
            diff_dir: env::current_dir()?,
            git: "git".into(),
            git_flags: Vec::new(),
            aur_url: "https://aur.archlinux.org".parse().unwrap(),
        })
    }

    /// Create a new handle with a specified cache dir.
    ///
    /// clone_dir will be a subdirectory named clone inside of the specified path.
    /// diff_dir will be a subdirectory named diff inside of the specified path.
    pub fn with_cache_dir<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref();

        Self {
            clone_dir: path.join("clone"),
            diff_dir: path.join("diff"),
            git: "git".into(),
            git_flags: Vec::new(),
            aur_url: "https://aur.archlinux.org".parse().unwrap(),
        }
    }

    /// Create a new handle with a specified cache dir.
    ///
    ///Both diffs and cloned packages will be places in the provided dir.
    pub fn with_combined_cache_dir<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref();

        Self {
            clone_dir: path.into(),
            diff_dir: path.into(),
            git: "git".into(),
            git_flags: Vec::new(),
            aur_url: "https://aur.archlinux.org".parse().unwrap(),
        }
    }

    /// Downloads a list of packages to the cache dir.
    ///
    /// This downloads packages from the AUR using git. New packages will be cloned, while packages
    /// that already exist in cache will be fetched. Merging will need to be done in a separate
    /// step.
    ///
    /// Each package is downloaded concurrently which givess a major speedup
    ///
    /// Depending on how many packages are being downloaded and connection speed this
    /// function may take a little while to complete. See [`download_cb`](fn.download_cb.html) if
    /// you wish track the progress of each download.
    ///
    /// This also filters the input list to packages that were already in cache. This filtered list
    /// can then be passed on to [`merge`](fn.merge.html) as freshly cloned packages will
    /// not need to be merged.
    pub fn download<S: AsRef<str> + Send + Sync>(&self, pkgs: &[S]) -> Result<Vec<String>> {
        self.download_cb(pkgs, |_| ())
    }

    /// The same as [`download`](fn.download.html) but calls a Callback after each download.
    ///
    /// The callback is called each time a package download is completed.
    pub fn download_cb<S: AsRef<str> + Send + Sync, F: Fn(Callback)>(
        &self,
        pkgs: &[S],
        f: F,
    ) -> Result<Vec<String>> {
        let repos = pkgs
            .iter()
            .map(|p| {
                let mut url = self.aur_url.clone();
                url.set_path(p.as_ref());
                Repo {
                    url,
                    name: p.as_ref().to_string(),
                }
            })
            .collect::<Vec<_>>();
        self.download_repos_cb(&repos, f)
    }

    /// The same as [`download`](fn.download.html) but downloads a specified list of repos instead of AUR packages.
    pub fn download_repos<F: Fn(Callback)>(&self, repos: &[Repo]) -> Result<Vec<String>> {
        self.download_repos_cb(repos, |_| ())
    }

    /// The same as [`download_repos`](fn.download_repos.html) but calls a Callback after each download.
    ///
    /// The callback is called each time a package download is completed.
    pub fn download_repos_cb<F: Fn(Callback)>(&self, repos: &[Repo], f: F) -> Result<Vec<String>> {
        let (pkg_send, pkg_rec) = channel::bounded(0);
        let (fetched_send, fetched_rec) = channel::bounded(32);
        let f = &f;
        let stop = &AtomicBool::new(false);
        let mut fetched = Vec::with_capacity(repos.len());

        std::thread::scope(|scope| {
            scope.spawn(move || {
                for repo in repos {
                    if pkg_send.send(repo).is_err() {
                        break;
                    }
                }
            });

            for _ in 0..20.min(repos.len()) {
                let fetched_send = fetched_send.clone();
                let pkg_rec = pkg_rec.clone();
                scope.spawn(move || {
                    for repo in &pkg_rec {
                        if stop.load(Ordering::Acquire) {
                            break;
                        }
                        match self.download_pkg(&repo.url, &repo.name) {
                            Ok((fetched, out)) => {
                                let _ = fetched_send.send(Ok((repo.name.clone(), fetched, out)));
                            }
                            Err(e) => {
                                stop.store(true, Ordering::Release);
                                let _ = fetched_send.send(Err(e));
                                break;
                            }
                        }
                    }
                });
            }

            drop(pkg_rec);
            drop(fetched_send);

            for (n, msg) in fetched_rec.into_iter().enumerate() {
                let (pkg, was_fetched, out) = msg?;
                f(Callback {
                    pkg: &pkg,
                    n: n + 1,
                    output: String::from_utf8_lossy(&out).trim(),
                });
                if was_fetched {
                    fetched.push(pkg)
                }
            }

            Ok(fetched)
        })
    }

    fn download_pkg<S: AsRef<str>>(&self, url: &Url, dir: S) -> Result<(bool, Vec<u8>)> {
        self.mk_clone_dir()?;

        let dir = dir.as_ref();
        let is_git_repo = self.is_git_repo(dir);

        let mut command = Command::new(&self.git);

        let fetched = if is_git_repo {
            command.current_dir(self.clone_dir.join(dir));
            command.args(["fetch", "-v"]);
            true
        } else {
            command.current_dir(&self.clone_dir);
            command.args(["clone", "--no-progress", "--", url.as_str(), dir]);
            false
        };
        log_cmd(&command);
        let output = command
            .output()
            .map_err(|e| command_err(&command, Some(e.to_string())))?;

        if !output.status.success() {
            return Err(command_err(
                &command,
                Some(String::from_utf8_lossy(&output.stderr).into_owned()),
            ));
        }

        Ok((fetched, output.stderr))
    }

    /// Filters a list of packages, keep ones that have a diff.
    ///
    /// A reoo has a diff if AUR_SEEN is defined and is different to the upstram HEAD.
    pub fn has_diff<'a, S: AsRef<str>>(&self, pkgs: &'a [S]) -> Result<Vec<&'a str>> {
        let mut ret = Vec::new();

        for pkg in pkgs {
            if git_has_diff(
                &self.git,
                &self.git_flags,
                self.clone_dir.join(pkg.as_ref()),
            )? {
                ret.push(pkg.as_ref());
            }
        }

        Ok(ret)
    }

    /// Filterrs a list of packages, keeping ones that have not yet been seen.
    ///
    /// A repo is seen if AUR_SEEN exists and is equal to the upstram HEAD.
    pub fn unseen<'a, S: AsRef<str>>(&self, pkgs: &'a [S]) -> Result<Vec<&'a str>> {
        let mut ret = Vec::new();

        for pkg in pkgs {
            if git_unseen(
                &self.git,
                &self.git_flags,
                self.clone_dir.join(pkg.as_ref()),
            )? {
                ret.push(pkg.as_ref());
            }
        }

        Ok(ret)
    }

    /// Diff a list of packages returning the diffs as strings.
    ///
    /// Diffing a package that is already up to date will generate a diff against an empty git tree
    ///
    /// Additionally this function gives you the ability to force color. This is useful if you
    /// intend to print the diffs to stdout.
    pub fn diff<S: AsRef<str>>(&self, pkgs: &[S], color: bool) -> Result<Vec<String>> {
        let pkgs = pkgs.iter();
        let mut ret = Vec::new();

        for pkg in pkgs {
            let output = git_log(
                &self.git,
                &self.git_flags,
                self.clone_dir.join(pkg.as_ref()),
                color,
            )?;
            let mut s: String = String::from_utf8_lossy(&output.stdout).into();
            let output = git_diff(
                &self.git,
                &self.git_flags,
                self.clone_dir.join(pkg.as_ref()),
                color,
            )?;
            s.push_str(&String::from_utf8_lossy(&output.stdout));
            s.push('\n');
            ret.push(s);
        }

        Ok(ret)
    }

    /// Diff a single package.
    ///
    /// Relies on `git diff` for printing. This means output will likley be coloured and ran through less.
    /// Although this is dependent on the user's git config
    pub fn print_diff<S: AsRef<str>>(&self, pkg: S) -> Result<()> {
        show_git_diff(
            &self.git,
            &self.git_flags,
            self.clone_dir.join(pkg.as_ref()),
        )
    }

    /// Diff a list of packages and save them to diff_dir.
    ///
    /// Diffing a package that is already up to date will generate a diff against an empty git tree
    pub fn save_diffs<S: AsRef<str>>(&self, pkgs: &[S]) -> Result<()> {
        self.mk_diff_dir()?;

        for pkg in pkgs {
            let mut path = self.diff_dir.join(pkg.as_ref());
            path.set_extension("diff");

            let mut file = File::create(path)?;

            file.write_all(
                &git_log(
                    &self.git,
                    &self.git_flags,
                    self.clone_dir.join(pkg.as_ref()),
                    false,
                )?
                .stdout,
            )?;
            file.write_all(b"\n")?;
            file.write_all(
                &git_diff(
                    &self.git,
                    &self.git_flags,
                    self.clone_dir.join(pkg.as_ref()),
                    false,
                )?
                .stdout,
            )?;
        }

        Ok(())
    }

    /// Makes a view of newly downloaded files.
    ///
    /// This view is a dir containing the packages downloaded/fetched and diffs
    /// for packages that have diffs.
    ///
    /// Files are symlinked from the cache dirs so there is no duplication of files.
    pub fn make_view<P: AsRef<Path>, S1: AsRef<str>, S2: AsRef<str>>(
        &self,
        dir: P,
        pkgs: &[S1],
        diffs: &[S2],
    ) -> Result<()> {
        let dir = dir.as_ref();

        for pkg in diffs {
            let pkg = format!("{}.diff", pkg.as_ref());
            let dest = dir.join(&pkg);
            let src = self.diff_dir.join(&pkg);
            if src.is_file() {
                symlink(src, dest)?;
            }
        }

        for pkg in pkgs {
            let dest = dir.join(pkg.as_ref());
            let pkgbuild_dest = dir.join(format!("{}.PKGBUILD", pkg.as_ref()));
            let srcinfo_dest = dir.join(format!("{}.SRCINFO", pkg.as_ref()));

            let src = self.clone_dir.join(pkg.as_ref());
            if src.is_dir() {
                symlink(src, dest)?;
            }

            let src = self.clone_dir.join(pkg.as_ref()).join("PKGBUILD");
            if src.is_file() {
                symlink(src, pkgbuild_dest)?;
            }

            let src = self.clone_dir.join(pkg.as_ref()).join(".SRCINFO");
            if src.is_file() {
                symlink(src, srcinfo_dest)?;
            }
        }

        Ok(())
    }

    /// Merge a list of packages with their upstream.
    pub fn merge<S: AsRef<str>>(&self, pkgs: &[S]) -> Result<()> {
        self.merge_cb(pkgs, |_| ())
    }

    /// Merge a list of packages with their upstream, calling callback for each merge.
    pub fn merge_cb<S: AsRef<str>, F: Fn(Callback)>(&self, pkgs: &[S], cb: F) -> Result<()> {
        let pkgs = pkgs.iter();

        for (n, pkg) in pkgs.enumerate() {
            let path = self.clone_dir.join(pkg.as_ref());
            let output = git_rebase(&self.git, &self.git_flags, path)?;
            cb(Callback {
                pkg: pkg.as_ref(),
                n,
                output: String::from_utf8_lossy(&output.stdout).trim(),
            });
        }

        Ok(())
    }

    /// Marks a list of repos as seen.
    ///
    /// This updates AUR_SEEN to the upstream HEAD
    pub fn mark_seen<S: AsRef<str>>(&self, pkgs: &[S]) -> Result<()> {
        for pkg in pkgs {
            let path = self.clone_dir.join(pkg.as_ref());
            git_mark_seen(&self.git, &self.git_flags, path)?;
        }

        Ok(())
    }

    /// Commits changes to list of packages
    ///
    /// This is intended to allow saving changes made by the user after reviewing.
    pub fn commit<S1: AsRef<str>, S2: AsRef<str>>(&self, pkgs: &[S1], message: S2) -> Result<()> {
        for pkg in pkgs {
            let path = self.clone_dir.join(pkg.as_ref());
            git_commit(&self.git, &self.git_flags, path, message.as_ref())?;
        }

        Ok(())
    }

    /// Check if a package is already cloned.
    pub fn is_git_repo<S: AsRef<str>>(&self, pkg: S) -> bool {
        self.clone_dir.join(pkg.as_ref()).join(".git").is_dir()
    }

    fn mk_clone_dir(&self) -> io::Result<()> {
        create_dir_all(&self.clone_dir)
    }

    fn mk_diff_dir(&self) -> io::Result<()> {
        create_dir_all(&self.diff_dir)
    }
}

fn color_str(color: bool) -> &'static str {
    if color {
        "--color=always"
    } else {
        "--color=never"
    }
}

fn git_command<S: AsRef<OsStr>, P: AsRef<Path>>(
    git: S,
    path: P,
    flags: &[String],
    args: &[&str],
) -> Result<Output> {
    let mut command = Command::new(git.as_ref());
    command
        .current_dir(path.as_ref())
        .args(flags)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0");

    log_cmd(&command);
    let output = command
        .output()
        .map_err(|e| command_err(&command, Some(e.to_string())))?;

    if output.status.success() {
        Ok(output)
    } else {
        Err(command_err(
            &command,
            Some(String::from_utf8_lossy(&output.stderr).into()),
        ))
    }
}

fn show_git_command<S: AsRef<OsStr>, P: AsRef<Path>>(
    git: S,
    path: P,
    flags: &[String],
    args: &[&str],
) -> Result<()> {
    let mut command = Command::new(git.as_ref());
    command
        .current_dir(path.as_ref())
        .args(flags)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0");

    log_cmd(&command);
    let status = command
        .spawn()
        .map_err(|e| command_err(&command, Some(e.to_string())))?
        .wait()
        .map_err(|e| command_err(&command, Some(e.to_string())))?;

    if status.success() {
        Ok(())
    } else {
        Err(command_err(&command, None))
    }
}

fn git_mark_seen<S: AsRef<OsStr>, P: AsRef<Path>>(
    git: S,
    flags: &[String],
    path: P,
) -> Result<Output> {
    git_command(&git, &path, flags, &["update-ref", SEEN, "HEAD"])
}

fn git_rebase<S: AsRef<OsStr>, P: AsRef<Path>>(
    git: S,
    flags: &[String],
    path: P,
) -> Result<Output> {
    git_command(&git, &path, flags, &["reset", "--hard", "-q", "HEAD"])?;
    if git_command(&git, &path, flags, &["symbolic-ref", "-q", "HEAD"]).is_err() {
        git_command(&git, &path, flags, &["checkout", "master"])?;
    }
    git_command(&git, &path, flags, &["rebase", "--stat"])
}

fn git_unseen<S: AsRef<OsStr>, P: AsRef<Path>>(git: S, flags: &[String], path: P) -> Result<bool> {
    if git_has_seen(&git, flags, &path)? {
        let is_unseen = git_command(
            git,
            path,
            flags,
            &["merge-base", "--is-ancestor", "HEAD@{u}", "AUR_SEEN"],
        )
        .is_err();
        Ok(is_unseen)
    } else {
        Ok(true)
    }
}

fn git_has_diff<S: AsRef<OsStr>, P: AsRef<Path>>(
    git: S,
    flags: &[String],
    path: P,
) -> Result<bool> {
    if git_has_seen(&git, flags, &path)? {
        let output = git_command(git, path, flags, &["rev-parse", SEEN, "HEAD@{u}"])?;

        let s = String::from_utf8_lossy(&output.stdout);
        let mut s = s.split('\n');

        let head = s.next().unwrap();
        let upstream = s.next().unwrap();

        Ok(head != upstream)
    } else {
        Ok(false)
    }
}

fn git_log<S: AsRef<OsStr>, P: AsRef<Path>>(
    git: S,
    flags: &[String],
    path: P,
    color: bool,
) -> Result<Output> {
    let color = color_str(color);
    git_command(git, path, flags, &["log", "..HEAD@{u}", color])
}

fn git_has_seen<S: AsRef<OsStr>, P: AsRef<Path>>(
    git: S,
    flags: &[String],
    path: P,
) -> Result<bool> {
    let output = git_command(&git, &path, flags, &["rev-parse", "--verify", SEEN]).is_ok();
    Ok(output)
}

fn git_head<S: AsRef<OsStr>, P: AsRef<Path>>(git: S, flags: &[String], path: P) -> Result<String> {
    let output = git_command(git, path, flags, &["rev-parse", "HEAD"])?;
    let output = String::from_utf8_lossy(&output.stdout);
    Ok(output.trim().to_string())
}

fn git_diff<S: AsRef<OsStr>, P: AsRef<Path>>(
    git: S,
    flags: &[String],
    path: P,
    color: bool,
) -> Result<Output> {
    let color = color_str(color);
    let head = git_head(&git, flags, &path)?;
    let output = if git_has_seen(&git, flags, &path)? {
        git_command(&git, &path, flags, &["reset", "--hard", SEEN])?;
        git_command(
            &git,
            &path,
            flags,
            &[
                "-c",
                "user.email=aur",
                "-c",
                "user.name=aur",
                "merge",
                "--no-edit",
                "--no-ff",
                "--no-commit",
            ],
        )?;
        Ok(git_command(
            &git,
            &path,
            flags,
            &[
                "diff",
                "--stat",
                "--patch",
                "--cached",
                color,
                "--",
                ":!.SRCINFO",
            ],
        )?)
    } else {
        Ok(git_command(
            &git,
            &path,
            flags,
            &[
                "diff",
                "--stat",
                "--patch",
                color,
                "4b825dc642cb6eb9a060e54bf8d69288fbee4904..HEAD@{u}",
                "--",
                ":!.SRCINFO",
            ],
        )?)
    };

    git_command(&git, &path, flags, &["reset", "--hard", &head])?;
    output
}

fn show_git_diff<S: AsRef<OsStr>, P: AsRef<Path>>(git: S, flags: &[String], path: P) -> Result<()> {
    let head = git_head(&git, flags, &path)?;
    if git_has_seen(&git, flags, &path)? {
        git_command(&git, &path, flags, &["reset", "--hard", SEEN])?;
        git_command(
            &git,
            &path,
            flags,
            &[
                "-c",
                "user.email=aur",
                "-c",
                "user.name=aur",
                "merge",
                "--no-edit",
                "--no-ff",
                "--no-commit",
            ],
        )?;
        show_git_command(
            &git,
            &path,
            flags,
            &["diff", "--stat", "--patch", "--cached", "--", ":!.SRCINFO"],
        )?;
    } else {
        show_git_command(
            &git,
            &path,
            flags,
            &[
                "diff",
                "--stat",
                "--patch",
                "4b825dc642cb6eb9a060e54bf8d69288fbee4904..HEAD@{u}",
                "--",
                ":!.SRCINFO",
            ],
        )?;
    }

    git_command(&git, &path, flags, &["reset", "--hard", &head])?;
    Ok(())
}

fn git_commit<S: AsRef<OsStr>, P: AsRef<Path>>(
    git: S,
    flags: &[String],
    path: P,
    message: &str,
) -> Result<()> {
    let path = path.as_ref();
    let git = git.as_ref();

    let has_user = git_command(git, path, flags, &["config", "user.name"]).is_ok()
        && git_command(git, path, flags, &["config", "user.email"]).is_ok();

    if git_command(git, path, flags, &["diff", "--exit-code"]).is_err() {
        if has_user {
            git_command(git, path, flags, &["commit", "-am", message])?;
        } else {
            git_command(
                git,
                path,
                flags,
                &[
                    "-c",
                    "user.email=aur",
                    "-c",
                    "user.name=aur",
                    "commit",
                    "-am",
                    "AUR",
                ],
            )?;
        }
    }

    Ok(())
}

fn log_cmd(cmd: &Command) {
    if log::log_enabled!(log::Level::Debug) {
        let bin = cmd.get_program().to_string_lossy().to_string();
        let args = cmd
            .get_args()
            .map(|s| s.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let dir = cmd
            .get_current_dir()
            .map(|p| p.to_owned())
            .unwrap_or_else(|| current_dir().unwrap_or_else(|_| "?".into()));
        let dir = dir.display();
        log::debug!("running: CWD={dir} {bin} {args}")
    }
}
