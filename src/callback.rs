/// Callback called whenever a download completes.
#[derive(Debug)]
pub struct Callback<'a> {
    /// The name of the package that completed.
    pub pkg: &'a str,
    /// The amount of packages that have finished downloading.
    pub n: usize,
}
