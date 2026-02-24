//! Multi-round Bee Colony debate protocol.
//!
//! Implements Multi-Agent Debate (MAD) pattern over A2A protocol:
//!
//! ```text
//! Round 1: Queen → all Bees (independent research)
//!          Each Bee gets the topic + knowledge base context
//! Round 2: Queen collects → shares all outputs → Bees critique
//!          Each Bee sees everyone's R1 output + must critique/extend
//! Round N: Convergence check → consensus or vote
//! ```
//!
//! Based on ReConcile (ACL 2024) confidence-weighted voting.

use crate::a2a::types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ─── Debate Configuration ────────────────────────────────────

/// Configuration for a Bee Colony debate session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebateConfig {
    /// The research topic or question.
    pub topic: String,

    /// Number of Bee agents participating.
    pub num_bees: usize,

    /// Maximum debate rounds before forced conclusion.
    #[serde(default = "default_max_rounds")]
    pub max_rounds: usize,

    /// Confidence threshold for consensus (0.0 - 1.0).
    /// If all Bees agree with >= this confidence, debate ends.
    #[serde(default = "default_consensus_threshold")]
    pub consensus_threshold: f64,

    /// Optional knowledge base context to inject into Round 1.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub knowledge_context: Vec<String>,

    /// Bee endpoint URLs (A2A servers to send tasks to).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bee_endpoints: Vec<String>,
}

fn default_max_rounds() -> usize {
    3
}

fn default_consensus_threshold() -> f64 {
    0.8
}

// ─── Debate State ────────────────────────────────────────────

/// A single Bee's response in a debate round.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeeResponse {
    /// Which Bee produced this response.
    pub bee_id: String,

    /// The Bee's endpoint URL.
    pub endpoint: String,

    /// The text content of the response.
    pub content: String,

    /// Confidence score (0.0 - 1.0) — how sure is this Bee?
    #[serde(default)]
    pub confidence: f64,

    /// Position or stance on the topic.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<String>,

    /// Key points extracted from the response.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub key_points: Vec<String>,
}

/// A single round in the debate.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebateRound {
    /// Round number (1-indexed).
    pub round_number: usize,

    /// The prompt sent to all Bees in this round.
    pub prompt: String,

    /// Responses collected from all Bees.
    pub responses: Vec<BeeResponse>,

    /// Consensus analysis after this round.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consensus: Option<ConsensusAnalysis>,
}

/// Analysis of consensus after a debate round.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsensusAnalysis {
    /// Average confidence across all Bees.
    pub avg_confidence: f64,

    /// Points that all Bees agree on.
    pub agreement_points: Vec<String>,

    /// Points of contention between Bees.
    pub contention_points: Vec<String>,

    /// Blind spots — topics no Bee addressed.
    pub blind_spots: Vec<String>,

    /// Whether consensus was reached.
    pub consensus_reached: bool,
}

/// The full state of a debate session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebateSession {
    /// Unique session ID.
    pub id: String,

    /// Debate configuration.
    pub config: DebateConfig,

    /// Current round number.
    pub current_round: usize,

    /// All completed rounds.
    pub rounds: Vec<DebateRound>,

    /// Final synthesis (populated when debate concludes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_synthesis: Option<String>,

    /// Debate state.
    pub state: DebateState,
}

/// State of the debate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DebateState {
    /// Waiting to start.
    Pending,
    /// A round is in progress.
    InRound,
    /// Waiting for Queen to analyze round results.
    Analyzing,
    /// Debate concluded with consensus.
    Concluded,
    /// Debate ended without consensus (max rounds reached).
    Exhausted,
}

// ─── Debate Engine ───────────────────────────────────────────

impl DebateSession {
    /// Create a new debate session.
    pub fn new(config: DebateConfig) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            current_round: 0,
            rounds: Vec::new(),
            final_synthesis: None,
            state: DebateState::Pending,
            config,
        }
    }

    /// Generate the Round 1 prompt (independent research).
    pub fn round1_prompt(&self) -> String {
        let mut prompt = format!(
            "## Debate Topic\n\n{}\n\n\
             ## Your Task (Round 1 — Independent Research)\n\n\
             Analyze this topic from your unique perspective. Provide:\n\
             1. Your **position** on the topic\n\
             2. **Key arguments** supporting your position\n\
             3. **Evidence** or reasoning\n\
             4. **Confidence score** (0.0-1.0) in your position\n\
             5. **Potential counterarguments** you anticipate\n",
            self.config.topic
        );

        // Inject knowledge base context if available
        if !self.config.knowledge_context.is_empty() {
            prompt.push_str("\n## Knowledge Base Context\n\n");
            prompt.push_str(
                "The following verified knowledge has been loaded. \
                 Use it to inform your analysis, but think beyond it:\n\n",
            );
            for (i, ctx) in self.config.knowledge_context.iter().enumerate() {
                prompt.push_str(&format!("### Source {}\n{}\n\n", i + 1, ctx));
            }
        }

        prompt
    }

    /// Generate a critique round prompt (Round 2+).
    /// Each Bee sees all previous responses and must critique/extend.
    pub fn critique_prompt(&self, round_num: usize) -> String {
        let prev_round = &self.rounds[round_num - 2]; // 0-indexed

        let mut prompt = format!(
            "## Debate Topic\n\n{}\n\n\
             ## Round {} — Critique & Synthesis\n\n\
             You have seen all participants' responses from Round {}. \
             Your task:\n\
             1. **Identify agreements** — what do most participants agree on?\n\
             2. **Challenge weak arguments** — which positions lack evidence?\n\
             3. **Synthesize insights** — combine the strongest ideas\n\
             4. **Update your position** if others' arguments changed your mind\n\
             5. **Confidence score** (0.0-1.0) — has your confidence changed?\n\n\
             ## Previous Round Responses\n\n",
            self.config.topic,
            round_num,
            round_num - 1,
        );

        for resp in &prev_round.responses {
            prompt.push_str(&format!(
                "### Bee {} (confidence: {:.1})\n{}\n\n",
                resp.bee_id, resp.confidence, resp.content
            ));
        }

        prompt
    }

    /// Build A2A messages for a debate round.
    pub fn build_round_messages(&self, round_num: usize) -> Vec<(String, Message)> {
        let prompt = if round_num == 1 {
            self.round1_prompt()
        } else {
            self.critique_prompt(round_num)
        };

        self.config
            .bee_endpoints
            .iter()
            .enumerate()
            .map(|(i, endpoint)| {
                let msg = Message {
                    message_id: Some(Uuid::new_v4().to_string()),
                    context_id: Some(self.id.clone()),
                    task_id: None,
                    role: Role::User,
                    parts: vec![Part::text(&prompt)],
                    metadata: Some({
                        let mut m = HashMap::new();
                        m.insert(
                            "debate_round".to_string(),
                            serde_json::json!(round_num),
                        );
                        m.insert(
                            "bee_index".to_string(),
                            serde_json::json!(i),
                        );
                        m.insert(
                            "debate_session_id".to_string(),
                            serde_json::json!(self.id),
                        );
                        m
                    }),
                };
                (endpoint.clone(), msg)
            })
            .collect()
    }

    /// Analyze consensus from a round's responses.
    pub fn analyze_consensus(responses: &[BeeResponse], threshold: f64) -> ConsensusAnalysis {
        let avg_confidence = if responses.is_empty() {
            0.0
        } else {
            responses.iter().map(|r| r.confidence).sum::<f64>() / responses.len() as f64
        };

        // Simple position-based agreement detection
        let mut position_counts: HashMap<String, usize> = HashMap::new();
        for resp in responses {
            if let Some(ref pos) = resp.position {
                *position_counts.entry(pos.to_lowercase()).or_insert(0) += 1;
            }
        }

        let total = responses.len();
        let agreement_points: Vec<String> = position_counts
            .iter()
            .filter(|&(_, count)| *count as f64 / total as f64 >= threshold)
            .map(|(pos, count)| format!("{} ({}/{} agree)", pos, count, total))
            .collect();

        let contention_points: Vec<String> = position_counts
            .iter()
            .filter(|&(_, count)| {
                let ratio = *count as f64 / total as f64;
                ratio > 0.0 && ratio < threshold
            })
            .map(|(pos, count)| format!("{} ({}/{} agree)", pos, count, total))
            .collect();

        let consensus_reached =
            avg_confidence >= threshold && !agreement_points.is_empty();

        ConsensusAnalysis {
            avg_confidence,
            agreement_points,
            contention_points,
            blind_spots: vec![], // filled by LLM in production
            consensus_reached,
        }
    }

    /// Record a completed round.
    pub fn record_round(
        &mut self,
        round_number: usize,
        prompt: String,
        responses: Vec<BeeResponse>,
    ) {
        let consensus =
            Self::analyze_consensus(&responses, self.config.consensus_threshold);
        let concluded = consensus.consensus_reached
            || round_number >= self.config.max_rounds;

        self.rounds.push(DebateRound {
            round_number,
            prompt,
            responses,
            consensus: Some(consensus),
        });
        self.current_round = round_number;

        if concluded {
            self.state = if self.rounds.last()
                .and_then(|r| r.consensus.as_ref())
                .is_some_and(|c| c.consensus_reached)
            {
                DebateState::Concluded
            } else {
                DebateState::Exhausted
            };
        } else {
            self.state = DebateState::Analyzing;
        }
    }

    /// Generate a summary report of the debate.
    pub fn summary_report(&self) -> String {
        let mut report = format!(
            "# 🐝 Bee Colony Debate Report\n\n\
             **Topic:** {}\n\
             **Bees:** {}\n\
             **Rounds:** {}/{}\n\
             **State:** {:?}\n\n",
            self.config.topic,
            self.config.num_bees,
            self.current_round,
            self.config.max_rounds,
            self.state,
        );

        for round in &self.rounds {
            report.push_str(&format!(
                "## Round {}\n\n",
                round.round_number
            ));

            for resp in &round.responses {
                report.push_str(&format!(
                    "### Bee {} (confidence: {:.1})\n{}\n\n",
                    resp.bee_id, resp.confidence, resp.content
                ));
            }

            if let Some(ref consensus) = round.consensus {
                report.push_str(&format!(
                    "### Consensus Analysis\n\
                     - Avg Confidence: {:.2}\n\
                     - Consensus Reached: {}\n",
                    consensus.avg_confidence, consensus.consensus_reached,
                ));
                if !consensus.agreement_points.is_empty() {
                    report.push_str("- **Agreements:**\n");
                    for p in &consensus.agreement_points {
                        report.push_str(&format!("  - {}\n", p));
                    }
                }
                if !consensus.contention_points.is_empty() {
                    report.push_str("- **Contentions:**\n");
                    for p in &consensus.contention_points {
                        report.push_str(&format!("  - {}\n", p));
                    }
                }
                report.push('\n');
            }
        }

        if let Some(ref synthesis) = self.final_synthesis {
            report.push_str(&format!("## Final Synthesis\n\n{}\n", synthesis));
        }

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> DebateConfig {
        DebateConfig {
            topic: "Should AI agents have persistent memory across sessions?".to_string(),
            num_bees: 3,
            max_rounds: 3,
            consensus_threshold: 0.8,
            knowledge_context: vec![
                "Memory architectures: L0-L6 layered system with SQLite + FTS5".to_string(),
                "Security concern: memory injection attacks via prompt manipulation".to_string(),
            ],
            bee_endpoints: vec![
                "http://bee-1:18789/a2a/v1".to_string(),
                "http://bee-2:18789/a2a/v1".to_string(),
                "http://bee-3:18789/a2a/v1".to_string(),
            ],
        }
    }

    #[test]
    fn test_debate_session_creation() {
        let config = test_config();
        let session = DebateSession::new(config);
        assert_eq!(session.state, DebateState::Pending);
        assert_eq!(session.current_round, 0);
        assert!(session.rounds.is_empty());
    }

    #[test]
    fn test_round1_prompt_includes_knowledge() {
        let config = test_config();
        let session = DebateSession::new(config);
        let prompt = session.round1_prompt();

        assert!(prompt.contains("Should AI agents"));
        assert!(prompt.contains("Knowledge Base Context"));
        assert!(prompt.contains("L0-L6 layered system"));
        assert!(prompt.contains("memory injection attacks"));
        assert!(prompt.contains("Confidence score"));
    }

    #[test]
    fn test_build_round_messages() {
        let config = test_config();
        let session = DebateSession::new(config);
        let messages = session.build_round_messages(1);

        assert_eq!(messages.len(), 3);
        for (endpoint, msg) in &messages {
            assert!(endpoint.starts_with("http://bee-"));
            assert_eq!(msg.role, Role::User);
            assert!(!msg.parts.is_empty());
            assert!(msg.metadata.is_some());
        }
    }

    #[test]
    fn test_consensus_analysis_agreement() {
        let responses = vec![
            BeeResponse {
                bee_id: "bee-1".to_string(),
                endpoint: "http://bee-1:18789".to_string(),
                content: "Yes, persistent memory is essential.".to_string(),
                confidence: 0.9,
                position: Some("pro".to_string()),
                key_points: vec![],
            },
            BeeResponse {
                bee_id: "bee-2".to_string(),
                endpoint: "http://bee-2:18789".to_string(),
                content: "Strongly agree with persistent memory.".to_string(),
                confidence: 0.85,
                position: Some("pro".to_string()),
                key_points: vec![],
            },
            BeeResponse {
                bee_id: "bee-3".to_string(),
                endpoint: "http://bee-3:18789".to_string(),
                content: "Yes, but with security constraints.".to_string(),
                confidence: 0.8,
                position: Some("pro".to_string()),
                key_points: vec![],
            },
        ];

        let consensus = DebateSession::analyze_consensus(&responses, 0.8);
        assert!(consensus.consensus_reached);
        assert!(!consensus.agreement_points.is_empty());
        assert!(consensus.avg_confidence > 0.8);
    }

    #[test]
    fn test_consensus_analysis_contention() {
        let responses = vec![
            BeeResponse {
                bee_id: "bee-1".to_string(),
                endpoint: "http://bee-1:18789".to_string(),
                content: "Yes to memory.".to_string(),
                confidence: 0.9,
                position: Some("pro".to_string()),
                key_points: vec![],
            },
            BeeResponse {
                bee_id: "bee-2".to_string(),
                endpoint: "http://bee-2:18789".to_string(),
                content: "No, too risky.".to_string(),
                confidence: 0.7,
                position: Some("con".to_string()),
                key_points: vec![],
            },
        ];

        let consensus = DebateSession::analyze_consensus(&responses, 0.8);
        assert!(!consensus.consensus_reached);
        assert!(!consensus.contention_points.is_empty());
    }

    #[test]
    fn test_record_round_and_state_transition() {
        let config = test_config();
        let mut session = DebateSession::new(config);

        let responses = vec![
            BeeResponse {
                bee_id: "bee-1".to_string(),
                endpoint: "http://bee-1:18789".to_string(),
                content: "My analysis...".to_string(),
                confidence: 0.9,
                position: Some("pro".to_string()),
                key_points: vec![],
            },
        ];

        session.record_round(1, "Round 1 prompt".to_string(), responses);
        assert_eq!(session.current_round, 1);
        // With only 1 bee saying "pro", consensus should be reached
        assert_eq!(session.state, DebateState::Concluded);
    }

    #[test]
    fn test_summary_report() {
        let config = test_config();
        let mut session = DebateSession::new(config);

        let responses = vec![BeeResponse {
            bee_id: "bee-1".to_string(),
            endpoint: "http://bee-1:18789".to_string(),
            content: "Persistent memory is crucial for continuity.".to_string(),
            confidence: 0.85,
            position: Some("pro".to_string()),
            key_points: vec!["continuity".to_string()],
        }];

        session.record_round(1, "Topic prompt".to_string(), responses);
        let report = session.summary_report();

        assert!(report.contains("Bee Colony Debate Report"));
        assert!(report.contains("Should AI agents"));
        assert!(report.contains("Persistent memory is crucial"));
        assert!(report.contains("Consensus Analysis"));
    }

    #[test]
    fn test_critique_prompt_includes_previous_responses() {
        let config = test_config();
        let mut session = DebateSession::new(config);

        // Simulate Round 1
        let r1_responses = vec![
            BeeResponse {
                bee_id: "bee-1".to_string(),
                endpoint: "http://bee-1:18789".to_string(),
                content: "Memory helps with learning.".to_string(),
                confidence: 0.8,
                position: Some("pro".to_string()),
                key_points: vec![],
            },
            BeeResponse {
                bee_id: "bee-2".to_string(),
                endpoint: "http://bee-2:18789".to_string(),
                content: "Privacy risks are high.".to_string(),
                confidence: 0.6,
                position: Some("con".to_string()),
                key_points: vec![],
            },
        ];
        session.record_round(1, "Round 1".to_string(), r1_responses);
        session.state = DebateState::InRound; // Force to allow R2

        let critique = session.critique_prompt(2);
        assert!(critique.contains("Critique & Synthesis"));
        assert!(critique.contains("Memory helps with learning"));
        assert!(critique.contains("Privacy risks are high"));
        assert!(critique.contains("Bee bee-1"));
        assert!(critique.contains("Bee bee-2"));
    }
}
