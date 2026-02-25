//! Centralized model pricing — loaded from ~/.opencrabs/usage_pricing.toml at runtime.
//!
//! Users can edit that file to add custom models or update rates without recompiling.
//! Falls back to built-in defaults if the file is missing or malformed.

use std::path::PathBuf;
use std::sync::OnceLock;

use serde::Deserialize;

/// A single pricing entry: matches models whose lowercased name contains `prefix`.
#[derive(Debug, Clone, Deserialize)]
pub struct PricingEntry {
    /// Substring to match against the lowercased model name (e.g. "claude-sonnet-4")
    pub prefix: String,
    /// Cost per 1 million input tokens (USD)
    pub input_per_m: f64,
    /// Cost per 1 million output tokens (USD)
    pub output_per_m: f64,
}

/// Top-level TOML structure
#[derive(Debug, Clone, Deserialize, Default)]
struct PricingFile {
    #[serde(default)]
    models: Vec<PricingEntry>,
}

/// The loaded pricing table, available globally after first access.
#[derive(Debug, Clone)]
pub struct PricingTable {
    entries: Vec<PricingEntry>,
}

impl PricingTable {
    /// Calculate cost for a model given input and output token counts.
    /// Matches the first entry whose prefix is contained in the lowercased model name.
    /// Returns 0.0 if no entry matches (unknown model).
    pub fn calculate_cost(&self, model: &str, input_tokens: u32, output_tokens: u32) -> f64 {
        let m = model.to_lowercase();
        for entry in &self.entries {
            if m.contains(entry.prefix.as_str()) {
                let input = (input_tokens as f64 / 1_000_000.0) * entry.input_per_m;
                let output = (output_tokens as f64 / 1_000_000.0) * entry.output_per_m;
                return input + output;
            }
        }
        0.0
    }

    /// Estimate cost from a total token count using an 80/20 input/output split.
    /// Returns None if model is unknown, Some((cost, is_estimate)) otherwise.
    pub fn estimate_cost(&self, model: &str, total_tokens: i64) -> Option<f64> {
        let m = model.to_lowercase();
        for entry in &self.entries {
            if m.contains(entry.prefix.as_str()) {
                let input = (total_tokens as f64 * 0.80 / 1_000_000.0) * entry.input_per_m;
                let output = (total_tokens as f64 * 0.20 / 1_000_000.0) * entry.output_per_m;
                return Some(input + output);
            }
        }
        None
    }

    /// Find pricing entry for a model (for display purposes).
    pub fn rates(&self, model: &str) -> Option<(f64, f64)> {
        let m = model.to_lowercase();
        for entry in &self.entries {
            if m.contains(entry.prefix.as_str()) {
                return Some((entry.input_per_m, entry.output_per_m));
            }
        }
        None
    }
}

static PRICING: OnceLock<PricingTable> = OnceLock::new();

/// Get the global pricing table, loading from disk on first call.
pub fn pricing() -> &'static PricingTable {
    PRICING.get_or_init(|| {
        let path = pricing_file_path();
        let table = load_from_file(&path).unwrap_or_else(|_| defaults());
        // Write defaults to disk if file doesn't exist yet
        if !path.exists() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&path, default_toml());
        }
        table
    })
}

fn pricing_file_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    PathBuf::from(home).join(".opencrabs").join("usage_pricing.toml")
}

fn load_from_file(path: &PathBuf) -> Result<PricingTable, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let file: PricingFile = toml::from_str(&content)?;
    Ok(PricingTable { entries: file.models })
}

fn defaults() -> PricingTable {
    PricingTable {
        entries: default_entries(),
    }
}

fn default_entries() -> Vec<PricingEntry> {
    vec![
        // ── Anthropic Claude 4 ──────────────────────────────────────────────
        PricingEntry { prefix: "claude-opus-4".into(),   input_per_m: 5.0,   output_per_m: 25.0  },
        PricingEntry { prefix: "claude-sonnet-4".into(), input_per_m: 3.0,   output_per_m: 15.0  },
        PricingEntry { prefix: "claude-haiku-4".into(),  input_per_m: 1.0,   output_per_m: 5.0   },
        // ── Anthropic Claude 3.x ────────────────────────────────────────────
        PricingEntry { prefix: "claude-3-opus".into(),         input_per_m: 15.0,  output_per_m: 75.0  },
        PricingEntry { prefix: "claude-3-7-sonnet".into(),     input_per_m: 3.0,   output_per_m: 15.0  },
        PricingEntry { prefix: "claude-3-5-sonnet".into(),     input_per_m: 3.0,   output_per_m: 15.0  },
        PricingEntry { prefix: "claude-3-sonnet".into(),       input_per_m: 3.0,   output_per_m: 15.0  },
        PricingEntry { prefix: "claude-3-5-haiku".into(),      input_per_m: 0.80,  output_per_m: 4.0   },
        PricingEntry { prefix: "claude-3-haiku".into(),        input_per_m: 0.25,  output_per_m: 1.25  },
        // ── OpenAI ──────────────────────────────────────────────────────────
        PricingEntry { prefix: "gpt-4o-mini".into(),    input_per_m: 0.15,  output_per_m: 0.60  },
        PricingEntry { prefix: "gpt-4o".into(),         input_per_m: 2.50,  output_per_m: 10.0  },
        PricingEntry { prefix: "gpt-4-turbo".into(),    input_per_m: 10.0,  output_per_m: 30.0  },
        PricingEntry { prefix: "gpt-4".into(),          input_per_m: 30.0,  output_per_m: 60.0  },
        PricingEntry { prefix: "gpt-3.5-turbo".into(),  input_per_m: 0.50,  output_per_m: 1.50  },
        PricingEntry { prefix: "o3-mini".into(),        input_per_m: 1.10,  output_per_m: 4.40  },
        PricingEntry { prefix: "o3".into(),             input_per_m: 10.0,  output_per_m: 40.0  },
        PricingEntry { prefix: "o1-mini".into(),        input_per_m: 1.10,  output_per_m: 4.40  },
        PricingEntry { prefix: "o1".into(),             input_per_m: 15.0,  output_per_m: 60.0  },
        // ── Google Gemini ────────────────────────────────────────────────────
        PricingEntry { prefix: "gemini-2.0-flash".into(),  input_per_m: 0.10,  output_per_m: 0.40  },
        PricingEntry { prefix: "gemini-2.0-pro".into(),    input_per_m: 1.25,  output_per_m: 5.0   },
        PricingEntry { prefix: "gemini-1.5-flash".into(),  input_per_m: 0.075, output_per_m: 0.30  },
        PricingEntry { prefix: "gemini-1.5-pro".into(),    input_per_m: 1.25,  output_per_m: 5.0   },
        // ── MiniMax ─────────────────────────────────────────────────────────
        PricingEntry { prefix: "minimax-m2.5".into(),   input_per_m: 0.30,  output_per_m: 1.20  },
        PricingEntry { prefix: "minimax-m2.1".into(),   input_per_m: 0.30,  output_per_m: 1.20  },
        PricingEntry { prefix: "minimax-text-01".into(),input_per_m: 0.20,  output_per_m: 1.10  },
        PricingEntry { prefix: "minimax".into(),        input_per_m: 0.30,  output_per_m: 1.20  },
        // ── Meta Llama (via OpenRouter) ──────────────────────────────────────
        PricingEntry { prefix: "llama-3.3".into(),      input_per_m: 0.20,  output_per_m: 0.20  },
        PricingEntry { prefix: "llama-3.1-405b".into(), input_per_m: 2.70,  output_per_m: 2.70  },
        PricingEntry { prefix: "llama-3.1-70b".into(),  input_per_m: 0.35,  output_per_m: 0.40  },
        PricingEntry { prefix: "llama-3.1-8b".into(),   input_per_m: 0.05,  output_per_m: 0.07  },
        // ── DeepSeek ────────────────────────────────────────────────────────
        PricingEntry { prefix: "deepseek-r1".into(),    input_per_m: 0.55,  output_per_m: 2.19  },
        PricingEntry { prefix: "deepseek-v3".into(),    input_per_m: 0.27,  output_per_m: 1.10  },
        PricingEntry { prefix: "deepseek".into(),       input_per_m: 0.27,  output_per_m: 1.10  },
        // ── Mistral ─────────────────────────────────────────────────────────
        PricingEntry { prefix: "mistral-large".into(),  input_per_m: 2.0,   output_per_m: 6.0   },
        PricingEntry { prefix: "mistral-small".into(),  input_per_m: 0.10,  output_per_m: 0.30  },
        PricingEntry { prefix: "mixtral".into(),        input_per_m: 0.24,  output_per_m: 0.24  },
    ]
}

/// The default TOML content written to disk on first run.
pub fn default_toml() -> &'static str {
    r#"# OpenCrabs Usage Pricing
# Edit this file to add custom models or update rates — no restart needed.
# Matching: first entry whose 'prefix' is found in the lowercased model name wins.
# Rates are in USD per 1 million tokens.

# ── Anthropic Claude 4 ───────────────────────────────────────────────────────
[[models]]
prefix = "claude-opus-4"
input_per_m = 5.0
output_per_m = 25.0

[[models]]
prefix = "claude-sonnet-4"
input_per_m = 3.0
output_per_m = 15.0

[[models]]
prefix = "claude-haiku-4"
input_per_m = 1.0
output_per_m = 5.0

# ── Anthropic Claude 3.x ─────────────────────────────────────────────────────
[[models]]
prefix = "claude-3-opus"
input_per_m = 15.0
output_per_m = 75.0

[[models]]
prefix = "claude-3-7-sonnet"
input_per_m = 3.0
output_per_m = 15.0

[[models]]
prefix = "claude-3-5-sonnet"
input_per_m = 3.0
output_per_m = 15.0

[[models]]
prefix = "claude-3-sonnet"
input_per_m = 3.0
output_per_m = 15.0

[[models]]
prefix = "claude-3-5-haiku"
input_per_m = 0.80
output_per_m = 4.0

[[models]]
prefix = "claude-3-haiku"
input_per_m = 0.25
output_per_m = 1.25

# ── OpenAI ───────────────────────────────────────────────────────────────────
[[models]]
prefix = "gpt-4o-mini"
input_per_m = 0.15
output_per_m = 0.60

[[models]]
prefix = "gpt-4o"
input_per_m = 2.50
output_per_m = 10.0

[[models]]
prefix = "gpt-4-turbo"
input_per_m = 10.0
output_per_m = 30.0

[[models]]
prefix = "gpt-4"
input_per_m = 30.0
output_per_m = 60.0

[[models]]
prefix = "gpt-3.5-turbo"
input_per_m = 0.50
output_per_m = 1.50

[[models]]
prefix = "o3-mini"
input_per_m = 1.10
output_per_m = 4.40

[[models]]
prefix = "o3"
input_per_m = 10.0
output_per_m = 40.0

[[models]]
prefix = "o1-mini"
input_per_m = 1.10
output_per_m = 4.40

[[models]]
prefix = "o1"
input_per_m = 15.0
output_per_m = 60.0

# ── Google Gemini ─────────────────────────────────────────────────────────────
[[models]]
prefix = "gemini-2.0-flash"
input_per_m = 0.10
output_per_m = 0.40

[[models]]
prefix = "gemini-2.0-pro"
input_per_m = 1.25
output_per_m = 5.0

[[models]]
prefix = "gemini-1.5-flash"
input_per_m = 0.075
output_per_m = 0.30

[[models]]
prefix = "gemini-1.5-pro"
input_per_m = 1.25
output_per_m = 5.0

# ── MiniMax ───────────────────────────────────────────────────────────────────
[[models]]
prefix = "minimax-m2.5"
input_per_m = 0.30
output_per_m = 1.20

[[models]]
prefix = "minimax-m2.1"
input_per_m = 0.30
output_per_m = 1.20

[[models]]
prefix = "minimax-text-01"
input_per_m = 0.20
output_per_m = 1.10

[[models]]
prefix = "minimax"
input_per_m = 0.30
output_per_m = 1.20

# ── Meta Llama ────────────────────────────────────────────────────────────────
[[models]]
prefix = "llama-3.3"
input_per_m = 0.20
output_per_m = 0.20

[[models]]
prefix = "llama-3.1-405b"
input_per_m = 2.70
output_per_m = 2.70

[[models]]
prefix = "llama-3.1-70b"
input_per_m = 0.35
output_per_m = 0.40

[[models]]
prefix = "llama-3.1-8b"
input_per_m = 0.05
output_per_m = 0.07

# ── DeepSeek ─────────────────────────────────────────────────────────────────
[[models]]
prefix = "deepseek-r1"
input_per_m = 0.55
output_per_m = 2.19

[[models]]
prefix = "deepseek-v3"
input_per_m = 0.27
output_per_m = 1.10

[[models]]
prefix = "deepseek"
input_per_m = 0.27
output_per_m = 1.10

# ── Mistral ───────────────────────────────────────────────────────────────────
[[models]]
prefix = "mistral-large"
input_per_m = 2.0
output_per_m = 6.0

[[models]]
prefix = "mistral-small"
input_per_m = 0.10
output_per_m = 0.30

[[models]]
prefix = "mixtral"
input_per_m = 0.24
output_per_m = 0.24

# ── Add your custom models below ──────────────────────────────────────────────
# [[models]]
# prefix = "my-custom-model"
# input_per_m = 1.0
# output_per_m = 3.0
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_sonnet_4_cost() {
        let table = defaults();
        let cost = table.calculate_cost("claude-sonnet-4-6", 1_000_000, 1_000_000);
        assert_eq!(cost, 18.0); // $3 + $15
    }

    #[test]
    fn test_claude_opus_4_cost() {
        let table = defaults();
        let cost = table.calculate_cost("claude-opus-4-6", 1_000_000, 1_000_000);
        assert_eq!(cost, 30.0); // $5 + $25
    }

    #[test]
    fn test_minimax_cost() {
        let table = defaults();
        let cost = table.calculate_cost("MiniMax-M2.5", 1_000_000, 1_000_000);
        assert_eq!(cost, 1.50); // $0.30 + $1.20
    }

    #[test]
    fn test_unknown_model_returns_zero() {
        let table = defaults();
        let cost = table.calculate_cost("some-unknown-model-xyz", 1_000_000, 1_000_000);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn test_estimate_cost_80_20_split() {
        let table = defaults();
        // 1M tokens, 80% input (800K) @ $3 + 20% output (200K) @ $15 = $2.4 + $3.0 = $5.4
        let cost = table.estimate_cost("claude-sonnet-4-6", 1_000_000).unwrap();
        assert!((cost - 5.4).abs() < 0.001);
    }

    #[test]
    fn test_prefix_ordering_gpt4o_mini_before_gpt4o() {
        let table = defaults();
        // gpt-4o-mini must not match as gpt-4o — prefix "gpt-4o-mini" comes first
        let mini_cost = table.calculate_cost("gpt-4o-mini", 1_000_000, 1_000_000);
        let full_cost = table.calculate_cost("gpt-4o", 1_000_000, 1_000_000);
        assert!(mini_cost < full_cost);
    }
}
