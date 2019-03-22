/// Callback called whenever a download completes.
#[derive(Debug)]
pub struct Callback {
    /// The name of the package that completed.
    pub pkg: String,
    /// The amount of packages that have finished downloading.
    pub n: usize,
}
