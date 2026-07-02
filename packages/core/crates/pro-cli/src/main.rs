use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use pro_audit::AuditLog;
use pro_capability::{profiles, Grant, GrantStore};
use pro_config::{Config, ProfileName};
use pro_core::{Engine, EngineError, ToolAdapter};
use pro_provider::OpenAiCompatibleClient;
use pro_sandbox::EnforcerRegistry;
use pro_supply_chain::{CratesIoRegistryClient, NpmRegistryClient, SupplyChainGate};

// Distinct non-zero exit codes per failure class
// (`01-architecture.md`'s clig.dev-style dual-mode CLI contract).
const EXIT_CONFIG_ERROR: u8 = 1;
const EXIT_CAPABILITY_DENIED: u8 = 2;
const EXIT_PROVIDER_ERROR: u8 = 3;
const EXIT_SANDBOX_ERROR: u8 = 4;
const EXIT_USAGE_ERROR: u8 = 5;
const EXIT_SUPPLY_CHAIN_REJECTED: u8 = 6;
const EXIT_GROUNDING_FAILED: u8 = 7;
const EXIT_REVIEW_DISAGREEMENT: u8 = 8;
const EXIT_NO_WINNER: u8 = 9;

#[derive(Parser)]
#[command(name = "pro", version, about = "pro — secure-by-default AI engineering core (Phase 0 scaffold)")]
struct Cli {
    /// Path to the project config file.
    #[arg(long, global = true, default_value = "pro.config.toml")]
    config: PathBuf,

    /// Emit machine-readable JSON on stdout instead of plain text.
    #[arg(long, global = true)]
    json: bool,

    /// Suppress non-essential stderr diagnostics.
    #[arg(long, global = true)]
    quiet: bool,

    /// Never block on interactive input, matching `CI=true` behavior even
    /// with a TTY attached.
    #[arg(long, global = true)]
    no_input: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Inspect the resolved project configuration.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Inspect the active capability grant inventory.
    Grant {
        #[command(subcommand)]
        action: GrantAction,
    },
    /// Send a prompt to the configured provider end-to-end.
    Ask {
        prompt: Option<String>,
        /// Run the confidence-signal gate (N resamples + semantic-entropy
        /// clustering) alongside the answer.
        #[arg(long)]
        confidence: bool,
        /// Use the model cascade (requires `[cascade_provider]` in
        /// config); escalates to the strong tier on low confidence.
        #[arg(long)]
        cascade: bool,
        #[arg(long, default_value_t = 3)]
        samples: usize,
        #[arg(long, default_value_t = 0.7)]
        temperature: f32,
        #[arg(long, default_value_t = 0.3)]
        entropy_threshold: f32,
    },
    /// Score a unified diff against the AI Slop Index.
    Slop {
        /// Path to a unified diff file; omit or pass "-" to read stdin.
        path: Option<PathBuf>,
        /// Also run the LLM-judge axes (costs an API call).
        #[arg(long)]
        full: bool,
        /// Declared scope paths for the scope-discipline heuristic.
        #[arg(long = "scope")]
        scope: Vec<PathBuf>,
    },
    /// Run the supply-chain gate against a package.
    SupplyChain {
        #[command(subcommand)]
        action: SupplyChainAction,
    },
    /// Print the tree-sitter repo map (Aider-style dependency graph),
    /// ranked by PageRank, elided to a token budget.
    Repomap {
        #[arg(long, default_value_t = 4000)]
        budget: usize,
    },
    /// Run the deterministic grounding gate against a file's current
    /// content — Rust gets a real rust-analyzer-backed check, other
    /// languages use the AST/symbol-table fallback.
    Ground { path: PathBuf },
    /// Review a diff: simple (default), adversarial (CLAIM/EXTRACT/DOUBT/
    /// RECONCILE), or orthogonal (a second, independent provider).
    Review {
        /// Path to a unified diff file; omit or pass "-" to read stdin.
        diff_file: Option<PathBuf>,
        #[arg(long)]
        adversarial: bool,
        /// Requires `[cascade_provider]` in config as the second reviewer.
        #[arg(long)]
        orthogonal: bool,
    },
    /// Codegen a task across `--parallel` git worktrees at once, verify
    /// each with a real command, keep only the first that passes.
    Run {
        task: String,
        #[arg(long, default_value_t = 1)]
        parallel: usize,
        #[arg(long)]
        verify: String,
        /// Files in scope for the codegen task, relative to the repo root.
        #[arg(long = "file")]
        files: Vec<PathBuf>,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    Show,
}

#[derive(Subcommand)]
enum GrantAction {
    List,
}

#[derive(Subcommand)]
enum SupplyChainAction {
    Check { ecosystem: EcosystemArg, package: String },
}

#[derive(ValueEnum, Clone, Copy)]
enum EcosystemArg {
    Npm,
    Crates,
}

fn interactive_allowed(cli: &Cli) -> bool {
    if cli.no_input {
        return false;
    }
    if std::env::var("CI").map(|v| v == "true").unwrap_or(false) {
        return false;
    }
    std::io::stdin().is_terminal()
}

fn default_profile_capabilities(config: &Config, project_root: &Path) -> Vec<pro_capability::Capability> {
    match config.profile.default {
        ProfileName::ReadOnly => profiles::read_only(project_root),
        ProfileName::Workspace => profiles::workspace(project_root),
        ProfileName::DangerFullAccess => profiles::danger_full_access(),
    }
}

fn build_grant_store(config: &Config, project_root: &Path) -> GrantStore {
    let mut store = GrantStore::new();
    for capability in default_profile_capabilities(config, project_root) {
        // Every capability instantiated from a profile starts at tier 0
        // (shadow) — see `02-security-core.md` §1.
        store.add_grant(Grant::new_shadow(capability));
    }
    store
}

#[tokio::main]
async fn main() -> ExitCode {
    // NO_COLOR: respected trivially — this CLI emits no ANSI color codes
    // anywhere yet, so no branching is needed, but the convention is
    // acknowledged here for when styled output is added.
    let _no_color = std::env::var("NO_COLOR").is_ok();

    let cli = Cli::parse();

    if !cli.quiet {
        eprintln!("pro: using config {}", cli.config.display());
    }

    let config = match Config::load(&cli.config) {
        Ok(config) => config,
        Err(err) => {
            eprintln!("config error: {err}");
            return ExitCode::from(EXIT_CONFIG_ERROR);
        }
    };

    let project_root = cli
        .config
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    let grants = build_grant_store(&config, &project_root);
    let audit = AuditLog::new(project_root.join(&config.audit.log_path));
    let interactive = interactive_allowed(&cli);
    let json = cli.json;

    match cli.command {
        Command::Config { action: ConfigAction::Show } => cmd_config_show(&config, json),
        Command::Grant { action: GrantAction::List } => cmd_grant_list(&grants, json),
        Command::Ask { prompt, confidence, cascade, samples, temperature, entropy_threshold } => {
            let Some(prompt) = prompt else {
                // Dual-mode contract: never block on a prompt in a
                // non-interactive context, and this Phase 0 CLI has no
                // interactive readline loop yet either way.
                let hint = if interactive {
                    "pass the prompt as an argument: `pro ask \"...\"` (interactive prompt entry isn't implemented yet)"
                } else {
                    "pass the prompt as an argument: `pro ask \"...\"` (no TTY / --no-input / CI=true: refusing to block)"
                };
                eprintln!("error: 'ask' requires a prompt argument — {hint}");
                return ExitCode::from(EXIT_USAGE_ERROR);
            };
            cmd_ask(
                config,
                grants,
                audit,
                &prompt,
                AskMode { confidence, cascade, samples, temperature, entropy_threshold },
                json,
            )
            .await
        }
        Command::Slop { path, full, scope } => cmd_slop(&config, path.as_deref(), full, &scope, json).await,
        Command::SupplyChain { action: SupplyChainAction::Check { ecosystem, package } } => {
            cmd_supply_chain_check(ecosystem, &package, json).await
        }
        Command::Repomap { budget } => cmd_repomap(&project_root, budget, json),
        Command::Ground { path } => cmd_ground(&path, json).await,
        Command::Review { diff_file, adversarial, orthogonal } => {
            cmd_review(&config, diff_file.as_deref(), adversarial, orthogonal, json).await
        }
        Command::Run { task, parallel, verify, files } => {
            cmd_run(config, project_root, &task, parallel, &verify, &files, json).await
        }
    }
}

fn cmd_config_show(config: &Config, json: bool) -> ExitCode {
    if json {
        println!("{}", serde_json::to_string_pretty(config).unwrap_or_default());
    } else {
        println!("provider: {} ({})", config.provider.name, config.provider.base_url);
        println!("model: {}", config.provider.model);
        println!("profile: {:?}", config.profile.default);
        println!("audit log: {} (retain {} days)", config.audit.log_path, config.audit.retention_days);
    }
    ExitCode::SUCCESS
}

fn cmd_grant_list(grants: &GrantStore, json: bool) -> ExitCode {
    if json {
        let rows: Vec<_> = grants
            .grants()
            .iter()
            .map(|g| {
                serde_json::json!({
                    "kind": g.capability.kind(),
                    "autonomy_tier": format!("{:?}", g.autonomy_tier),
                    "content_hash": g.content_hash.to_string(),
                    "granted_at": g.granted_at.format(&time::format_description::well_known::Rfc3339).unwrap_or_default(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&rows).unwrap_or_default());
    } else {
        for grant in grants.grants() {
            println!(
                "{:<10} tier={:<16?} hash={}",
                grant.capability.kind(),
                grant.autonomy_tier,
                grant.content_hash
            );
        }
    }
    ExitCode::SUCCESS
}

struct AskMode {
    confidence: bool,
    cascade: bool,
    samples: usize,
    temperature: f32,
    entropy_threshold: f32,
}

fn provider_api_key(cfg: &pro_config::ProviderConfig) -> Option<String> {
    if cfg.api_key_env.is_empty() {
        None
    } else {
        std::env::var(&cfg.api_key_env).ok()
    }
}

async fn cmd_ask(config: Config, grants: GrantStore, audit: AuditLog, prompt: &str, mode: AskMode, json: bool) -> ExitCode {
    let api_key = provider_api_key(&config.provider);
    let provider =
        OpenAiCompatibleClient::new(config.provider.base_url.clone(), api_key, config.provider.model.clone());
    let engine = Engine::new(config, grants, audit, EnforcerRegistry::new(), Box::new(provider));

    if mode.cascade {
        return match engine.ask_cascaded(prompt, mode.samples, mode.temperature, mode.entropy_threshold).await {
            Ok(outcome) => {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "content": outcome.response.content,
                            "escalated": outcome.escalated,
                            "confidence": outcome.confidence,
                        })
                    );
                } else {
                    if outcome.escalated {
                        eprintln!("(escalated to strong tier — cheap-tier answers disagreed)");
                    }
                    println!("{}", outcome.response.content);
                }
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("ask --cascade failed: {err}");
                exit_code_for(&err)
            }
        };
    }

    if mode.confidence {
        return match engine.ask_with_confidence(prompt, mode.samples, mode.temperature, mode.entropy_threshold).await
        {
            Ok((response, report)) => {
                if json {
                    println!("{}", serde_json::json!({ "content": response.content, "confidence": report }));
                } else {
                    println!("{}", response.content);
                    eprintln!(
                        "(confidence: entropy {:.3} across {} clusters via {})",
                        report.entropy, report.num_clusters, report.strategy
                    );
                }
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("ask --confidence failed: {err}");
                exit_code_for(&err)
            }
        };
    }

    match engine.ask(prompt).await {
        Ok(response) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&response.raw).unwrap_or_default());
            } else {
                println!("{}", response.content);
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("ask failed: {err}");
            exit_code_for(&err)
        }
    }
}

/// Shared by every command that takes "a diff, or stdin" (`slop`, `review`):
/// reads `path`, or stdin when `path` is `None`/`-`.
fn read_diff_or_stdin(path: Option<&Path>) -> Result<String, ExitCode> {
    let read_from_stdin = path.is_none() || path == Some(Path::new("-"));
    if read_from_stdin {
        let mut buf = String::new();
        if std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf).is_err() {
            eprintln!("error: failed to read diff from stdin");
            return Err(ExitCode::from(EXIT_USAGE_ERROR));
        }
        Ok(buf)
    } else {
        let path = path.expect("read_from_stdin is false only when path is Some");
        std::fs::read_to_string(path).map_err(|err| {
            eprintln!("error: failed to read diff file {}: {err}", path.display());
            ExitCode::from(EXIT_CONFIG_ERROR)
        })
    }
}

async fn cmd_slop(config: &Config, path: Option<&Path>, full: bool, scope: &[PathBuf], json: bool) -> ExitCode {
    let diff_text = match read_diff_or_stdin(path) {
        Ok(text) => text,
        Err(code) => return code,
    };

    let provider = if full {
        Some(OpenAiCompatibleClient::new(
            config.provider.base_url.clone(),
            provider_api_key(&config.provider),
            config.provider.model.clone(),
        ))
    } else {
        None
    };
    let provider_ref: Option<&dyn pro_provider::Provider> = provider.as_ref().map(|p| p as _);

    match pro_slop::score(provider_ref, &diff_text, scope).await {
        Ok(report) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());
            } else {
                for (axis, score) in &report.axis_scores {
                    println!("{axis:?}: {score:.2}");
                }
                for finding in &report.findings {
                    println!("- [{:?}/{:?}] {}", finding.axis, finding.category, finding.description);
                }
                println!("overall: {:.2}", report.overall);
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("slop scoring failed: {err}");
            ExitCode::from(EXIT_PROVIDER_ERROR)
        }
    }
}

async fn cmd_supply_chain_check(ecosystem: EcosystemArg, package: &str, json: bool) -> ExitCode {
    let result = match ecosystem {
        EcosystemArg::Npm => {
            let client = NpmRegistryClient::default();
            SupplyChainGate::npm(&client).check(package).await
        }
        EcosystemArg::Crates => {
            let client = CratesIoRegistryClient::default();
            SupplyChainGate::crates_io(&client).check(package).await
        }
    };

    match result {
        Ok(report) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());
            } else {
                println!("{} ({}): {:?}", report.package, report.ecosystem, report.verdict);
                for reason in &report.reasons {
                    println!("- {reason:?}");
                }
            }
            match report.verdict {
                pro_supply_chain::Verdict::Clear => ExitCode::SUCCESS,
                _ => ExitCode::from(EXIT_SUPPLY_CHAIN_REJECTED),
            }
        }
        Err(err) => {
            eprintln!("supply-chain check failed: {err}");
            ExitCode::from(EXIT_PROVIDER_ERROR)
        }
    }
}

fn cmd_repomap(project_root: &Path, budget: usize, json: bool) -> ExitCode {
    let map = pro_repomap::RepoMap::build(project_root);
    let rendered = map.render(budget);
    if json {
        println!("{}", serde_json::json!({ "render": rendered }));
    } else {
        print!("{rendered}");
    }
    ExitCode::SUCCESS
}

/// Spawns a real `rust-analyzer` for Rust files, root-uri'd at the file's
/// containing directory; returns `None` (never blocks/hangs) for any other
/// language or if the binary isn't on `PATH` — `pro-grounding` then falls
/// back to the AST/symbol-table tier on its own.
async fn spawn_lsp_for(path: &Path, root: &Path) -> Option<pro_lsp::LspClient> {
    if pro_repomap::Language::of_path(path) != Some(pro_repomap::Language::Rust) {
        return None;
    }
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let root_uri = format!("file://{}", canonical_root.display()).parse::<lsp_types::Uri>().ok()?;
    pro_lsp::LspClient::spawn("rust-analyzer", &[], root_uri).await.ok()
}

async fn cmd_ground(path: &Path, json: bool) -> ExitCode {
    let content = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("error: failed to read {}: {err}", path.display());
            return ExitCode::from(EXIT_CONFIG_ERROR);
        }
    };
    let root = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new(".")).to_path_buf();
    let repo_map = pro_repomap::RepoMap::build(&root);

    let lsp_client = spawn_lsp_for(path, &root).await;
    let lsp_arg = match &lsp_client {
        Some(client) => {
            let absolute = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            format!("file://{}", absolute.display()).parse::<lsp_types::Uri>().ok().map(|uri| (client, uri))
        }
        None => None,
    };

    let gate = pro_grounding::GroundingGate::new(&repo_map);
    let report = gate.check(path, &root, &content, lsp_arg).await;

    if json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());
    } else if report.is_clean() {
        println!("clean — no grounding findings");
    } else {
        for finding in &report.findings {
            println!("- [{:?}] {}: {}", finding.category, finding.symbol_or_path, finding.detail);
        }
    }
    if report.is_clean() { ExitCode::SUCCESS } else { ExitCode::from(EXIT_GROUNDING_FAILED) }
}

async fn cmd_review(config: &Config, diff_path: Option<&Path>, adversarial: bool, orthogonal: bool, json: bool) -> ExitCode {
    let diff_text = match read_diff_or_stdin(diff_path) {
        Ok(text) => text,
        Err(code) => return code,
    };

    let provider =
        OpenAiCompatibleClient::new(config.provider.base_url.clone(), provider_api_key(&config.provider), config.provider.model.clone());

    if orthogonal {
        let Some(cascade_cfg) = &config.cascade_provider else {
            eprintln!("error: --orthogonal requires [cascade_provider] in config");
            return ExitCode::from(EXIT_CONFIG_ERROR);
        };
        let secondary =
            OpenAiCompatibleClient::new(cascade_cfg.base_url.clone(), provider_api_key(cascade_cfg), cascade_cfg.model.clone());
        return match pro_review::orthogonal(&provider, &secondary, &diff_text).await {
            Ok(result) => {
                if json {
                    println!("{}", serde_json::to_string_pretty(&result).unwrap_or_default());
                } else {
                    println!("primary: {}", result.primary.verdict);
                    println!("secondary: {}", result.secondary.verdict);
                    if let Some(disagreement) = &result.disagreement {
                        println!("disagreement: {disagreement}");
                    }
                }
                if result.disagreement.is_some() { ExitCode::from(EXIT_REVIEW_DISAGREEMENT) } else { ExitCode::SUCCESS }
            }
            Err(err) => {
                eprintln!("orthogonal review failed: {err}");
                ExitCode::from(EXIT_PROVIDER_ERROR)
            }
        };
    }

    let result =
        if adversarial { pro_review::adversarial(&provider, &diff_text).await } else { pro_review::simple(&provider, &diff_text).await };
    match result {
        Ok(result) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&result).unwrap_or_default());
            } else {
                for step in &result.trail {
                    println!("{step}\n");
                }
                println!("verdict: {}", result.verdict);
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("review failed: {err}");
            ExitCode::from(EXIT_PROVIDER_ERROR)
        }
    }
}

struct CodegenProposer {
    config: Config,
    task: String,
    files: Vec<PathBuf>,
}

#[async_trait::async_trait]
impl pro_worktree::Proposer for CodegenProposer {
    async fn propose(&self, worktree: &pro_worktree::Worktree, attempt: usize) {
        let mut grants = GrantStore::new();
        grants.add_grant(Grant::new_shadow(pro_capability::Capability::FsRead { paths: vec![worktree.path.clone()] }));
        grants.add_grant(Grant::new_shadow(pro_capability::Capability::FsWrite { paths: vec![worktree.path.clone()] }));

        let audit = AuditLog::new(worktree.path.join(".pro/audit.jsonl"));
        let provider = OpenAiCompatibleClient::new(
            self.config.provider.base_url.clone(),
            provider_api_key(&self.config.provider),
            self.config.provider.model.clone(),
        );
        let engine = Engine::new(self.config.clone(), grants, audit, EnforcerRegistry::new(), Box::new(provider));

        let scoped_files: Vec<PathBuf> = self.files.iter().map(|f| worktree.path.join(f)).collect();
        match engine.propose_diff(&self.task, &scoped_files).await {
            Ok(change) => {
                for file in change.files {
                    if let Err(err) = engine.fs_write(&file.path, file.new_content.as_bytes()) {
                        eprintln!("attempt {attempt}: fs_write failed for {}: {err}", file.path.display());
                    }
                }
            }
            Err(err) => eprintln!("attempt {attempt}: propose_diff failed: {err}"),
        }
    }
}

async fn cmd_run(config: Config, repo_root: PathBuf, task: &str, parallel: usize, verify: &str, files: &[PathBuf], json: bool) -> ExitCode {
    let worktree_base = repo_root.parent().map(Path::to_path_buf).unwrap_or_else(|| repo_root.clone());
    let proposer = CodegenProposer { config, task: task.to_string(), files: files.to_vec() };

    match pro_worktree::run_best_of_n(&repo_root, &worktree_base, parallel, verify, &proposer).await {
        Ok(result) => {
            if json {
                let candidates: Vec<_> = result
                    .candidates
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "branch": c.worktree.branch,
                            "verify_passed": c.verify_passed,
                            "verify_output": c.verify_output,
                        })
                    })
                    .collect();
                println!("{}", serde_json::json!({ "candidates": candidates, "winner_index": result.winner_index }));
            } else {
                for (index, candidate) in result.candidates.iter().enumerate() {
                    let marker = if Some(index) == result.winner_index { "WINNER" } else { "torn down" };
                    println!("[{index}] {} — {marker}", candidate.worktree.branch);
                }
            }
            if result.winner_index.is_some() { ExitCode::SUCCESS } else { ExitCode::from(EXIT_NO_WINNER) }
        }
        Err(err) => {
            eprintln!("pro run failed: {err}");
            ExitCode::from(EXIT_PROVIDER_ERROR)
        }
    }
}

fn exit_code_for(err: &EngineError) -> ExitCode {
    let code = match err {
        EngineError::CapabilityDenied(_) => EXIT_CAPABILITY_DENIED,
        EngineError::Sandbox(_) => EXIT_SANDBOX_ERROR,
        EngineError::Provider(_) | EngineError::Net(_) => EXIT_PROVIDER_ERROR,
        EngineError::Io(_) => EXIT_CONFIG_ERROR,
        EngineError::SupplyChainRegistry(_) => EXIT_PROVIDER_ERROR,
        EngineError::SupplyChainRejected { .. } => EXIT_SUPPLY_CHAIN_REJECTED,
        EngineError::NoCascadeProvider => EXIT_CONFIG_ERROR,
        EngineError::Json(_) => EXIT_PROVIDER_ERROR,
    };
    ExitCode::from(code)
}
