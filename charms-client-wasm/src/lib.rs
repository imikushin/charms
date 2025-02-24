use wasm_bindgen::prelude::*;

use charms_client;

#[wasm_bindgen]
pub fn extract_and_verify_spell(tx: JsValue, spell_vk: &str) -> anyhow::Result<JsValue, JsValue> {
    let tx =
        serde_wasm_bindgen::from_value(tx).map_err(|e| JsValue::from_str(&format!("{}", e)))?;
    let spell = charms_client::tx::extract_and_verify_spell(&tx, spell_vk)
        .map_err(|e| JsValue::from_str(&format!("{}", e)))?;
    let spell =
        serde_wasm_bindgen::to_value(&spell).map_err(|e| JsValue::from_str(&format!("{}", e)))?;
    Ok(spell)
}

#[cfg(test)]
mod test {
    #[test]
    fn dummy() {}
}
