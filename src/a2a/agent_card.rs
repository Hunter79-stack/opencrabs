//! Agent Card generation for `.well-known/agent.json`.
//!
//! Builds an `AgentCard` from the running OpenCrabs configuration,
//! exposing available skills and capabilities to other A2A agents.

use crate::a2a::types::*;

/// Build the Agent Card for this OpenCrabs instance.
pub fn build_agent_card(host: &str, port: u16) -> AgentCard {
    let base_url = format!("http://{}:{}", host, port);

    AgentCard {
        name: format!("OpenCrabs Bee (v{})", crate::VERSION),
        description: Some(
            "High-performance AI orchestration agent with A2A protocol support. \
             Part of the Bee Colony multi-agent system."
                .to_string(),
        ),
        version: Some(crate::VERSION.to_string()),
        documentation_url: Some("https://github.com/adolfousier/opencrabs".to_string()),
        icon_url: None,
        supported_interfaces: vec![SupportedInterface {
            url: format!("{}/a2a/v1", base_url),
            protocol_binding: "JSONRPC".to_string(),
            protocol_version: Some("1.0".to_string()),
        }],
        provider: Some(AgentProvider {
            organization: "OpenCrabs Contributors".to_string(),
            url: Some("https://github.com/adolfousier/opencrabs".to_string()),
        }),
        capabilities: Some(AgentCapabilities {
            streaming: false, // MVP: no streaming yet
            push_notifications: false,
            state_transition_history: true,
        }),
        skills: vec![
            AgentSkill {
                id: "code-analysis".to_string(),
                name: "Code Analysis & Refactoring".to_string(),
                description: Some(
                    "Analyze source code, identify issues, and suggest improvements."
                        .to_string(),
                ),
                tags: vec![
                    "code".to_string(),
                    "analysis".to_string(),
                    "refactoring".to_string(),
                ],
                examples: vec!["Analyze this Rust module for performance issues.".to_string()],
                input_modes: vec!["text/plain".to_string(), "application/json".to_string()],
                output_modes: vec!["text/plain".to_string(), "application/json".to_string()],
            },
            AgentSkill {
                id: "research".to_string(),
                name: "Deep Research".to_string(),
                description: Some(
                    "Perform multi-source research, cross-domain analysis, and synthesis."
                        .to_string(),
                ),
                tags: vec![
                    "research".to_string(),
                    "analysis".to_string(),
                    "synthesis".to_string(),
                ],
                examples: vec![
                    "Research the latest developments in AI agent security.".to_string(),
                ],
                input_modes: vec!["text/plain".to_string()],
                output_modes: vec!["text/plain".to_string(), "application/json".to_string()],
            },
            AgentSkill {
                id: "debate".to_string(),
                name: "Multi-Agent Debate".to_string(),
                description: Some(
                    "Participate in structured multi-round debates with other A2A agents."
                        .to_string(),
                ),
                tags: vec![
                    "debate".to_string(),
                    "council".to_string(),
                    "multi-agent".to_string(),
                ],
                examples: vec![
                    "Debate the pros and cons of microservices vs monoliths.".to_string(),
                ],
                input_modes: vec!["text/plain".to_string(), "application/json".to_string()],
                output_modes: vec!["text/plain".to_string(), "application/json".to_string()],
            },
        ],
        default_input_modes: vec!["text/plain".to_string(), "application/json".to_string()],
        default_output_modes: vec!["text/plain".to_string(), "application/json".to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_agent_card() {
        let card = build_agent_card("127.0.0.1", 18789);
        assert!(card.name.contains("OpenCrabs"));
        assert_eq!(card.skills.len(), 3);
        assert_eq!(
            card.supported_interfaces[0].url,
            "http://127.0.0.1:18789/a2a/v1"
        );
        assert_eq!(
            card.provider.as_ref().expect("provider").organization,
            "OpenCrabs Contributors"
        );
    }
}
