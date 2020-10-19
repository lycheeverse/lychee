use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "lychee",
    about = "A boring link checker for my projects (and maybe yours)"
)]
pub(crate) struct LycheeOptions {
    /// Input files
    pub inputs: Vec<String>,

    #[structopt(flatten)]
    pub config: Config,
}

#[derive(Debug, StructOpt)]
pub(crate) struct Config {
    /// Verbose program output
    #[structopt(short, long)]
    pub verbose: bool,

    /// Show progress
    #[structopt(short, long)]
    pub progress: bool,

    /// Maximum number of allowed redirects
    #[structopt(short, long, default_value = "10")]
    pub max_redirects: usize,

    /// Number of threads to utilize.
    /// Defaults to number of cores available to the system
    #[structopt(short = "T", long)]
    pub threads: Option<usize>,

    /// User agent
    #[structopt(short, long, default_value = "curl/7.71.1")]
    pub user_agent: String,

    /// Proceed for server connections considered insecure (invalid TLS)
    #[structopt(short, long)]
    pub insecure: bool,

    /// Only test links with the given scheme (e.g. https)
    #[structopt(short, long)]
    pub scheme: Option<String>,

    /// Exclude URLs from checking (supports regex)
    #[structopt(short, long)]
    pub exclude: Vec<String>,

    /// Exclude all private IPs from checking.
    /// Equivalent to `--exclude-private --exclude-link-local --exclude-loopback`
    #[structopt(short = "E", long)]
    pub exclude_all_private: bool,

    /// Exclude private IP address ranges from checking
    #[structopt(long)]
    pub exclude_private: bool,

    /// Exclude link-local IP address range from checking
    #[structopt(long)]
    pub exclude_link_local: bool,

    /// Exclude loopback IP address range from checking
    #[structopt(long)]
    pub exclude_loopback: bool,

    /// Custom request headers
    #[structopt(short, long)]
    pub headers: Vec<String>,

    /// Comma-separated list of accepted status codes for valid links
    #[structopt(short, long)]
    pub accept: Option<String>,

    /// Website timeout from connect to response finished
    #[structopt(short, long, default_value = "20")]
    pub timeout: String,

    /// Request method
    #[structopt(short = "M", long, default_value = "get")]
    pub method: String,
}
