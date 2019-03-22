//! # aur-fetch
//!
//! aur-fetch is a crate that manages downloading and diffing packages downloading from the AUR.
//! This process is split up into many different steps to give maximum control of how diffs are
//! handled and to ensure packages are never merged until the user has confirmed they have read
//! them.
//!
//! **Note:** This crate only deals with fetching packages. It assumes the list of packages you
//! pass to it are pkgbases and therefore can not work with split packages. To deal with split
//! packages the AUR RPC must be used to get the pkgbase from a pkgname.
//!
//! # Examples
//!
//! ## Printing - Diffs
//!
//! ```no_run
//! use aur_fetch::Handle;
//!
//! # use aur_fetch::Error;
//! # fn main() -> Result<(), Error> {
//!
//! let pkgs = vec!["discord", "spotify", "pacman-git"];
//!
//! // Create our handle
//! let fetch = Handle::new()?;
//!
//! // Clone/Fetch the packages.
//! let feteched = fetch.download(&pkgs)?;
//!
//! // Filter the fetched packages to ones that need to be merged.
//! let to_merge = fetch.needs_merge(&pkgs)?;
//!
//! // Print each diff
//! // Note that these are only diffs of packages that were cloned already. Meaning newly cloned
//! // packages are unveted. For sake of example we are not covering new files.
//! for (diff, pkg) in fetch.diff(&to_merge, true)?.iter().zip(pkgs.iter()) {
//!     println!("{}:", pkg);
//!     println!("{}", diff.trim());
//! }
//!
//! // Merge the changes.
//! // In a real tool you would ask for user conformation before this
//! // As long as the changes are not merged this process can always be restarted and the diffs
//! // perserved
//! fetch.merge(&to_merge)?;
//!
//! # Ok(())
//! # }
//! ```
//!
//! ## Diff View
//! ```no_run
//! use aur_fetch::Handle;
//! use std::process::Command;
//!
//! # use aur_fetch::Error;
//! # fn main() -> Result<(), Error> {
//!
//! let pkgs = vec!["discord", "spotify", "pacman-git"];
//!
//! // Create our handle
//! let fetch = Handle::new()?;
//!
//! // Clone/Fetch the packages.
//! let feteched = fetch.download(&pkgs)?;
//!
//! // Filter the fetched packages to ones that need to be merged.
//! let to_merge = fetch.needs_merge(&pkgs)?;
//!
//! // Save diffs to cache.
//! fetch.save_diffs(&to_merge)?;
//!
//! // Make a view of the new files so we can easily see them in the file browser
//! let dir = fetch.make_view(&pkgs, &to_merge)?;
//! Command::new("vifm").arg(dir.path()).spawn()?.wait()?;
//!
//! // Merge the changes.
//! fetch.merge(&to_merge)?;
//!
//! # Ok(())
//! # }
//! ```
//!
//! ## Using a Callback
//!
//! ```no_run
//! use aur_fetch::Handle;
//!
//! # use aur_fetch::Error;
//! # fn main() -> Result<(), Error> {
//!
//! let pkgs = vec!["discord", "spotify", "pacman-git"];
//!
//! // Create our handle
//! let fetch = Handle::new()?;
//!
//! // Clone/Fetch the packages.
//! let feteched = fetch.download(&pkgs)?;
//!
//! // Download the packages, printing downloads as they complete.
//! let fetched = fetch.download_cb(&pkgs, |cb| {
//!     println!("Downloaded ({:0pad$}/{:0pad$}): {}", cb.n, pkgs.len(), cb.pkg, pad = 3);
//! })?;
//!
//! // Filter the fetched packages to ones that need to be merged.
//! let to_merge = fetch.needs_merge(&pkgs)?;
//!
//! // Merge the changes.
//! // In a real tool you would ask for user conformation before this
//! // As long as the changes are not merged this process can always be restarted and the diffs
//! // perserved
//! fetch.merge(&to_merge)?;
//!
//! # Ok(())
//! # }
//! ```
#![warn(missing_docs)]
mod callback;
mod error;
mod fetch;

pub use callback::*;
pub use error::*;
pub use fetch::*;
