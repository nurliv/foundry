use super::*;
mod retrieval;
mod synthesis;

#[derive(Debug, Serialize)]
struct AskCitation {
    id: String,
    title: String,
    path: String,
}

#[derive(Debug, Serialize)]
struct AskEvidence {
    id: String,
    snippet: String,
    score: f64,
}

#[derive(Debug, Serialize)]
pub(super) struct AskExplanation {
    pub(super) id: String,
    pub(super) reason: String,
}

#[derive(Debug, Serialize)]
struct AskOutput {
    question: String,
    mode: String,
    answer: String,
    confidence: f64,
    citations: Vec<AskCitation>,
    evidence: Vec<AskEvidence>,
    explanations: Vec<AskExplanation>,
    gaps: Vec<String>,
}

pub(super) fn run_ask(args: &AskArgs) -> Result<()> {
    let config = load_runtime_config();
    let retrieved = retrieval::retrieve_ask_inputs(args)?;
    let output = synthesis::synthesize_ask_output(
        args,
        retrieved.mode,
        retrieved.hits,
        &retrieved.meta_by_id,
        &config.ask,
    );
    match args.format {
        AskFormat::Json => println!("{}", serde_json::to_string_pretty(&output)?),
        AskFormat::Table => print_ask_table(&output),
    }
    Ok(())
}

pub(super) fn expand_ask_context(
    hits: &[SearchHit],
    meta_by_id: &HashMap<String, SpecNodeMeta>,
    limit: usize,
    weights: &AskEdgeWeightConfig,
) -> (Vec<String>, Vec<String>) {
    retrieval::expand_ask_context(hits, meta_by_id, limit, weights)
}

#[allow(dead_code)]
pub(super) fn build_ask_explanations(
    question: &str,
    hits: &[SearchHit],
    related_ids: &[String],
    meta_by_id: &HashMap<String, SpecNodeMeta>,
    weights: &AskEdgeWeightConfig,
) -> Vec<AskExplanation> {
    synthesis::build_ask_explanations(question, hits, related_ids, meta_by_id, weights)
}

fn print_ask_table(output: &AskOutput) {
    synthesis::print_ask_table(output);
}
pub(super) fn edge_weight(edge_type: &str, w: &AskEdgeWeightConfig) -> f64 {
    match edge_type {
        "depends_on" => w.depends_on,
        "tests" => w.tests,
        "refines" => w.refines,
        "impacts" => w.impacts,
        "conflicts_with" => w.conflicts_with,
        _ => 0.0,
    }
}
