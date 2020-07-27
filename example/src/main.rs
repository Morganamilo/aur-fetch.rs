use aur_fetch::{Error, Handle};
use indicatif::{ProgressBar, ProgressStyle};
use std::env;

fn main() {
    if let Err(err) = run() {
        eprintln!("{}", err);
    }
}

fn run() -> Result<(), Error> {
    let h = Handle::new()?;
    let args = env::args();
    let pkgs = args.skip(1).collect::<Vec<_>>();

    if !pkgs.is_empty() {
        let pb = ProgressBar::new(pkgs.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template(" {prefix} [{wide_bar}] {pos}/{len} ")
                .progress_chars("-> "),
        );
        pb.set_prefix("Downloading Packages");

        let fetched = h.download_cb(&pkgs, |cb| {
            pb.println(cb.output);
            pb.inc(1);
        })?;

        pb.finish();

        let merge = h.needs_merge(&fetched)?;

        if !merge.is_empty() {
            println!();
            let pb = ProgressBar::new(merge.len() as u64);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template(" {prefix} [{wide_bar}] {pos}/{len} ")
                    .progress_chars("-> "),
            );
            pb.set_prefix("Merging Packages");

            h.merge_cb(&merge, |cb| {
                pb.println(cb.output);
                pb.inc(1);
            })?;

            pb.finish();
        }
    }

    Ok(())
}
