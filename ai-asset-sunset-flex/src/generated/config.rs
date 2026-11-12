use serde::Deserialize;
#[derive(Deserialize, Clone, Debug)]
pub struct Brownouts0Config {
    #[serde(alias = "durationMinutes")]
    pub duration_minutes: i64,
    #[serde(alias = "start")]
    pub start: String,
}
#[derive(Deserialize, Clone, Debug)]
pub struct Exemptions0Config {
    #[serde(alias = "clientId")]
    pub client_id: String,
    #[serde(alias = "reason")]
    pub reason: Option<String>,
    #[serde(alias = "until")]
    pub until: String,
}
#[derive(Deserialize, Clone, Debug)]
pub struct Config {
    #[serde(alias = "annotateToolDescriptions")]
    pub annotate_tool_descriptions: Option<bool>,
    #[serde(alias = "brownouts")]
    pub brownouts: Option<Vec<Brownouts0Config>>,
    #[serde(alias = "cacheClampSeconds")]
    pub cache_clamp_seconds: Option<i64>,
    #[serde(alias = "contactEmail")]
    pub contact_email: Option<String>,
    #[serde(alias = "contactName")]
    pub contact_name: Option<String>,
    #[serde(alias = "deprecationAt")]
    pub deprecation_at: String,
    #[serde(alias = "disclosure")]
    pub disclosure: Option<String>,
    #[serde(alias = "docsUrl")]
    pub docs_url: Option<String>,
    #[serde(alias = "emitViolation")]
    pub emit_violation: Option<bool>,
    #[serde(alias = "exemptions")]
    pub exemptions: Option<Vec<Exemptions0Config>>,
    #[serde(alias = "identifyYourselfAt")]
    pub identify_yourself_at: Option<String>,
    #[serde(alias = "injectBodyMetadata")]
    pub inject_body_metadata: Option<bool>,
    #[serde(alias = "llmTombstoneStyle")]
    pub llm_tombstone_style: Option<String>,
    #[serde(alias = "maxBodyRewriteBytes")]
    pub max_body_rewrite_bytes: Option<i64>,
    #[serde(alias = "mcpTombstoneStyle")]
    pub mcp_tombstone_style: Option<String>,
    #[serde(alias = "message")]
    pub message: Option<String>,
    #[serde(alias = "protocol")]
    pub protocol: Option<String>,
    #[serde(alias = "successorName")]
    pub successor_name: Option<String>,
    #[serde(alias = "successorUrl")]
    pub successor_url: Option<String>,
    #[serde(alias = "sunsetAt")]
    pub sunset_at: String,
    #[serde(alias = "switchOffAt")]
    pub switch_off_at: String,
    #[serde(alias = "taskGraceMinutes")]
    pub task_grace_minutes: Option<i64>,
}
#[pdk::hl::entrypoint_flex]
fn init(abi: &dyn pdk::flex_abi::api::FlexAbi) -> Result<(), anyhow::Error> {
    abi.setup()?;
    Ok(())
}
