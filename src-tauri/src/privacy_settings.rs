//! Explicit allowlist of persisted device-related keys; keep general preferences.
pub fn is_device_key(key: &str) -> bool {
    key == "last_device" || key == "audio_options" || key.starts_with("receiver:")
}
#[cfg(test)]
mod tests {
    #[test] fn clears_device_identifiers_but_preserves_general_preferences() {
        let mut values = serde_json::json!({
            "last_device":{"ip":"192.168.1.2","name":"test","receiver_id":"fixture"},
            "receiver:fixture":{"volume":0.2},"audio_options":{"device_id":"fixture"},
            "volume":0.3,"playback_mode":"balanced","ui_language":"ja","auto_connect":true
        }).as_object().unwrap().clone();
        values.retain(|key,_| !super::is_device_key(key));
        values.insert("auto_connect".into(),serde_json::json!(false));
        assert_eq!(values.len(),4);
        assert_eq!(values["volume"],0.3);
        assert_eq!(values["playback_mode"],"balanced");
        assert_eq!(values["ui_language"],"ja");
        assert_eq!(values["auto_connect"],false);
        assert!(!serde_json::to_string(&values).unwrap().contains("fixture"));
        // Round-trip the persisted representation: a reload must not resurrect deleted keys.
        let persisted=serde_json::to_vec(&values).unwrap();
        let reloaded:serde_json::Map<String,serde_json::Value>=serde_json::from_slice(&persisted).unwrap();
        assert_eq!(reloaded,values);
        assert!(!reloaded.keys().any(|key|super::is_device_key(key)));
    }
}
