use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::Parser;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;
use crate::cli::*;
mod ask;
mod core;
mod impact;
mod init;
mod lint;
mod link;
mod search;
use core::*;
use impact::*;
use init::*;
use lint::*;
use link::*;
use search::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpecNodeMeta {
    id: String,
    #[serde(rename = "type")]
    node_type: String,
    status: String,
    title: String,
    body_md_path: String,
    terms: Vec<String>,
    hash: String,
    edges: Vec<SpecEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpecEdge {
    to: String,
    #[serde(rename = "type")]
    edge_type: String,
    rationale: String,
    confidence: f64,
    status: String,
}

#[derive(Default)]
struct InitSummary {
    created: usize,
    updated: usize,
    skipped: usize,
    errors: usize,
}

#[derive(Default)]
struct LintState {
    errors: Vec<String>,
}

const NODE_TYPES: &[&str] = &[
    "product_goal",
    "feature_requirement",
    "non_functional_requirement",
    "constraint",
    "domain_concept",
    "decision",
    "workflow",
    "api_contract",
    "data_contract",
    "test_spec",
];

const NODE_STATUSES: &[&str] = &["draft", "review", "active", "deprecated", "archived"];
const EDGE_TYPES: &[&str] = &["depends_on", "refines", "conflicts_with", "tests", "impacts"];
const EDGE_STATUSES: &[&str] = &["confirmed", "proposed"];
const EMBEDDING_DIM: usize = 256;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct RuntimeConfig {
    ask: AskRuntimeConfig,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            ask: AskRuntimeConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct AskRuntimeConfig {
    neighbor_limit: usize,
    snippet_count_in_answer: usize,
    edge_weight: AskEdgeWeightConfig,
}

impl Default for AskRuntimeConfig {
    fn default() -> Self {
        Self {
            neighbor_limit: 5,
            snippet_count_in_answer: 2,
            edge_weight: AskEdgeWeightConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct AskEdgeWeightConfig {
    depends_on: f64,
    tests: f64,
    refines: f64,
    impacts: f64,
    conflicts_with: f64,
}

impl Default for AskEdgeWeightConfig {
    fn default() -> Self {
        Self {
            depends_on: 1.0,
            tests: 0.8,
            refines: 0.7,
            impacts: 0.6,
            conflicts_with: 1.2,
        }
    }
}


pub fn run_main() {
    match run() {
        Ok(exit_code) => std::process::exit(exit_code),
        Err(err) => {
            eprintln!("error: {err:#}");
            std::process::exit(2);
        }
    }
}

fn run() -> Result<i32> {
    let cli = Cli::parse();
    match cli.command {
        Command::Spec(spec) => match spec.command {
            SpecSubcommand::Init(args) => {
                run_init(args.sync)?;
                Ok(0)
            }
            SpecSubcommand::Lint => Ok(run_lint()?),
            SpecSubcommand::Link(link) => {
                run_link(link)?;
                Ok(0)
            }
            SpecSubcommand::Impact(args) => {
                run_impact(&args)?;
                Ok(0)
            }
            SpecSubcommand::Search(search) => {
                run_search(search)?;
                Ok(0)
            }
            SpecSubcommand::Ask(args) => {
                ask::run_ask(&args)?;
                Ok(0)
            }
        },
    }
}

fn load_runtime_config() -> RuntimeConfig {
    let path = Path::new(".foundry/config.json");
    let raw = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(_) => return RuntimeConfig::default(),
    };
    serde_json::from_str::<RuntimeConfig>(&raw).unwrap_or_default()
}

fn unix_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "app/tests.rs"]
mod tests;
