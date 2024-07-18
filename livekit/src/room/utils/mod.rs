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
        let old_value = old_attributes.get(key).cloned().unwrap_or_else(String::new);
        let new_value = new_attributes.get(key).cloned().unwrap_or_else(String::new);
        if old_value != new_value {
            changed.insert(key.clone(), new_value.clone());
        }
    }
    return changed;
}
