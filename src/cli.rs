use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "foundry")]
#[command(about = "Spec graph CLI for AI-driven development support")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    Spec(SpecCommand),
}

#[derive(Args, Debug)]
pub(crate) struct SpecCommand {
    #[command(subcommand)]
    pub(crate) command: SpecSubcommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum SpecSubcommand {
    Init(InitArgs),
    Lint(LintArgs),
    Link(LinkCommand),
    Impact(ImpactArgs),
    Plan(PlanCommand),
    Search(SearchCommand),
    Ask(AskArgs),
}

#[derive(Args, Debug)]
pub(crate) struct InitArgs {
    #[arg(long)]
    pub(crate) sync: bool,
}

#[derive(Args, Debug)]
pub(crate) struct LintArgs {
    #[arg(long, value_enum, default_value_t = LintFormat::Table)]
    pub(crate) format: LintFormat,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LintFormat {
    Table,
    Json,
}

#[derive(Args, Debug)]
pub(crate) struct ImpactArgs {
    pub(crate) node_id: String,
    #[arg(long, default_value_t = 2)]
    pub(crate) depth: usize,
    #[arg(long, value_enum, default_value_t = ImpactFormat::Table)]
    pub(crate) format: ImpactFormat,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImpactFormat {
    Table,
    Json,
}

#[derive(Args, Debug)]
pub(crate) struct PlanCommand {
    #[command(subcommand)]
    pub(crate) command: PlanSubcommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum PlanSubcommand {
    Ready(PlanReadyArgs),
    Batches(PlanBatchesArgs),
}

#[derive(Args, Debug)]
pub(crate) struct PlanReadyArgs {
    #[arg(long, value_enum, default_value_t = PlanFormat::Table)]
    pub(crate) format: PlanFormat,
}

#[derive(Args, Debug)]
pub(crate) struct PlanBatchesArgs {
    #[arg(long, value_enum, default_value_t = PlanFormat::Table)]
    pub(crate) format: PlanFormat,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlanFormat {
    Table,
    Json,
}

#[derive(Args, Debug)]
pub(crate) struct SearchCommand {
    #[command(subcommand)]
    pub(crate) command: SearchSubcommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum SearchSubcommand {
    Index(SearchIndexArgs),
    Query(SearchQueryArgs),
    Doctor,
}

#[derive(Args, Debug)]
pub(crate) struct SearchIndexArgs {
    #[arg(long)]
    pub(crate) rebuild: bool,
}

#[derive(Args, Debug)]
pub(crate) struct SearchQueryArgs {
    pub(crate) query: String,
    #[arg(long, default_value_t = 10)]
    pub(crate) top_k: usize,
    #[arg(long, value_enum, default_value_t = SearchFormat::Table)]
    pub(crate) format: SearchFormat,
    #[arg(long, value_enum, default_value_t = SearchMode::Lexical)]
    pub(crate) mode: SearchMode,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SearchFormat {
    Table,
    Json,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SearchMode {
    Lexical,
    Hybrid,
}

#[derive(Args, Debug)]
pub(crate) struct AskArgs {
    pub(crate) question: String,
    #[arg(long, default_value_t = 5)]
    pub(crate) top_k: usize,
    #[arg(long, value_enum, default_value_t = SearchMode::Hybrid)]
    pub(crate) mode: SearchMode,
    #[arg(long, value_enum, default_value_t = AskFormat::Table)]
    pub(crate) format: AskFormat,
    #[arg(long)]
    pub(crate) explain: bool,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AskFormat {
    Table,
    Json,
}

#[derive(Args, Debug)]
pub(crate) struct LinkCommand {
    #[command(subcommand)]
    pub(crate) command: LinkSubcommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum LinkSubcommand {
    Add(LinkAddArgs),
    Remove(LinkRemoveArgs),
    List(LinkListArgs),
    Propose(LinkProposeArgs),
}

#[derive(Args, Debug)]
pub(crate) struct LinkAddArgs {
    #[arg(long)]
    pub(crate) from: String,
    #[arg(long)]
    pub(crate) to: String,
    #[arg(long)]
    pub(crate) r#type: String,
    #[arg(long)]
    pub(crate) rationale: String,
    #[arg(long, default_value_t = 1.0)]
    pub(crate) confidence: f64,
}

#[derive(Args, Debug)]
pub(crate) struct LinkRemoveArgs {
    #[arg(long)]
    pub(crate) from: String,
    #[arg(long)]
    pub(crate) to: String,
    #[arg(long)]
    pub(crate) r#type: String,
}

#[derive(Args, Debug)]
pub(crate) struct LinkListArgs {
    #[arg(long)]
    pub(crate) node: String,
}

#[derive(Args, Debug)]
pub(crate) struct LinkProposeArgs {
    #[arg(long)]
    pub(crate) node: Option<String>,
    #[arg(long)]
    pub(crate) from: Option<String>,
    #[arg(long)]
    pub(crate) to: Option<String>,
    #[arg(long, default_value = "impacts")]
    pub(crate) r#type: String,
    #[arg(long)]
    pub(crate) rationale: Option<String>,
    #[arg(long, default_value_t = 0.6)]
    pub(crate) confidence: f64,
    #[arg(long, default_value_t = 3)]
    pub(crate) limit: usize,
}
