// Generated shape of gcl.yaml (regenerate with `make build-asset-files` → cargo anypoint config-gen).
// Kept in sync by hand for review; the generator output replaces this file.
use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
pub struct Brownouts0Config {
    #[serde(alias = "start")]
    pub start: String,
    #[serde(alias = "durationMinutes")]
    pub duration_minutes: i64,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Exemptions0Config {
    #[serde(alias = "clientId")]
    pub client_id: String,
    #[serde(alias = "until")]
    pub until: String,
    #[serde(alias = "reason")]
    pub reason: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Config {
    #[serde(alias = "deprecationAt")]
    pub deprecation_at: String,
    #[serde(alias = "sunsetAt")]
    pub sunset_at: String,
    #[serde(alias = "switchOffAt")]
    pub switch_off_at: String,
    #[serde(alias = "message")]
    pub message: Option<String>,
    #[serde(alias = "successorName")]
    pub successor_name: Option<String>,
    #[serde(alias = "successorUrl")]
    pub successor_url: Option<String>,
    #[serde(alias = "docsUrl")]
    pub docs_url: Option<String>,
    #[serde(alias = "contactName")]
    pub contact_name: Option<String>,
    #[serde(alias = "contactEmail")]
    pub contact_email: Option<String>,
    #[serde(alias = "protocol")]
    pub protocol: Option<String>,
    #[serde(alias = "mcpTombstoneStyle")]
    pub mcp_tombstone_style: Option<String>,
    #[serde(alias = "llmTombstoneStyle")]
    pub llm_tombstone_style: Option<String>,
    #[serde(alias = "disclosure")]
    pub disclosure: Option<String>,
    #[serde(alias = "injectBodyMetadata")]
    pub inject_body_metadata: Option<bool>,
    #[serde(alias = "annotateToolDescriptions")]
    pub annotate_tool_descriptions: Option<bool>,
    #[serde(alias = "maxBodyRewriteBytes")]
    pub max_body_rewrite_bytes: Option<i64>,
    #[serde(alias = "taskGraceMinutes")]
    pub task_grace_minutes: Option<i64>,
    #[serde(alias = "identifyYourselfAt")]
    pub identify_yourself_at: Option<String>,
    #[serde(alias = "brownouts")]
    pub brownouts: Option<Vec<Brownouts0Config>>,
    #[serde(alias = "exemptions")]
    pub exemptions: Option<Vec<Exemptions0Config>>,
    #[serde(alias = "emitViolation")]
    pub emit_violation: Option<bool>,
    #[serde(alias = "cacheClampSeconds")]
    pub cache_clamp_seconds: Option<i64>,
}

#[pdk::hl::entrypoint_flex]
fn init(abi: &dyn pdk::flex_abi::api::FlexAbi) -> Result<(), anyhow::Error> {
    abi.setup()?;
    Ok(())
}
