use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use pro_audit::AuditLog;
use pro_capability::{profiles, Grant, GrantStore};
use pro_config::{Config, ProfileName};
use pro_core::{Engine, EngineError};
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

async fn cmd_slop(config: &Config, path: Option<&Path>, full: bool, scope: &[PathBuf], json: bool) -> ExitCode {
    let read_from_stdin = path.is_none() || path == Some(Path::new("-"));
    let diff_text = if read_from_stdin {
        let mut buf = String::new();
        if std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf).is_err() {
            eprintln!("error: failed to read diff from stdin");
            return ExitCode::from(EXIT_USAGE_ERROR);
        }
        buf
    } else {
        let path = path.expect("read_from_stdin is false only when path is Some");
        match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) => {
                eprintln!("error: failed to read diff file {}: {err}", path.display());
                return ExitCode::from(EXIT_CONFIG_ERROR);
            }
        }
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

fn exit_code_for(err: &EngineError) -> ExitCode {
    let code = match err {
        EngineError::CapabilityDenied(_) => EXIT_CAPABILITY_DENIED,
        EngineError::Sandbox(_) => EXIT_SANDBOX_ERROR,
        EngineError::Provider(_) | EngineError::Net(_) => EXIT_PROVIDER_ERROR,
        EngineError::Io(_) => EXIT_CONFIG_ERROR,
        EngineError::SupplyChainRegistry(_) => EXIT_PROVIDER_ERROR,
        EngineError::SupplyChainRejected { .. } => EXIT_SUPPLY_CHAIN_REJECTED,
        EngineError::NoCascadeProvider => EXIT_CONFIG_ERROR,
    };
    ExitCode::from(code)
}
