use kodegen_mcp_schema::filesystem::*;
use kodegen_mcp_schema::introspection::{INSPECT_TOOL_CALLS, INSPECT_USAGE_STATS};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Update event for background processor
enum StatsUpdate {
    Success(String), // tool_name
    Failure(String), // tool_name
}

// Session timeout: 30 minutes of inactivity = new session
const SESSION_TIMEOUT_SECS: i64 = 30 * 60;

/// Statistics tracked for tool usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageStats {
    // Tool category counters
    pub filesystem_operations: u64,
    pub terminal_operations: u64,
    pub edit_operations: u64,
    pub search_operations: u64,
    pub config_operations: u64,
    pub process_operations: u64,

    // Overall counters
    pub total_tool_calls: u64,
    pub successful_calls: u64,
    pub failed_calls: u64,

    // Tool-specific counters
    pub tool_counts: HashMap<String, u64>,

    // Timing information
    pub first_used: i64, // Unix timestamp
    pub last_used: i64,  // Unix timestamp
    pub total_sessions: u64,
}

impl Default for UsageStats {
    fn default() -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            filesystem_operations: 0,
            terminal_operations: 0,
            edit_operations: 0,
            search_operations: 0,
            config_operations: 0,
            process_operations: 0,
            total_tool_calls: 0,
            successful_calls: 0,
            failed_calls: 0,
            tool_counts: HashMap::new(),
            first_used: now,
            last_used: now,
            total_sessions: 1,
        }
    }
}

/// Usage tracker that manages statistics for all tool calls
#[derive(Clone)]
pub struct UsageTracker {
    stats: Arc<RwLock<UsageStats>>,
    stats_file: PathBuf,
    session_start: std::time::Instant,
    /// Fire-and-forget channel for stat updates
    update_sender: tokio::sync::mpsc::UnboundedSender<StatsUpdate>,
}

impl UsageTracker {
    /// Create new `UsageTracker` with instance-specific stats file in ~/.kodegen/stats_{`instance_id}.json`
    #[must_use]
    pub fn new(instance_id: String) -> Self {
        let stats_file = Self::get_stats_file_path(&instance_id);
        let stats = UsageStats::default(); // Load async in background task

        // Create unbounded channel for fire-and-forget updates
        let (update_sender, update_receiver) = tokio::sync::mpsc::unbounded_channel();

        let tracker = Self {
            stats: Arc::new(RwLock::new(stats)),
            stats_file: stats_file.clone(),
            session_start: std::time::Instant::now(),
            update_sender,
        };

        // Start background processor
        tracker.start_background_processor(update_receiver);

        tracker
    }

    /// Get stats file path using kodegen_config (directory creation happens async)
    fn get_stats_file_path(instance_id: &str) -> PathBuf {
        kodegen_config::KodegenConfig::data_dir()
            .map(|dir| dir.join("stats").join(format!("stats_{instance_id}.json")))
            .unwrap_or_else(|_| PathBuf::from(format!("stats_{instance_id}.json")))
    }

    /// Load stats from disk or create default (async)
    async fn load_or_default(path: &PathBuf) -> UsageStats {
        match tokio::fs::read_to_string(path).await {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(_) => UsageStats::default(),
        }
    }

    /// Check if this is a new session (30+ min since last activity)
    fn is_new_session(last_used: i64) -> bool {
        let now = chrono::Utc::now().timestamp();
        (now - last_used) > SESSION_TIMEOUT_SECS
    }

    /// Get tool category for categorization
    fn get_category(tool_name: &str) -> Option<&'static str> {
        match tool_name {
            FS_READ_FILE
            | FS_READ_MULTIPLE_FILES
            | FS_WRITE_FILE
            | FS_CREATE_DIRECTORY
            | FS_LIST_DIRECTORY
            | FS_MOVE_FILE
            | FS_DELETE_FILE
            | FS_DELETE_DIRECTORY
            | FS_GET_FILE_INFO => Some("filesystem"),

            "execute_command" | "read_output" | "send_input" | "force_terminate"
            | "list_sessions" => Some("terminal"),

            FS_EDIT_BLOCK => Some("edit"),

            FS_SEARCH => Some("search"),

            INSPECT_USAGE_STATS
            | "get_config"
            | "set_config_value"
            | "prompt_get"
            | INSPECT_TOOL_CALLS => Some("config"),

            "list_processes" | "kill_process" => Some("process"),

            _ => None,
        }
    }

    /// Track a successful tool call (fire-and-forget, never blocks)
    pub fn track_success(&self, tool_name: &str) {
        let _ = self
            .update_sender
            .send(StatsUpdate::Success(tool_name.to_string()));
    }

    /// Track a failed tool call (fire-and-forget, never blocks)
    pub fn track_failure(&self, tool_name: &str) {
        let _ = self
            .update_sender
            .send(StatsUpdate::Failure(tool_name.to_string()));
    }

    /// Background task that processes stat updates and batches disk writes
    fn start_background_processor(
        &self,
        mut update_receiver: tokio::sync::mpsc::UnboundedReceiver<StatsUpdate>,
    ) {
        let stats = Arc::clone(&self.stats);
        let stats_file = self.stats_file.clone();

        tokio::spawn(async move {
            // Create directory and load initial stats
            if let Some(parent) = stats_file.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }

            // Load existing stats from disk
            let loaded_stats = Self::load_or_default(&stats_file).await;
            *stats.write() = loaded_stats;

            // Flush stats to disk every 5 seconds
            let mut save_interval = tokio::time::interval(std::time::Duration::from_secs(5));
            let mut has_pending_writes = false;

            loop {
                tokio::select! {
                    // Receive stat update from channel
                    Some(update) = update_receiver.recv() => {
                        // Update in-memory stats immediately
                        {
                            let mut stats_guard = stats.write();
                            let now = chrono::Utc::now().timestamp();

                            // Check if new session (30 min timeout)
                            if Self::is_new_session(stats_guard.last_used) {
                                stats_guard.total_sessions += 1;
                            }

                            // Update common counters
                            stats_guard.total_tool_calls += 1;
                            stats_guard.last_used = now;

                            // Process update type
                            let tool_name = match update {
                                StatsUpdate::Success(name) => {
                                    stats_guard.successful_calls += 1;
                                    name
                                }
                                StatsUpdate::Failure(name) => {
                                    stats_guard.failed_calls += 1;
                                    name
                                }
                            };

                            // Update tool-specific counter
                            *stats_guard.tool_counts.entry(tool_name.clone()).or_insert(0) += 1;

                            // Update category counter
                            if let Some(category) = Self::get_category(&tool_name) {
                                match category {
                                    "filesystem" => stats_guard.filesystem_operations += 1,
                                    "terminal" => stats_guard.terminal_operations += 1,
                                    "edit" => stats_guard.edit_operations += 1,
                                    "search" => stats_guard.search_operations += 1,
                                    "config" => stats_guard.config_operations += 1,
                                    "process" => stats_guard.process_operations += 1,
                                    _ => {}
                                }
                            }
                        }

                        has_pending_writes = true;
                    }

                    // Periodic disk flush (every 5 seconds)
                    _ = save_interval.tick() => {
                        if has_pending_writes {
                            // Serialize and write stats to disk
                            let json = {
                                let stats_guard = stats.read();
                                match serde_json::to_string_pretty(&*stats_guard) {
                                    Ok(j) => j,
                                    Err(e) => {
                                        log::error!("Failed to serialize usage stats: {e}");
                                        continue;
                                    }
                                }
                            };

                            if let Err(e) = tokio::fs::write(&stats_file, json).await {
                                log::error!("Failed to write usage stats to {}: {}",
                                    stats_file.display(), e);
                            }

                            has_pending_writes = false;
                        }
                    }

                    // Channel closed (server shutdown)
                    else => {
                        // Final flush before exit
                        if has_pending_writes {
                            let json = {
                                let stats_guard = stats.read();
                                serde_json::to_string_pretty(&*stats_guard).unwrap_or_default()
                            };
                            let _ = tokio::fs::write(&stats_file, json).await;
                        }
                        break;
                    }
                }
            }
        });
    }

    /// Get formatted summary for display
    #[must_use]
    pub fn get_summary(&self) -> String {
        let stats = self.stats.read();
        let uptime = self.session_start.elapsed().as_secs();

        let success_rate = if stats.total_tool_calls > 0 {
            f64::from(u32::try_from(stats.successful_calls).unwrap_or(u32::MAX))
                / f64::from(u32::try_from(stats.total_tool_calls).unwrap_or(u32::MAX))
                * 100.0
        } else {
            0.0
        };

        let failure_rate = if stats.total_tool_calls > 0 {
            f64::from(u32::try_from(stats.failed_calls).unwrap_or(u32::MAX))
                / f64::from(u32::try_from(stats.total_tool_calls).unwrap_or(u32::MAX))
                * 100.0
        } else {
            0.0
        };

        // Get top 10 tools
        let mut sorted: Vec<_> = stats.tool_counts.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));
        let top_tools = sorted
            .iter()
            .take(10)
            .map(|(name, count)| format!("  - {name}: {count}"))
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            "Usage Statistics:\n\n\
             Total Tool Calls: {}\n\
             Successful: {} ({:.1}%)\n\
             Failed: {} ({:.1}%)\n\n\
             Operations by Category:\n\
             - Filesystem: {}\n\
             - Terminal: {}\n\
             - Edit: {}\n\
             - Search: {}\n\
             - Config: {}\n\
             - Process: {}\n\n\
             Total Sessions: {}\n\
             Session Uptime: {}s\n\
             First Used: {}\n\
             Last Used: {}\n\n\
             Top Tools:\n{}\n",
            stats.total_tool_calls,
            stats.successful_calls,
            success_rate,
            stats.failed_calls,
            failure_rate,
            stats.filesystem_operations,
            stats.terminal_operations,
            stats.edit_operations,
            stats.search_operations,
            stats.config_operations,
            stats.process_operations,
            stats.total_sessions,
            uptime,
            Self::format_timestamp(stats.first_used),
            Self::format_timestamp(stats.last_used),
            if top_tools.is_empty() {
                "  (none yet)"
            } else {
                &top_tools
            }
        )
    }

    /// Get formatted summary with ANSI colors and Nerd Font icons (2-line output)
    #[must_use]
    pub fn get_formatted_summary(&self) -> String {
        let stats = self.stats.read();

        // Calculate counts needed for display
        let total_calls = stats.total_tool_calls;
        let unique_tools = stats.tool_counts.len();
        let error_count = stats.failed_calls;

        // Format with magenta color only on line 1, using Nerd Font icons
        format!(
            "\x1b[35m󰊢 Usage Statistics\x1b[0m\n\
             󰋼 Total calls: {} · Unique tools: {} · Errors: {}",
            total_calls,
            unique_tools,
            error_count
        )
    }

    /// Get a snapshot of current usage statistics
    #[must_use]
    pub fn get_stats(&self) -> UsageStats {
        self.stats.read().clone()
    }

    fn format_timestamp(timestamp: i64) -> String {
        chrono::DateTime::from_timestamp(timestamp, 0).map_or_else(
            || "Unknown".to_string(),
            |dt| dt.format("%Y-%m-%d %H:%M:%S").to_string(),
        )
    }
}
