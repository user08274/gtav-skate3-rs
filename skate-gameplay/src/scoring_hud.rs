//! Trick-label localisation from the upstream scoring HUD (rendering not ported).
pub fn localize_trick(label: &str, assets: Option<&crate::apt_text::TextAssets>) -> String {
    if let Some(literal) = label.strip_prefix('#') {
        return literal.to_owned();
    }
    label
        .split_whitespace()
        .map(|part| {
            let text = assets
                .map(|a| a.localize(part))
                .unwrap_or_else(|| part.to_owned());
            if text.starts_with("ID_") {
                humanize_trick_id(&text)
            } else {
                text
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn humanize_trick_id(id: &str) -> String {
    let rest = id
        .strip_prefix("ID_TRICK_")
        .or_else(|| id.strip_prefix("ID_"))
        .unwrap_or(id);
    rest.split('_')
        .filter(|word| !word.is_empty())
        .map(|word| {
            let lower = word.to_ascii_lowercase();
            let mut chars = lower.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
