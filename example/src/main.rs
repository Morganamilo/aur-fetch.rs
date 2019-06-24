use aur_fetch::{Error, Handle};

use std::env;

use indicatif::{ProgressBar, ProgressStyle};

fn main() {
    if let Err(err) = run() {
        eprintln!("{}", err);
    }
}

fn run() -> Result<(), Error> {
    let h = Handle::new()?;
    let args = env::args();
    let pkgs = &args.skip(1).collect::<Vec<_>>();

    let pb = ProgressBar::new(pkgs.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(" {prefix} [{wide_bar}] {pos}/{len} ")
            .progress_chars("-> "),
    );

    let fetched = h.download_cb(&pkgs, |cb| {
        pb.println(format!(":: {}", &cb.pkg));
        pb.inc(1);
        pb.set_prefix("Downloading Packages");
    })?;

    pb.finish_and_clear();

    let merge = h.needs_merge(&fetched)?;
    println!("Merging...");
    h.merge(&merge)?;
    Ok(())
}
