//! Centralized model pricing table.
//!
//! Reads `~/.opencrabs/usage_pricing.toml` at runtime — no recompile needed.
//! Users can add/edit entries freely. Falls back to compiled-in defaults if
//! the file is missing or unparseable.

use once_cell::sync::OnceCell;
use serde::Deserialize;
use std::path::PathBuf;

// ── TOML schema ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Default)]
struct PricingFile {
    #[serde(default)]
    usage: UsageSection,
}

#[derive(Debug, Deserialize, Default)]
struct UsageSection {
    #[serde(default)]
    pricing: PricingSection,
}

#[derive(Debug, Deserialize, Default)]
struct PricingSection {
    #[serde(default)]
    anthropic: Vec<ModelPricing>,
    #[serde(default)]
    openai: Vec<ModelPricing>,
    #[serde(default)]
    minimax: Vec<ModelPricing>,
    #[serde(default)]
    other: Vec<ModelPricing>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ModelPricing {
    /// Substring to match against the model string (case-insensitive)
    pub prefix: String,
    /// Cost per 1M input tokens in USD
    pub input_per_m: f64,
    /// Cost per 1M output tokens in USD
    pub output_per_m: f64,
}

// ── Public API ────────────────────────────────────────────────────────────────

pub struct PricingTable {
    entries: Vec<ModelPricing>,
}

impl PricingTable {
    /// Calculate cost for a model given input/output token counts.
    /// Returns 0.0 if the model is not in the pricing table.
    pub fn calculate_cost(&self, model: &str, input_tokens: u32, output_tokens: u32) -> f64 {
        let m = model.to_lowercase();
        for entry in &self.entries {
            if m.contains(&entry.prefix.to_lowercase()) {
                let input_cost = (input_tokens as f64 / 1_000_000.0) * entry.input_per_m;
                let output_cost = (output_tokens as f64 / 1_000_000.0) * entry.output_per_m;
                return input_cost + output_cost;
            }
        }
        0.0
    }

    /// Estimate cost from a total token count using an 80/20 input/output split.
    /// Returns None if the model is unknown.
    pub fn estimate_cost(&self, model: &str, total_tokens: i64) -> Option<f64> {
        let m = model.to_lowercase();
        for entry in &self.entries {
            if m.contains(&entry.prefix.to_lowercase()) {
                let input_tokens = (total_tokens as f64 * 0.80) as u64;
                let output_tokens = (total_tokens as f64 * 0.20) as u64;
                let cost = (input_tokens as f64 / 1_000_000.0) * entry.input_per_m
                    + (output_tokens as f64 / 1_000_000.0) * entry.output_per_m;
                return Some(cost);
            }
        }
        None
    }

    /// Returns true if the model has a known pricing entry.
    pub fn is_known(&self, model: &str) -> bool {
        let m = model.to_lowercase();
        self.entries.iter().any(|e| m.contains(&e.prefix.to_lowercase()))
    }
}

// ── Global instance ───────────────────────────────────────────────────────────

static PRICING: OnceCell<PricingTable> = OnceCell::new();

/// Returns the global pricing table, loading from disk on first call.
pub fn pricing() -> &'static PricingTable {
    PRICING.get_or_init(load_pricing)
}

fn pricing_file_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".opencrabs")
        .join("usage_pricing.toml")
}

fn load_pricing() -> PricingTable {
    let path = pricing_file_path();
    let parsed = if path.exists() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| toml::from_str::<PricingFile>(&s).ok())
    } else {
        None
    };

    let section = parsed
        .unwrap_or_default()
        .usage
        .pricing;

    // Merge all provider sections into one flat list (order = priority)
    let mut entries = Vec::new();
    entries.extend(section.anthropic);
    entries.extend(section.openai);
    entries.extend(section.minimax);
    entries.extend(section.other);

    // If file was missing or empty, use compiled-in defaults
    if entries.is_empty() {
        entries = default_entries();
    }

    PricingTable { entries }
}

fn default_entries() -> Vec<ModelPricing> {
    vec![
        // ── Anthropic ────────────────────────────────────────────────────────
        ModelPricing { prefix: "claude-opus-4".into(),       input_per_m: 5.0,  output_per_m: 25.0  },
        ModelPricing { prefix: "claude-opus-3".into(),       input_per_m: 15.0, output_per_m: 75.0  },
        ModelPricing { prefix: "claude-3-opus".into(),       input_per_m: 15.0, output_per_m: 75.0  },
        ModelPricing { prefix: "claude-sonnet-4".into(),     input_per_m: 3.0,  output_per_m: 15.0  },
        ModelPricing { prefix: "claude-3-7-sonnet".into(),   input_per_m: 3.0,  output_per_m: 15.0  },
        ModelPricing { prefix: "claude-3-5-sonnet".into(),   input_per_m: 3.0,  output_per_m: 15.0  },
        ModelPricing { prefix: "claude-3-sonnet".into(),     input_per_m: 3.0,  output_per_m: 15.0  },
        ModelPricing { prefix: "claude-haiku-4".into(),      input_per_m: 1.0,  output_per_m: 5.0   },
        ModelPricing { prefix: "claude-3-5-haiku".into(),    input_per_m: 0.80, output_per_m: 4.0   },
        ModelPricing { prefix: "claude-3-haiku".into(),      input_per_m: 0.25, output_per_m: 1.25  },
        // ── OpenAI ───────────────────────────────────────────────────────────
        ModelPricing { prefix: "gpt-4o-mini".into(),         input_per_m: 0.15, output_per_m: 0.60  },
        ModelPricing { prefix: "gpt-4o".into(),              input_per_m: 2.50, output_per_m: 10.0  },
        ModelPricing { prefix: "gpt-4-turbo".into(),         input_per_m: 10.0, output_per_m: 30.0  },
        ModelPricing { prefix: "gpt-4".into(),               input_per_m: 30.0, output_per_m: 60.0  },
        ModelPricing { prefix: "gpt-3.5-turbo".into(),       input_per_m: 0.50, output_per_m: 1.50  },
        ModelPricing { prefix: "o1-mini".into(),             input_per_m: 1.10, output_per_m: 4.40  },
        ModelPricing { prefix: "o1".into(),                  input_per_m: 15.0, output_per_m: 60.0  },
        ModelPricing { prefix: "o3-mini".into(),             input_per_m: 1.10, output_per_m: 4.40  },
        ModelPricing { prefix: "o3".into(),                  input_per_m: 10.0, output_per_m: 40.0  },
        // ── MiniMax ──────────────────────────────────────────────────────────
        ModelPricing { prefix: "minimax-m2.5".into(),        input_per_m: 0.30, output_per_m: 1.20  },
        ModelPricing { prefix: "minimax-m2.1".into(),        input_per_m: 0.30, output_per_m: 1.20  },
        ModelPricing { prefix: "minimax-text-01".into(),     input_per_m: 0.20, output_per_m: 1.10  },
        ModelPricing { prefix: "minimax".into(),             input_per_m: 0.30, output_per_m: 1.20  },
        // ── Google ───────────────────────────────────────────────────────────
        ModelPricing { prefix: "gemini-2.0-flash".into(),    input_per_m: 0.10, output_per_m: 0.40  },
        ModelPricing { prefix: "gemini-1.5-pro".into(),      input_per_m: 1.25, output_per_m: 5.0   },
        ModelPricing { prefix: "gemini-1.5-flash".into(),    input_per_m: 0.075,output_per_m: 0.30  },
        // ── DeepSeek ─────────────────────────────────────────────────────────
        ModelPricing { prefix: "deepseek-r1".into(),         input_per_m: 0.55, output_per_m: 2.19  },
        ModelPricing { prefix: "deepseek-v3".into(),         input_per_m: 0.27, output_per_m: 1.10  },
        ModelPricing { prefix: "deepseek".into(),            input_per_m: 0.27, output_per_m: 1.10  },
    ]
}
