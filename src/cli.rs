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
    Write(WriteArgs),
    Derive(DeriveCommand),
    Lint(LintArgs),
    Link(LinkCommand),
    Impact(ImpactArgs),
    Plan(PlanCommand),
    Agent(AgentCommand),
    Search(SearchCommand),
    Ask(AskArgs),
}

#[derive(Args, Debug)]
pub(crate) struct DeriveCommand {
    #[command(subcommand)]
    pub(crate) command: DeriveSubcommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum DeriveSubcommand {
    Design(DeriveDesignArgs),
}

#[derive(Args, Debug)]
pub(crate) struct DeriveDesignArgs {
    #[arg(long)]
    pub(crate) from: String,
    #[arg(long)]
    pub(crate) path: Option<String>,
    #[arg(long)]
    pub(crate) title: Option<String>,
    #[arg(long = "type", default_value = "component_design")]
    pub(crate) node_type: String,
    #[arg(long, default_value = "review")]
    pub(crate) status: String,
    #[arg(long)]
    pub(crate) body: Option<String>,
    #[arg(long)]
    pub(crate) body_file: Option<String>,
    #[arg(long, default_value = "derived design from source spec")]
    pub(crate) rationale: String,
    #[arg(long = "term")]
    pub(crate) terms: Vec<String>,
}

#[derive(Args, Debug)]
pub(crate) struct WriteArgs {
    #[arg(long)]
    pub(crate) path: String,
    #[arg(long)]
    pub(crate) id: Option<String>,
    #[arg(long = "type")]
    pub(crate) node_type: Option<String>,
    #[arg(long)]
    pub(crate) status: Option<String>,
    #[arg(long)]
    pub(crate) title: Option<String>,
    #[arg(long)]
    pub(crate) body: Option<String>,
    #[arg(long)]
    pub(crate) body_file: Option<String>,
    #[arg(long = "term")]
    pub(crate) terms: Vec<String>,
}

#[derive(Args, Debug)]
pub(crate) struct InitArgs {
    #[arg(long)]
    pub(crate) sync: bool,
    #[arg(long, value_enum)]
    pub(crate) agent: Vec<AgentTarget>,
    #[arg(long)]
    pub(crate) agent_sync: bool,
    #[arg(long, value_enum, default_value_t = AgentOutput::Docs)]
    pub(crate) agent_output: AgentOutput,
    #[arg(long)]
    pub(crate) codex_home: Option<String>,
    #[arg(long)]
    pub(crate) claude_dir: Option<String>,
    #[arg(long, value_enum, default_value_t = TemplateSource::Github)]
    pub(crate) template_source: TemplateSource,
    #[arg(long, default_value = "https://github.com/nurliv/foundry.git")]
    pub(crate) template_repo: String,
    #[arg(long, default_value = "main")]
    pub(crate) template_ref: String,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum AgentTarget {
    Codex,
    Claude,
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
pub(crate) struct AgentCommand {
    #[command(subcommand)]
    pub(crate) command: AgentSubcommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum AgentSubcommand {
    Doctor(AgentDoctorArgs),
}

#[derive(Args, Debug)]
pub(crate) struct AgentDoctorArgs {
    #[arg(long, value_enum)]
    pub(crate) agent: Vec<AgentTarget>,
    #[arg(long, value_enum, default_value_t = AgentFormat::Table)]
    pub(crate) format: AgentFormat,
    #[arg(long, value_enum, default_value_t = AgentOutput::Docs)]
    pub(crate) agent_output: AgentOutput,
    #[arg(long)]
    pub(crate) codex_home: Option<String>,
    #[arg(long)]
    pub(crate) claude_dir: Option<String>,
    #[arg(long, value_enum, default_value_t = TemplateSource::Github)]
    pub(crate) template_source: TemplateSource,
    #[arg(long, default_value = "https://github.com/nurliv/foundry.git")]
    pub(crate) template_repo: String,
    #[arg(long, default_value = "main")]
    pub(crate) template_ref: String,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentFormat {
    Table,
    Json,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentOutput {
    Docs,
    Install,
    Both,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TemplateSource {
    Local,
    Github,
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
