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
//! use aur_fetch::Fetch;
//!
//! # use aur_fetch::Error;
//! # async fn foo() -> Result<(), Error> {
//!
//! let pkgs = vec!["discord", "spotify", "pacman-git"];
//!
//! // Create our handle
//! let fetch = Fetch::new()?;
//!
//! // Clone/Fetch the packages.
//! let fetched = fetch.download(&pkgs)?;
//!
//! // Merge changes
//! fetch.merge(&fetched)?;
//!
//! // Only diff packages that have not been reviewed
//! let to_diff = fetch.unseen(&pkgs)?;
//!
//! // Print each diff
//! for (diff, pkg) in fetch.diff(&to_diff, true)?.iter().zip(pkgs.iter()) {
//!     println!("{}:", pkg);
//!     println!("{}", diff.trim());
//! }
//!
//! # Ok(())
//! # }
//! ```
//!
//! ## Diff View
//! ```no_run
//! use aur_fetch::Fetch;
//! use std::process::Command;
//!
//! # use aur_fetch::Error;
//! # async fn foo() -> Result<(), Error> {
//!
//! let pkgs = vec!["discord", "spotify", "pacman-git"];
//!
//! // Create our handle
//! let fetch = Fetch::new()?;
//!
//! // Clone/Fetch the packages.
//! let fetched = fetch.download(&pkgs)?;
//!
//! // Merge the changes.
//! fetch.merge(&fetched)?;
//!
//! // Save diffs to cache.
//! fetch.save_diffs(&fetched)?;
//!
//! // Make a view of the new files so we can easily see them in the file browser
//! let dir = fetch.make_view( "/tmp/aur_view",  &pkgs, &fetched)?;
//! Command::new("vifm").arg("/tmp/aur_view").spawn()?.wait()?;
//!
//! # Ok(())
//! # }
//! ```
//!
//! ## Using a Callback
//!
//! ```no_run
//! use aur_fetch::Fetch;
//!
//! # use aur_fetch::Error;
//! # async fn foo() -> Result<(), Error> {
//!
//! let pkgs = vec!["discord", "spotify", "pacman-git"];
//!
//! // Create our handle
//! let fetch = Fetch::new()?;
//!
//! // Clone/Fetch the packages.
//! let feteched = fetch.download(&pkgs)?;
//!
//! // Download the packages, printing downloads as they complete.
//! let fetched = fetch.download_cb(&pkgs, |cb| {
//!     println!("Downloaded ({:0pad$}/{:0pad$}): {}", cb.n, pkgs.len(), cb.pkg, pad = 3);
//! })?;
//!
//! // Merge the changes.
//! // In a real tool you would ask for user conformation before this
//! // As long as the changes are not merged this process can always be restarted and the diffs
//! // perserved
//! fetch.merge(&fetched)?;
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
