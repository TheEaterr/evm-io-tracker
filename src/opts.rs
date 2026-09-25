extern crate structopt;

pub use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(about = "EVM IO Tracker.", rename_all = "kebab-case")]
pub enum Options {
    SortAccounts(SortAccountsOptions),
    Fetch(FetchOptions),
    Combine(CombineOptions),
    Seal(SealOptions),
    Analyze(AnalyzeOptions),
}

#[derive(Debug, StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub struct FetchOptions {
    #[structopt(long, default_value = "http://127.0.0.1:8545/")]
    pub node_url: String,

    #[structopt(long)]
    pub start_block: usize,

    #[structopt(long, default_value = "50")]
    pub batch_size: usize,

    #[structopt(long, default_value = "data")]
    pub trace_path: String,

    #[structopt(long)]
    pub dump_raw_data: bool,

    #[structopt(long, default_value = "data")]
    pub raw_data_path: String,
}

#[derive(Debug, StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub struct CombineOptions {
    #[structopt(long)]
    pub start_block: Option<usize>,

    #[structopt(long)]
    pub end_block: Option<usize>,

    #[structopt(long, default_value = "data")]
    pub path: String,
}

#[derive(Debug, StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub struct SealOptions {
    #[structopt(long, default_value = "http://127.0.0.1:8545/")]
    pub node_url: String,

    #[structopt(long)]
    pub input: String,

    #[structopt(long, default_value = "data")]
    pub output: String,
}

#[derive(Debug, StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub struct AnalyzeOptions {
    #[structopt(long)]
    pub input: String,
}

#[derive(Debug, StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub struct SortAccountsOptions {
    #[structopt(long, default_value = "http://127.0.0.1:8545/")]
    pub node_url: String,

    #[structopt(long)]
    pub start_block: usize,

    #[structopt(long)]
    pub end_block: usize,

    #[structopt(long, default_value = "50")]
    pub batch_size: usize,

    #[structopt(long, default_value = "data")]
    pub sorted_accounts_path: String,
}
