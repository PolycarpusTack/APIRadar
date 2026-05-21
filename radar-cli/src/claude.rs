/// D-1: Generate a targeted, per-consumer migration guide.
/// Returns None if no AI provider is configured (graceful degradation).
pub async fn generate_consumer_narrative(
    consumer_name: &str,
    owner_team: &str,
    breaking_changes: &str,
) -> Option<String> {
    let prompt = format!(
        "You are a technical writer for an API platform team. \
         Write a targeted, actionable migration guide (3-5 sentences) for the team '{owner_team}' \
         that maintains the service '{consumer_name}'. They consume an API that has the following \
         breaking changes and must update their integration.\n\n\
         Breaking changes:\n{breaking_changes}\n\n\
         Be specific: describe exactly what they need to change in their code, what new fields or \
         endpoints to use, and any backwards-compatibility workarounds available."
    );
    crate::ai_provider::complete(&prompt, 512).await
}
