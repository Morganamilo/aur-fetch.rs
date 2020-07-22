/// Callback called whenever a download completes.
#[derive(Debug)]
pub struct Callback<'a> {
    /// The name of the package that completed.
    pub pkg: &'a str,
    /// The amount of packages that have finished downloading.
    pub n: usize,
    /// Output of the git command called to download the package.
    pub output: &'a str,
}
