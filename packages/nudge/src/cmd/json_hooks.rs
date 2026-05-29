use color_eyre::eyre::{Result, bail};
use serde_json::{Map, Value, json};

pub(crate) fn hooks_object<'a>(
    root: &'a mut Value,
    root_label: &str,
) -> Result<&'a mut Map<String, Value>> {
    let Value::Object(root) = root else {
        bail!("expected {root_label} to be an object, got: {root:?}");
    };

    let hooks = root.entry("hooks").or_insert_with(|| json!({}));
    let Value::Object(hooks) = hooks else {
        bail!("expected hooks to be an object, got: {hooks:?}");
    };

    Ok(hooks)
}

pub(crate) fn merge_hooks(
    hooks: &mut Map<String, Value>,
    desired_hooks: impl IntoIterator<Item = (&'static str, Value)>,
) -> Result<()> {
    for (event, matcher) in desired_hooks {
        let entry = hooks.entry(event).or_insert_with(|| json!([]));
        let Value::Array(matchers) = entry else {
            bail!("expected hook matchers to be an array, got: {entry:?}");
        };

        if !matchers.contains(&matcher) {
            matchers.push(matcher);
        }
    }

    Ok(())
}
