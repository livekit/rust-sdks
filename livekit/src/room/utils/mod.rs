use std::collections::HashMap;

pub mod take_cell;
pub(crate) mod ttl_map;
pub(crate) mod tx_queue;
pub mod utf8_chunk;

pub fn calculate_changed_attributes(
    old_attributes: HashMap<String, String>,
    new_attributes: HashMap<String, String>,
) -> HashMap<String, String> {
    let old_keys = old_attributes.keys();
    let new_keys = new_attributes.keys();
    let all_keys: Vec<_> = old_keys.chain(new_keys).collect();

    let mut changed: HashMap<String, String> = HashMap::new();
    for key in all_keys {
        let old_value = old_attributes.get(key);
        let new_value = new_attributes.get(key);

        if old_value != new_value {
            match new_value {
                Some(new_value) => {
                    changed.insert(key.clone(), new_value.clone());
                }
                None => {
                    changed.insert(key.clone(), String::new());
                }
            }
        }
    }
    changed
}
