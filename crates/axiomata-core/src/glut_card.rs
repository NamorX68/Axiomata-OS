//! A Studio session on Glut (`docs/plans/glut-cards.md`): what the studio writes into the session's channel at every
//! start, and what `axiomata-cli harness card` reads back in the pane.
//!
//! The pane only types `axiomata-cli harness card`; everything the run is told — its folder, model, rights, the team
//! server, its instructions and its first message — is in [`CardSpec`], a `0600` file in the session's own channel
//! (`~/.axiomata/agent-events/<id>/`), which Glut's sandbox hides from every command the session runs. Nothing of it is
//! on a command line, and the CLI finds the file by the session's id alone, so a session cannot point it at another.

use std::path::{Path, PathBuf};

use axiomata_harness::permissions::Mode;
use axiomata_ide::lifecycle::{Channel, ChannelRoots};
use serde::{Deserialize, Serialize};

use crate::AxiomataError;

/// The run's spec in the channel.
pub const SPEC_FILE: &str = "glut-card.json";
/// The run's conversation in the channel, kept across the turns of one session (C7).
pub const CONVERSATION_FILE: &str = "glut-conversation.json";
/// Every event of the session's runs as JSON lines, for the meter and the Flow tiles (C6).
pub const EVENTS_FILE: &str = "glut-events.jsonl";
/// A spec larger than this was not written by the studio.
pub const MAX_SPEC_BYTES: u64 = 512 * 1024;
/// A saved conversation larger than this is not read back: it was not written by a run whose results are bounded and
/// whose older steps are compacted (Glut lean, Glut robust).
pub const MAX_CONVERSATION_BYTES: u64 = 32 * 1024 * 1024;

/// Everything a card session's run on Glut is started with. Written by the studio only ([`write`]), read by the CLI
/// in the session's pane ([`read`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardSpec {
    /// The session; the CLI refuses a spec of another one than its environment names.
    pub agent_id: i64,
    /// The folder the run works in: the session's worktree (or its read-only checkout).
    pub cwd: PathBuf,
    /// The engine's model; `None` takes the chat provider's chat model.
    pub model: Option<String>,
    /// The permission mode, as Glut spells it (`accept_edits`, `default`, `auto`).
    pub mode: String,
    /// What the run may do without asking.
    pub allow: Vec<String>,
    /// What the run is refused whatever else says so.
    pub deny: Vec<String>,
    /// The team server's configuration (`{"mcpServers": …}`), in the channel beside this file.
    pub mcp_config: PathBuf,
    /// The session's instructions: the team tools and its role.
    pub instructions: String,
    /// The first message, when the studio started the session for something (a card, a plan); `None` for a session
    /// the owner talks to from the first line.
    pub prompt: Option<String>,
    /// Whether the folder's `AGENTS.md` joins the instructions; not for a reviewer, whose folder the worker wrote.
    #[serde(default = "yes")]
    pub project_guide: bool,
}

fn yes() -> bool {
    true
}

impl CardSpec {
    /// The mode, read as Glut reads it.
    ///
    /// # Errors
    ///
    /// [`AxiomataError::InvalidAgentModel`] for a mode Glut does not know — only a spec not written by [`write`].
    pub fn mode(&self) -> Result<Mode, AxiomataError> {
        Mode::parse(&self.mode).ok_or_else(|| AxiomataError::InvalidAgentModel {
            reason: format!("the session's spec names an unknown mode {:?}", self.mode),
        })
    }
}

/// Writes `spec` into its session's channel, readable by the owner only.
///
/// # Errors
///
/// The channel cannot be written.
pub fn write(roots: &ChannelRoots, spec: &CardSpec) -> Result<PathBuf, AxiomataError> {
    let channel = Channel::for_agent(roots, spec.agent_id);
    let text =
        serde_json::to_string_pretty(spec).map_err(|err| AxiomataError::InvalidAgentModel {
            reason: format!("the session's spec cannot be written down: {err}"),
        })?;
    channel.write_private(SPEC_FILE, &format!("{text}\n"))?;
    Ok(channel.dir().join(SPEC_FILE))
}

/// Forgets what an earlier start of session `agent_id` on Glut left in its channel — its spec, conversation and turn
/// journals — when it starts on another harness: nothing may run on a spec its session no longer stands for. The event
/// log stays, it is what the session used.
pub fn forget(roots: &ChannelRoots, agent_id: i64) {
    let dir = Channel::for_agent(roots, agent_id).dir().to_path_buf();
    for name in [SPEC_FILE, CONVERSATION_FILE] {
        let _ = std::fs::remove_file(dir.join(name));
    }
    let _ = std::fs::remove_dir_all(dir.join(JOURNAL_DIR));
}

/// The folder of a session's turn journals, in its channel (C8).
pub const JOURNAL_DIR: &str = "glut-runs";
/// The lock one pane holds for its session while it runs: a second `harness card` of the same session is refused.
pub const LOCK_FILE: &str = "glut.lock";

/// Reads session `agent_id`'s spec from its channel: a regular file, not a link, of a bounded size.
///
/// # Errors
///
/// No spec (the session was not started by the studio on Glut), one that is not a plain file, or one that does not
/// parse or names another session.
pub fn read(roots: &ChannelRoots, agent_id: i64) -> Result<CardSpec, AxiomataError> {
    let path = Channel::for_agent(roots, agent_id).dir().join(SPEC_FILE);
    let text = read_private(&path, MAX_SPEC_BYTES)?;
    let spec: CardSpec =
        serde_json::from_str(&text).map_err(|err| refusal(&path, &err.to_string()))?;
    if spec.agent_id != agent_id {
        return Err(refusal(&path, "it belongs to another session"));
    }
    Ok(spec)
}

/// The text of a file of the channel, at most `max_bytes` of it, opened without following a link — the channel is the
/// session's to reach on other harnesses, so a link put in place of the file must not lead the read elsewhere.
///
/// # Errors
///
/// The file cannot be opened, is no regular file, is larger than `max_bytes` or is no text.
pub fn read_private(path: &Path, max_bytes: u64) -> Result<String, AxiomataError> {
    use rustix::fs::OFlags;
    use std::io::Read as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags((OFlags::NOFOLLOW | OFlags::NONBLOCK).bits() as i32)
        .open(path)
        .map_err(|err| refusal(path, &err.to_string()))?;
    let meta = file
        .metadata()
        .map_err(|err| refusal(path, &err.to_string()))?;
    if !meta.is_file() {
        return Err(refusal(path, "not a regular file"));
    }
    let mut text = String::new();
    file.take(max_bytes + 1)
        .read_to_string(&mut text)
        .map_err(|err| refusal(path, &err.to_string()))?;
    if text.len() as u64 > max_bytes {
        return Err(refusal(path, &format!("larger than {max_bytes} bytes")));
    }
    Ok(text)
}

/// The model endpoint of a Glut session: the chat provider's base URL and key, the engine's model or else the chat
/// provider's chat model, and the chat's effort.
///
/// # Errors
///
/// [`AxiomataError::InvalidAgentModel`] as for a Glut chat: no provider with a base URL, no key for OpenRouter, no
/// model.
pub fn endpoint(
    config: &crate::config::Config,
    model: Option<String>,
) -> Result<axiomata_harness::ModelEndpoint, AxiomataError> {
    let role = crate::config::ProviderRole::Chat;
    let provider = config.agents.provider_for(role);
    // An engine's model written as Opencode spells it (`openrouter/<model>`) means the same model to Glut, which
    // speaks to the provider directly.
    let opencode_prefix = match provider {
        crate::config::ProviderId::OpenRouter => Some("openrouter/"),
        crate::config::ProviderId::Ollama => Some("ollama/"),
        _ => None,
    };
    let model = model.map(|model| {
        match opencode_prefix.and_then(|prefix| model.trim().strip_prefix(prefix)) {
            Some(bare) => bare.to_owned(),
            None => model,
        }
    });
    let model = model.filter(|model| !model.trim().is_empty()).or_else(|| {
        config
            .agents
            .providers
            .get(&provider)
            .map(|settings| settings.chat_model.clone())
    });
    crate::agents::axh::endpoint(config, role, model, None)
}

fn refusal(path: &Path, why: &str) -> AxiomataError {
    crate::roster::refusal(
        "Glut session",
        format!("{} cannot be used: {why}", path.display()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    /// A fresh folder of its own for one test.
    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "axiomata-glut-card-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn roots(dir: &Path) -> ChannelRoots {
        ChannelRoots {
            events: dir.join("events"),
            claude_tasks: dir.join("tasks"),
            claude_plans: dir.join("plans"),
        }
    }

    fn spec(agent_id: i64) -> CardSpec {
        CardSpec {
            agent_id,
            cwd: "/tmp/w".into(),
            model: Some("deepseek/deepseek-v4.1-flash".into()),
            mode: "accept_edits".into(),
            allow: vec!["mcp__axiomata__report_done".into()],
            deny: vec!["Bash(git push)".into()],
            mcp_config: "/tmp/mcp.json".into(),
            instructions: "You are the session".into(),
            prompt: Some("Work on card #3".into()),
            project_guide: true,
        }
    }

    #[test]
    fn a_written_spec_reads_back_and_is_private() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = temp_dir();
        let roots = roots(&dir);
        let path = write(&roots, &spec(7)).unwrap();
        assert_eq!(read(&roots, 7).unwrap(), spec(7));
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(read(&roots, 7).unwrap().mode().unwrap(), Mode::AcceptEdits);
    }

    #[test]
    fn another_sessions_spec_a_link_or_no_spec_is_refused() {
        let dir = temp_dir();
        let roots = roots(&dir);
        assert!(read(&roots, 8).is_err(), "no spec");
        // A spec copied into another session's channel still names its own session.
        let path = write(&roots, &spec(7)).unwrap();
        let other = Channel::for_agent(&roots, 8);
        std::fs::create_dir_all(other.dir()).unwrap();
        std::fs::copy(&path, other.dir().join(SPEC_FILE)).unwrap();
        assert!(read(&roots, 8).is_err(), "another session's spec");
        // A link in place of the file is not followed.
        let linked = Channel::for_agent(&roots, 9);
        std::fs::create_dir_all(linked.dir()).unwrap();
        std::os::unix::fs::symlink(&path, linked.dir().join(SPEC_FILE)).unwrap();
        assert!(read(&roots, 9).is_err(), "a link");
    }

    #[test]
    fn the_endpoint_is_the_chat_providers_and_takes_an_opencode_spelling_of_its_model() {
        let mut config = crate::config::Config::default();
        config.agents.chat_provider = crate::config::ProviderId::OpenRouter;
        config.agents.providers.insert(
            crate::config::ProviderId::OpenRouter,
            crate::config::ProviderSettings {
                base_url: Some("https://openrouter.ai/api".into()),
                api_key: Some("sk-or".into()),
                chat_model: "chat/default".into(),
                skill_model: "skill/model".into(),
            },
        );
        let named = endpoint(
            &config,
            Some("openrouter/deepseek/deepseek-v4.1-flash".into()),
        )
        .unwrap();
        assert_eq!(named.model, "deepseek/deepseek-v4.1-flash");
        assert_eq!(named.base_url, "https://openrouter.ai/api");
        assert_eq!(endpoint(&config, None).unwrap().model, "chat/default");
        assert_eq!(
            endpoint(&config, Some(" ".into())).unwrap().model,
            "chat/default"
        );
    }
}
