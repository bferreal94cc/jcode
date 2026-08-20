//! `/chronicle tips` — personalised tips derived from the user's session history.
//!
//! Scans recent sessions, tallies which slash-commands and features appear in
//! user messages, then recommends the most relevant under-used capabilities.

use super::{App, DisplayMessage};

/// Maximum number of sessions to scan for usage patterns.
const SCAN_LIMIT: usize = 50;

/// Number of personalised tips to surface.
const MAX_TIPS: usize = 5;

// ── Feature descriptors ─────────────────────────────────────────────────────

/// A feature that can be suggested as a tip.
struct Feature {
    /// Slash-command prefix(es) that indicate use of this feature (lowercase).
    triggers: &'static [&'static str],
    /// One-line tip shown when the feature appears unused.
    tip: &'static str,
    /// Priority: lower numbers surface first (higher priority).
    priority: u8,
}

const FEATURES: &[Feature] = &[
    Feature {
        triggers: &["/improve"],
        tip: "/improve — autonomously improves the codebase in a supervised loop (try: /improve fix all lint warnings)",
        priority: 1,
    },
    Feature {
        triggers: &["/review"],
        tip: "/review — launches a one-shot AI code review of your current changes",
        priority: 2,
    },
    Feature {
        triggers: &["/plan"],
        tip: "/plan — asks the model to produce a plan card without writing any code yet",
        priority: 3,
    },
    Feature {
        triggers: &["/fork", "/split"],
        tip: "/fork — opens a parallel conversation branch so you can explore alternatives without losing your current context",
        priority: 4,
    },
    Feature {
        triggers: &["/poke"],
        tip: "/poke — nudges the model to continue when todos are incomplete; use /poke on to enable auto-poke",
        priority: 5,
    },
    Feature {
        triggers: &["/btw"],
        tip: "/btw — asks a quick side question in the side panel without interrupting the main conversation",
        priority: 6,
    },
    Feature {
        triggers: &["/swarm"],
        tip: "/swarm — enables swarm mode: multiple sessions coordinate plans, share context, and track file conflicts automatically",
        priority: 7,
    },
    Feature {
        triggers: &["/memory"],
        tip: "/memory — persists important facts across sessions; the model injects relevant memories automatically at each turn",
        priority: 8,
    },
    Feature {
        triggers: &["/commit", "/commit-push"],
        tip: "/commit — makes logical, well-described commits from the current diff without you having to craft the message",
        priority: 9,
    },
    Feature {
        triggers: &["/refactor"],
        tip: "/refactor — runs a safe, incremental refactor loop with automatic revert on test failures",
        priority: 10,
    },
    Feature {
        triggers: &["/compact"],
        tip: "/compact — compresses long conversation history to free up context while preserving key facts",
        priority: 11,
    },
    Feature {
        triggers: &["/observe"],
        tip: "/observe — pins the latest tool output in the side panel so you can read it while the conversation continues",
        priority: 12,
    },
    Feature {
        triggers: &["/todos", "/todo"],
        tip: "/todos — surfaces the AI-tracked todo list as a card in the chat; great for long multi-step tasks",
        priority: 13,
    },
    Feature {
        triggers: &["/overnight"],
        tip: "/overnight — runs an extended supervised coordinator that works while you are away and emails a summary",
        priority: 14,
    },
    Feature {
        triggers: &["/test"],
        tip: "/test — verifies a claim or validates current changes with a layered test sweep",
        priority: 15,
    },
];

// ── Usage analysis ───────────────────────────────────────────────────────────

/// Counts how many sessions contain at least one user message starting with
/// each trigger string.
fn count_feature_use(sessions: &[crate::tui::session_picker::SessionInfo]) -> Vec<usize> {
    let mut counts = vec![0usize; FEATURES.len()];
    for session in sessions {
        for msg in &session.messages_preview {
            if msg.role != "user" {
                continue;
            }
            let lower = msg.content.to_lowercase();
            for (i, feature) in FEATURES.iter().enumerate() {
                if counts[i] == 0
                    && feature
                        .triggers
                        .iter()
                        .any(|&t| lower.starts_with(t) || lower.contains(&format!("\n{t}")))
                {
                    counts[i] += 1;
                }
            }
        }
        // Also scan `first_user_prompt` which may not be included in preview.
        if let Some(ref prompt) = session.first_user_prompt {
            let lower = prompt.to_lowercase();
            for (i, feature) in FEATURES.iter().enumerate() {
                if counts[i] == 0
                    && feature
                        .triggers
                        .iter()
                        .any(|&t| lower.starts_with(t))
                {
                    counts[i] += 1;
                }
            }
        }
    }
    counts
}

/// Summarise high-level session patterns from the loaded list.
struct SessionSummary {
    total_sessions: usize,
    avg_user_messages: f64,
}

fn summarise(sessions: &[crate::tui::session_picker::SessionInfo]) -> SessionSummary {
    let total = sessions.len();
    let avg = if total == 0 {
        0.0
    } else {
        sessions.iter().map(|s| s.user_message_count as f64).sum::<f64>() / total as f64
    };
    SessionSummary {
        total_sessions: total,
        avg_user_messages: avg,
    }
}

// ── Formatting ───────────────────────────────────────────────────────────────

fn format_tips_output(
    summary: &SessionSummary,
    tips: &[&Feature],
    scanned: usize,
) -> String {
    let mut out = String::new();

    out.push_str("## 📖 Chronicle Tips\n\n");

    if scanned == 0 {
        out.push_str("No session history found yet — come back after a few sessions for personalised recommendations.\n\n");
        out.push_str("**General tip:** type `/help` to see all available slash-commands.\n");
        return out;
    }

    out.push_str(&format!(
        "Analysed **{}** recent session{} (avg **{:.0}** user message{} each).\n\n",
        scanned,
        if scanned == 1 { "" } else { "s" },
        summary.avg_user_messages,
        if summary.avg_user_messages == 1.0 { "" } else { "s" },
    ));

    if tips.is_empty() {
        out.push_str("You're already using all the major features — great work! 🎉\n\n");
        out.push_str("Check `/help` for a full command reference or `/hotkeys` to see your keyboard shortcut usage.\n");
        return out;
    }

    let sessions_noun = if summary.total_sessions == 1 { "session" } else { "sessions" };
    out.push_str(&format!(
        "Based on your {} {sessions_noun}, here are features you haven't tried yet:\n\n",
        summary.total_sessions,
    ));

    for (n, feature) in tips.iter().enumerate() {
        out.push_str(&format!("{}. {}\n", n + 1, feature.tip));
    }

    out.push('\n');
    out.push_str("*Run `/help <command>` for detailed usage of any command above.*\n");
    out
}

// ── Public entry point ───────────────────────────────────────────────────────

/// Handle `/chronicle` and `/chronicle tips`.
///
/// Returns `true` if the input was consumed, `false` if it doesn't match.
pub(super) fn handle_chronicle_command(app: &mut App, trimmed: &str) -> bool {
    if trimmed != "/chronicle" && trimmed != "/chronicle tips" {
        return false;
    }

    // Load sessions; surface an error if loading fails.
    let sessions = match crate::tui::session_picker::load_sessions() {
        Ok(s) => s,
        Err(err) => {
            app.push_display_message(DisplayMessage::system(format!(
                "/chronicle tips: failed to load session history — {err}"
            )));
            return true;
        }
    };

    let scanned = sessions.len().min(SCAN_LIMIT);
    let sessions = &sessions[..scanned];

    let summary = summarise(sessions);
    let counts = count_feature_use(sessions);

    // Collect features the user hasn't used yet, in priority order.
    let mut unused: Vec<(u8, &Feature)> = FEATURES
        .iter()
        .enumerate()
        .filter(|(i, _)| counts[*i] == 0)
        .map(|(_, f)| (f.priority, f))
        .collect();

    unused.sort_by_key(|(p, _)| *p);
    let tips: Vec<&Feature> = unused.iter().take(MAX_TIPS).map(|(_, f)| *f).collect();

    let output = format_tips_output(&summary, &tips, scanned);
    app.push_display_message(DisplayMessage::system(output));
    true
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::session_picker::SessionInfo;
    use chrono::Utc;
    use jcode_tui_session_picker::PreviewMessage;
    use jcode_tui_session_picker::{ResumeTarget, SessionSource};
    use jcode_session_types::SessionStatus;

    fn make_session(user_messages: &[&str]) -> SessionInfo {
        let preview: Vec<PreviewMessage> = user_messages
            .iter()
            .map(|m| PreviewMessage {
                role: "user".to_string(),
                content: m.to_string(),
                tool_calls: vec![],
                tool_data: None,
                timestamp: None,
            })
            .collect();
        SessionInfo {
            id: "test-id".to_string(),
            parent_id: None,
            short_name: "test".to_string(),
            icon: "🗒".to_string(),
            title: "Test session".to_string(),
            message_count: user_messages.len(),
            user_message_count: user_messages.len(),
            assistant_message_count: 0,
            created_at: Utc::now(),
            last_message_time: Utc::now(),
            last_active_at: None,
            working_dir: None,
            model: None,
            provider_key: None,
            is_canary: false,
            is_debug: false,
            saved: false,
            save_label: None,
            status: SessionStatus::Active,
            needs_catchup: false,
            estimated_tokens: 0,
            first_user_prompt: user_messages.first().map(|s| s.to_string()),
            messages_preview: preview,
            search_index: String::new(),
            server_name: None,
            server_icon: None,
            source: SessionSource::Jcode,
            resume_target: ResumeTarget::JcodeSession { session_id: "test-id".to_string() },
            external_path: None,
        }
    }

    #[test]
    fn no_sessions_returns_no_usage() {
        let counts = count_feature_use(&[]);
        assert!(counts.iter().all(|&c| c == 0));
    }

    #[test]
    fn detects_improve_command_usage() {
        let session = make_session(&["/improve fix all warnings"]);
        let counts = count_feature_use(&[session]);
        let improve_idx = FEATURES.iter().position(|f| f.triggers.contains(&"/improve")).unwrap();
        assert_eq!(counts[improve_idx], 1);
    }

    #[test]
    fn does_not_detect_unrelated_messages() {
        let session = make_session(&["Can you help me refactor this function?"]);
        let counts = count_feature_use(&[session]);
        // "/refactor" requires the message to start with the trigger
        let refactor_idx = FEATURES.iter().position(|f| f.triggers.contains(&"/refactor")).unwrap();
        assert_eq!(counts[refactor_idx], 0);
    }

    #[test]
    fn format_tips_output_no_sessions() {
        let summary = SessionSummary { total_sessions: 0, avg_user_messages: 0.0 };
        let output = format_tips_output(&summary, &[], 0);
        assert!(output.contains("No session history found"));
    }

    #[test]
    fn format_tips_output_all_features_used() {
        let summary = SessionSummary { total_sessions: 10, avg_user_messages: 3.0 };
        let output = format_tips_output(&summary, &[], 10);
        assert!(output.contains("already using all"));
    }

    #[test]
    fn format_tips_output_shows_unused_features() {
        let summary = SessionSummary { total_sessions: 5, avg_user_messages: 2.0 };
        let tips: Vec<&Feature> = FEATURES.iter().take(2).collect();
        let output = format_tips_output(&summary, &tips, 5);
        assert!(output.contains("1."));
        assert!(output.contains("2."));
    }
}
