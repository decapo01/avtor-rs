

pub fn field_names_without_id(fields: &[&str]) -> Vec<String> {
    fields
        .iter()
        .map(|x| x.to_string())
        .filter(|x| x != &"id".to_string())
        .collect()
}
