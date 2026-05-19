use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(name = "kleya", version, about = "Bootstrap AWS spot dev boxes")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Cmd,
    #[arg(long, global = true, env = "KLEYA_CONFIG")]
    pub config: Option<String>,
    #[arg(long, global = true, env = "KLEYA_PROFILE")]
    pub profile: Option<String>,
    #[arg(long, global = true, env = "KLEYA_REGION")]
    pub region: Option<String>,
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,
    #[arg(
        long,
        global = true,
        value_enum,
        env = "KLEYA_LOG_FORMAT",
        default_value_t = LogFormat::Text
    )]
    pub log_format: LogFormat,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum LogFormat {
    Text,
    Json,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    Template {
        #[command(subcommand)]
        action: TemplateCmd,
    },
    Launch(LaunchArgs),
    List(ListArgs),
    Connect(ConnectArgs),
    Terminate(TerminateArgs),
    Config {
        #[command(subcommand)]
        action: ConfigCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum TemplateCmd {
    Create(TemplateCreateArgs),
    Update(TemplateUpdateArgs),
    List(TemplateListArgs),
    Delete {
        name: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Args, Debug)]
pub struct TemplateCreateArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub ami: Option<String>,
    #[arg(long)]
    pub instance_type: Option<String>,
    #[arg(long)]
    pub key_name: Option<String>,
    #[arg(long)]
    pub user_data: Option<String>,
}

#[derive(Args, Debug)]
pub struct TemplateUpdateArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub ami: Option<String>,
    #[arg(long)]
    pub instance_type: Option<String>,
    #[arg(long)]
    pub key_name: Option<String>,
    #[arg(long)]
    pub user_data: Option<String>,
}

#[derive(Args, Debug, Default)]
pub struct TemplateListArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct LaunchArgs {
    #[arg(long)]
    pub template: Option<String>,
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub instance_type: Option<String>,
    #[arg(long, value_enum)]
    pub market: Option<Market>,
    #[arg(long)]
    pub connect: bool,
    #[arg(long)]
    pub wait_bootstrap: bool,
    #[arg(long)]
    pub no_wait_bootstrap: bool,
    #[arg(long)]
    pub dry_run: bool,
    /// On (local-absent, EC2-present): delete the EC2 key, regenerate
    /// locally, and re-register. Without this flag the case is fatal
    /// `Error::KeyOrphaned`.
    #[arg(long)]
    pub regenerate_key: bool,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum Market {
    Spot,
    OnDemand,
}

#[derive(Args, Debug)]
pub struct ListArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct ConnectArgs {
    pub name: String,
    #[arg(long)]
    pub print: bool,
    #[arg(long)]
    pub no_tmux: bool,
    #[arg(long)]
    pub tmux_session: Option<String>,
    #[arg(long, name = "instance-id")]
    pub instance_id: Option<String>,
}

#[derive(Args, Debug)]
pub struct TerminateArgs {
    pub name: String,
    #[arg(long)]
    pub yes: bool,
}

#[derive(Subcommand, Debug)]
pub enum ConfigCmd {
    Show,
    Path,
}
