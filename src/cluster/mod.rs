pub(crate) mod reader;
pub(crate) mod writer;
/// Whether `NoFatChain` bit is set or cleared.
#[derive(Debug)]
pub(crate) enum ClusterChainOptions {
    // If the NoFatChain bit is 1 then DataLength must not be zero
    Contiguous { data_length: u64 },
    Fat { data_length: Option<u64> },
}

impl Default for ClusterChainOptions {
    fn default() -> Self {
        Self::Fat {
            data_length: Option::None,
        }
    }
}
