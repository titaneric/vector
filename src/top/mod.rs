mod cmd;
mod dashboard;
mod events;
mod metrics;
mod state;

use structopt::StructOpt;
use url::Url;

pub use cmd::cmd;

#[derive(StructOpt, Debug, Clone)]
#[structopt(rename_all = "kebab-case")]
pub struct Opts {
    /// Interval to sample metrics at, in milliseconds
    #[structopt(default_value = "500", short = "i", long)]
    interval: u64,

    /// Vector GraphQL API server endpoint
    #[structopt(short, long)]
    url: Option<Url>,

    /// Humanize metrics, using numeric suffixes - e.g. 1,100 = 1.10 k, 1,000,000 = 1.00 M
    #[structopt(short, long)]
    human_metrics: bool,
}
