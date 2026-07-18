use std::collections::HashSet;

use super::ToolDescriptor;

pub(super) fn capability_requested(
    message: &str,
    descriptor: &ToolDescriptor,
    explicitly_requested: &HashSet<&str>,
) -> bool {
    explicitly_requested.contains(descriptor.name.as_str())
        || descriptor
            .triggers
            .iter()
            .any(|trigger| message.contains(&trigger.to_lowercase()))
}

pub(super) fn explicitly_requested_capabilities(message: &str) -> HashSet<&str> {
    message
        .match_indices('@')
        .filter_map(|(index, _)| {
            let token = message[index + 1..]
                .split(|character: char| {
                    !(character.is_ascii_lowercase()
                        || character.is_ascii_digit()
                        || character == '_')
                })
                .next()
                .unwrap_or_default();
            (!token.is_empty()).then_some(token)
        })
        .collect()
}
