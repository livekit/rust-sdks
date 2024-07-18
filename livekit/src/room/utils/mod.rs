use std::collections::HashMap;

pub fn calculate_changed_attributes(
    old_attributes: HashMap<String, String>,
    new_attributes: HashMap<String, String>,
) -> HashMap<String, String> {
    let old_keys: Vec<_> = old_attributes.keys().collect();
    let new_keys: Vec<_> = new_attributes.keys().collect();
    let all_keys: Vec<_> = old_keys.into_iter().chain(new_keys.into_iter()).collect();

    let mut changed: HashMap<String, String> = HashMap::new();
    for key in all_keys {
        if old_attributes.get(key) != new_attributes.get(key) {
            if let Some(new_value) = new_attributes.get(key).cloned() {
                changed.insert(key.clone(), new_value);
            }
        }
    }
    return changed;
}
